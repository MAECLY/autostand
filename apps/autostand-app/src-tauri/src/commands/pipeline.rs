//! Pipeline + scheduler IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `compile_standup`, `compile_all`,
//! `trigger_run_now`, `get_pipeline_status`, `preview_gather`,
//! `get_scheduler_status`, `set_scheduler_schedule`.
//!
//! The gather pipeline is not yet wired, so `compile_*` emit the right events
//! and return an error `CompileResult` with `message: "gather pipeline not yet wired"`.

use chrono::{Local, NaiveDate};
use tauri::{AppHandle, Emitter, State};

use crate::commands::types::{
    CompileResult, CompileStatus, GatherPreview, LastTrigger, PipelineProgress, RenderUsed,
    SchedulerSource, SchedulerStatus,
};
use crate::error::AppError;
use crate::state::AppState;

/// Date format used by every `date` argument and DTO field in the contract.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Progress percent reported once the run has a date + host but no facts yet.
const INIT_PERCENT: u8 = 5;

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

/// Run the full pipeline for a single date (default: today).
///
/// Emits `pipeline-started`, a `pipeline-progress` (step `init`), then — since
/// gather is not wired — `pipeline-error` (step `gather`, code `io`) and returns
/// a `CompileResult { status: error, message: "gather pipeline not yet wired" }`.
#[tauri::command]
pub async fn compile_standup(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    date: Option<String>,
) -> Result<CompileResult, AppError> {
    let resolved = resolve_date(date.as_deref())?;
    let date_str = resolved.format(DATE_FORMAT).to_string();

    let host = autostand_core::host::load_or_detect(&super::state_dir())
        .unwrap_or_else(|_| "unknown-host".to_string());

    state.set_state(
        date_str.clone(),
        host.clone(),
        crate::state::PipelineStateKind::Gathering,
        "gather".to_string(),
    );
    let _ = app_handle.emit(
        "pipeline-started",
        PipelineStarted {
            date: date_str.clone(),
            host: host.clone(),
            trigger: LastTrigger::Manual,
        },
    );
    state.set_percent(INIT_PERCENT, "init".to_string());
    let _ = app_handle.emit(
        "pipeline-progress",
        PipelineProgress {
            date: date_str.clone(),
            host: host.clone(),
            step: "init".into(),
            percent: INIT_PERCENT,
        },
    );

    let message = "gather pipeline not yet wired".to_string();
    state.set_error(message.clone());
    let _ = app_handle.emit(
        "pipeline-error",
        PipelineError {
            code: "io".into(),
            message: message.clone(),
            step: "gather".into(),
            date: date_str.clone(),
        },
    );

    let result = CompileResult {
        date: date_str,
        host,
        status: CompileStatus::Error,
        render_used: RenderUsed::Det,
        fellback: false,
        audit_path: None,
        file_path: String::new(),
        accumulated_count: 0,
        message,
    };
    Ok(result)
}

/// Recompile F_TODAY + F_PREV (business-day aware).
#[tauri::command]
pub async fn compile_all(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<Vec<CompileResult>, AppError> {
    let today = Local::now().date_naive();
    let (f_today, f_prev) = autostand_scheduler::selfheal::compute_targets(today);
    let mut results = Vec::with_capacity(2);
    for d in [f_today, f_prev] {
        let s = d.format(DATE_FORMAT).to_string();
        let r = compile_standup(app_handle.clone(), state.clone(), Some(s))
            .await
            .unwrap_or_default();
        results.push(r);
    }
    Ok(results)
}

/// Trigger a run immediately.
#[tauri::command]
pub async fn trigger_run_now(
    app_handle: AppHandle,
    state: State<'_, AppState>,
) -> Result<CompileResult, AppError> {
    compile_standup(app_handle, state, None).await
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
/// Gather is not wired yet — returns the right shape with empty fields.
#[tauri::command]
pub async fn preview_gather(
    _app_handle: AppHandle,
    date: String,
) -> Result<GatherPreview, AppError> {
    let resolved = resolve_date(Some(&date))?;
    let date_str = resolved.format(DATE_FORMAT).to_string();
    let host = autostand_core::host::load_or_detect(&super::state_dir())
        .unwrap_or_else(|_| "unknown-host".to_string());
    Ok(GatherPreview {
        date: date_str,
        host,
        ..Default::default()
    })
}

/// Get the scheduler status.
///
/// Stub: returns `enabled: false, source: "in-process"` until the
/// scheduler is wired.
#[tauri::command]
pub async fn get_scheduler_status() -> Result<SchedulerStatus, AppError> {
    Ok(SchedulerStatus {
        enabled: false,
        source: SchedulerSource::InProcess,
        cron: "0 7-19 * * 1-5".to_string(),
        next_run_at: None,
        last_run_at: None,
        last_trigger: None,
    })
}

/// Persist a cron schedule to config (does not yet install system units).
#[tauri::command]
pub async fn set_scheduler_schedule(app_handle: AppHandle, cron: String) -> Result<(), AppError> {
    let _ = autostand_scheduler::cron::next_run(&cron, chrono::Utc::now())
        .map_err(|e| AppError::Invalid(format!("invalid cron '{cron}': {e}")))?;
    let mut config = super::load_config(&app_handle)?;
    config.scheduler.cron = cron;
    super::save_config(&app_handle, &config)
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
    use super::{resolve_date, PipelineError, PipelineStarted, DATE_FORMAT, INIT_PERCENT};
    use crate::commands::types::{LastTrigger, PipelineProgress};
    use crate::error::AppError;
    use chrono::{Local, NaiveDate};

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
    fn error_payload_matches_the_event_contract() {
        let value = serde_json::to_value(PipelineError {
            code: "io".to_string(),
            message: "gather pipeline not yet wired".to_string(),
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
}
