//! Pipeline + scheduler IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `compile_standup`, `compile_all`,
//! `trigger_run_now`, `get_pipeline_status`, `preview_gather`,
//! `get_scheduler_status`, `set_scheduler_schedule`.
//!
//! These are thin adapters: every compile goes through
//! [`crate::pipeline_runner::trigger`], which owns the lock, the ordered steps
//! and the `pipeline-started` / `-progress` / `-done` / `-error` events. The
//! scheduler pair delegates to [`crate::scheduler_runtime`].

use chrono::{Local, NaiveDate};
use tauri::{AppHandle, State};

use autostand_scheduler::triggers::TriggerSource;

use crate::commands::types::{
    CompileResult, FilingTarget, GatherPreview, LastTrigger, SchedulerStatus,
};
use crate::error::AppError;
use crate::pipeline_runner;
use crate::state::AppState;

/// Date format used by every `date` argument and DTO field in the contract.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Resolve an optional `YYYY-MM-DD` date argument, defaulting to today.
///
/// Strict on purpose: a mis-parsed date would file work under the wrong day,
/// which anti-backdating cannot undo once the file is written.
fn resolve_date(date: Option<&str>) -> Result<NaiveDate, AppError> {
    match date {
        Some(s) => NaiveDate::parse_from_str(s, DATE_FORMAT)
            .map_err(|e| AppError::Invalid(format!("invalid date '{s}': {e}"))),
        None => Ok(Local::now().date_naive()),
    }
}

/// Reduce a multi-target run to the single `CompileResult` the contract returns.
///
/// The first element is `F_TODAY` (or the explicitly requested date), which is
/// the one the caller asked about; a run that produced nothing at all is
/// reported as an error rather than as a silent success. `host` is lazy because
/// resolving the slug hits the state dir and the empty case never happens in
/// practice.
fn first_result(
    results: Vec<CompileResult>,
    date: NaiveDate,
    host: impl FnOnce() -> String,
) -> CompileResult {
    results.into_iter().next().unwrap_or_else(|| {
        pipeline_runner::error_result(date, &host(), "pipeline produced no result")
    })
}

/// Host slug for a result the pipeline never got far enough to produce one for.
fn fallback_host() -> String {
    autostand_core::host::load_or_detect(&super::state_dir())
        .unwrap_or_else(|_| "unknown-host".to_string())
}

/// Run the full pipeline for a single date (default: today).
///
/// Emits `pipeline-started`, one `pipeline-progress` per step, and
/// `pipeline-done` with the returned `CompileResult`.
#[tauri::command]
pub async fn compile_standup(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    date: Option<String>,
) -> Result<CompileResult, AppError> {
    let resolved = resolve_date(date.as_deref())?;
    let results = pipeline_runner::trigger(
        &app_handle,
        state.inner(),
        TriggerSource::Manual,
        Some(resolved),
    )
    .await?;
    Ok(first_result(results, resolved, fallback_host))
}

/// Recompile `F_TODAY` + `F_PREV` (business-day aware).
#[tauri::command]
pub async fn compile_all(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<CompileResult>, AppError> {
    pipeline_runner::trigger(&app_handle, state.inner(), TriggerSource::Manual, None).await
}

/// Trigger a run immediately (the UI's "Run now" button).
///
/// Same targets as [`compile_all`] — `F_TODAY` plus the self-heal slot — but the
/// contract returns only the primary result.
#[tauri::command]
pub async fn trigger_run_now(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<CompileResult, AppError> {
    let results =
        pipeline_runner::trigger(&app_handle, state.inner(), TriggerSource::Manual, None).await?;
    let today = Local::now().date_naive();
    Ok(first_result(results, today, fallback_host))
}

/// Resolve which standup file a day of work is filed into.
///
/// `date` is a **work day** (default: today), not a filing date — the whole
/// point of the command is to turn one into the other under the configured
/// policy. Read-only and lock-free.
///
/// The dashboard calls this instead of doing the arithmetic itself: the
/// filing rule is what decides which file gets written, and a UI that computed
/// its own answer could label a file the pipeline never touches. That mismatch
/// is precisely how a machine ended up with a `2026-08-13.md` and no
/// `2026-08-14.md`.
#[tauri::command]
pub async fn get_filing_target(
    app_handle: AppHandle,
    date: Option<String>,
) -> Result<FilingTarget, AppError> {
    let work_day = resolve_date(date.as_deref())?;
    let config = super::load_config(&app_handle)?;
    Ok(pipeline_runner::filing_target(
        &config,
        work_day,
        Local::now().date_naive(),
    ))
}

/// Get the current pipeline status snapshot.
#[tauri::command]
pub async fn get_pipeline_status(
    state: State<'_, AppState>,
) -> Result<crate::state::PipelineStatus, AppError> {
    Ok(state.status())
}

/// Preview the gathered FACTS/NOTES/ENRICHMENT for a date (debug UI).
///
/// Read-only: takes no run lock and writes no file, so it is safe to call while
/// a compile is in progress.
#[tauri::command]
pub async fn preview_gather(
    app_handle: AppHandle,
    date: String,
) -> Result<GatherPreview, AppError> {
    let resolved = resolve_date(Some(&date))?;
    pipeline_runner::preview(&app_handle, resolved).await
}

/// Get the scheduler status (next/last run, source, cron).
#[tauri::command]
pub async fn get_scheduler_status(app_handle: AppHandle) -> Result<SchedulerStatus, AppError> {
    crate::scheduler_runtime::status(&app_handle).await
}

/// Persist a cron schedule and reschedule the runtime.
#[tauri::command]
pub async fn set_scheduler_schedule(app_handle: AppHandle, cron: String) -> Result<(), AppError> {
    crate::scheduler_runtime::set_cron(&app_handle, &cron).await
}

/// Enable or disable scheduled runs and reconcile the platform scheduler.
#[tauri::command]
pub async fn set_scheduler_enabled(app_handle: AppHandle, enabled: bool) -> Result<(), AppError> {
    crate::scheduler_runtime::set_enabled(&app_handle, enabled).await
}

/// Payload for `pipeline-started`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineStarted {
    /// Filing date.
    pub date: String,
    /// Host slug.
    pub host: String,
    /// Trigger source; typed so the wire value stays `"scheduled" | "manual" |
    /// "self-heal"` instead of whatever string a caller happens to pass.
    pub trigger: LastTrigger,
}

/// Payload for `pipeline-error`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PipelineError {
    /// Error code (matches the `code` tag `AppError` serializes to).
    pub code: String,
    /// Human-readable message.
    pub message: String,
    /// Step that failed.
    pub step: String,
    /// Filing date.
    pub date: String,
}

#[cfg(test)]
mod tests {
    use super::{first_result, resolve_date, PipelineError, PipelineStarted, DATE_FORMAT};
    use crate::commands::types::{CompileStatus, LastTrigger, PipelineProgress};
    use crate::error::AppError;
    use crate::pipeline_runner::{base_result, Step};
    use chrono::{Local, NaiveDate};
    use std::path::Path;

    /// Progress percent of the first `pipeline-progress` tick a run emits.
    ///
    /// Pinned by the frontend's event fixtures; the test below ties it to
    /// `pipeline_runner::Step::Window` so the two cannot drift.
    const INIT_PERCENT: u8 = 5;

    #[test]
    fn parses_an_explicit_iso_date() {
        assert_eq!(
            resolve_date(Some("2026-08-03")).unwrap(),
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap()
        );
    }

    #[test]
    fn defaults_to_today_when_the_argument_is_omitted() {
        // `compile_standup` takes `date?: string`; omitting it means "today".
        assert_eq!(resolve_date(None).unwrap(), Local::now().date_naive());
    }

    /// Every `date`-taking command routes through [`resolve_date`] —
    /// `compile_standup`, `preview_gather` and `get_filing_target` — so this
    /// table is the rejection contract for all of them.
    #[test]
    fn rejects_malformed_dates() {
        for bad in [
            "",
            "03-08-2026",
            "2026/08/03",
            "2026-08-03T00:00:00Z",
            "2026-08-03 ",
            "today",
            "2026-13-01",
            "2026-02-30",
        ] {
            match resolve_date(Some(bad)) {
                Ok(date) => panic!("{bad:?} must be rejected, parsed as {date}"),
                Err(err) => assert!(matches!(err, AppError::Invalid(_)), "{bad:?} → {err}"),
            }
        }
    }

    #[test]
    fn normalizes_unpadded_month_and_day() {
        // chrono accepts unpadded components; the run must still file under the
        // padded `YYYY-MM-DD` name the rest of the pipeline expects.
        let resolved = resolve_date(Some("2026-8-3")).expect("unpadded date parses");
        assert_eq!(resolved.format(DATE_FORMAT).to_string(), "2026-08-03");
    }

    #[test]
    fn rejection_surfaces_the_invalid_code_and_echoes_the_input() {
        let err = resolve_date(Some("2026-13-01")).expect_err("month 13");
        let value = serde_json::to_value(&err).unwrap();
        assert_eq!(value["code"], serde_json::json!("invalid"));
        assert!(
            value["message"].as_str().unwrap().contains("2026-13-01"),
            "message should name the offending input: {value}"
        );
    }

    #[test]
    fn resolved_dates_round_trip_through_the_contract_format() {
        let resolved = resolve_date(Some("2026-08-03")).unwrap();
        assert_eq!(resolved.format(DATE_FORMAT).to_string(), "2026-08-03");
    }

    #[test]
    fn started_payload_matches_the_event_contract() {
        let value = serde_json::to_value(PipelineStarted {
            date: "2026-08-03".to_string(),
            host: "MacStudio-de-Miguel".to_string(),
            trigger: LastTrigger::SelfHeal,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "date": "2026-08-03",
                "host": "MacStudio-de-Miguel",
                "trigger": "self-heal",
            })
        );
    }

    #[test]
    fn progress_payload_matches_the_event_contract() {
        let value = serde_json::to_value(PipelineProgress {
            date: "2026-08-03".to_string(),
            host: "MacStudio-de-Miguel".to_string(),
            step: "init".to_string(),
            percent: INIT_PERCENT,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "date": "2026-08-03",
                "host": "MacStudio-de-Miguel",
                "step": "init",
                "percent": 5,
            })
        );
    }

    #[test]
    fn the_first_progress_tick_matches_the_runners_first_step() {
        // Regression guard: the two are documented as the same number.
        assert_eq!(INIT_PERCENT, Step::Window.percent());
    }

    #[test]
    fn error_payload_matches_the_event_contract() {
        let value = serde_json::to_value(PipelineError {
            code: "io".to_string(),
            message: "render_llm: provider timed out".to_string(),
            step: "gather".to_string(),
            date: "2026-08-03".to_string(),
        })
        .unwrap();
        let mut keys: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(keys, ["code", "date", "message", "step"]);
    }

    #[test]
    fn the_single_date_commands_return_the_primary_target() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let primary = base_result(date, "host", Path::new("/dailies/2026-08-04.md"));
        let secondary = base_result(
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            "host",
            Path::new("/dailies/2026-08-03.md"),
        );
        let picked = first_result(vec![primary, secondary], date, || "host".to_string());
        assert_eq!(picked.date, "2026-08-04");
    }

    #[test]
    fn an_empty_run_still_returns_an_error_result() {
        let date = NaiveDate::from_ymd_opt(2026, 8, 4).unwrap();
        let picked = first_result(Vec::new(), date, || "host".to_string());
        assert_eq!(picked.status, CompileStatus::Error);
        assert_eq!(picked.date, "2026-08-04");
        assert_eq!(picked.host, "host");
        assert!(!picked.message.is_empty());
    }
}
