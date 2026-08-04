//! In-process scheduler runtime: the 60 s cron tick, the `scheduler-tick` event,
//! and the durable last-run record.
//!
//! See `docs/architecture/04-state-machine.md` (per-run states) and the events
//! table in `docs/tauri/02-ipc-contracts.md`.
//!
//! Two sources coexist. The **in-process** runtime is a tokio task owned by the
//! running app, so it fires only while the app is open; the **system unit**
//! (`launchd` / `systemd --user` / Task Scheduler) is installed by
//! [`autostand_scheduler::install`] and runs `autostand-app --compile` whether
//! or not anything is open. [`status`] reports whichever one
//! [`autostand_scheduler::install::detect`] actually finds, and the two cannot
//! double-compile: they share the run lock and the same durable last-run
//! record, so whichever fires first makes the other's next boundary not due.
//!
//! Installation is deliberately narrow. It happens **only** inside [`set_cron`]
//! — the IPC command `set_scheduler_schedule`, which the contract already
//! defines as "persist cron + reinstall system unit". Never on app start, never
//! from [`status`], never from a test: touching the user's login items is not
//! something a status read is allowed to do.
//!
//! Three rules shape this module:
//!
//! - **Never two runs at once.** The pipeline runner's lock is the guard, so a
//!   `lock` error out of a tick is an expected outcome (a manual run is already
//!   in flight), logged at `info` and *not* recorded as a run.
//! - **Never panic the app.** Each tick runs in its own task and the supervisor
//!   loop logs whatever comes back, so a panic deep in the pipeline costs one
//!   tick rather than the whole scheduler.
//! - **Survive a restart.** The last-run stamp lives in a JSON file under the
//!   state dir; without it, every app start would look like "never ran" and a
//!   missed run could not be told apart from a fresh install.

use std::path::{Path, PathBuf};
use std::time::Duration;

use chrono::{DateTime, SecondsFormat, TimeDelta, Utc};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use autostand_scheduler::install::{self, SchedulerKind};
use autostand_scheduler::{cron, triggers, triggers::TriggerSource};

use crate::commands::types::{LastTrigger, SchedulerConfig, SchedulerSource, SchedulerStatus};
use crate::error::AppError;
use crate::state::AppState;

/// How often the in-process runtime wakes, in seconds.
///
/// The contract (`docs/tauri/02-ipc-contracts.md`, `scheduler-tick` row) pins
/// this at 60 s: one `scheduler-tick` per minute is what the frontend's
/// scheduler-status invalidation is tuned for.
const TICK_SECONDS: u64 = 60;

/// Tick period derived from [`TICK_SECONDS`].
const TICK_INTERVAL: Duration = Duration::from_secs(TICK_SECONDS);

/// How far back [`is_due`] looks when no run was ever recorded.
///
/// Same length as [`TICK_SECONDS`], expressed as a signed delta for chrono.
/// WHY a window rather than "fire immediately": with no history, anchoring at
/// the previous tick means a fresh install fires only when a real cron boundary
/// passes, instead of firing once at every app start regardless of schedule.
const CATCHUP_SECONDS: i64 = 60;

/// Filename of the durable last-run record inside the state dir.
const LAST_RUN_FILE: &str = "scheduler-last-run.json";

/// Payload of the `scheduler-tick` event.
///
/// Field names and types mirror `SchedulerTickEvent` in
/// `apps/autostand-app/src/lib/types.ts`; `source` is the typed enum so the wire
/// value can only ever be one of the five contract strings.
#[derive(Debug, Clone, Serialize)]
pub struct SchedulerTick {
    /// ISO-8601 timestamp of the next scheduled run.
    pub next_run_at: String,
    /// Where the schedule comes from.
    pub source: SchedulerSource,
}

/// Durable record of the most recent run the scheduler knows about.
///
/// Persisted as JSON so a restart does not lose it. `at` is stored as a string
/// (not a `DateTime`) because it is handed straight back to the frontend as
/// `SchedulerStatus.last_run_at`.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct LastRun {
    /// ISO-8601 (UTC, `Z`-suffixed) timestamp of the run.
    at: String,
    /// Trigger that produced the run, in IPC vocabulary.
    trigger: LastTrigger,
}

/// Start the in-process scheduler.
///
/// Spawns a supervisor task that wakes every [`TICK_INTERVAL`], emits
/// `scheduler-tick`, and fires a run through
/// [`crate::pipeline_runner::trigger`] when the persisted cron says one is due.
/// Returns immediately; the task lives as long as the app.
pub fn spawn(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        info!(
            interval_secs = TICK_SECONDS,
            "scheduler: in-process runtime started"
        );
        let mut ticker = tokio::time::interval(TICK_INTERVAL);
        // A tick that overruns its slot (a slow pipeline run) must not be
        // followed by a burst of catch-up ticks: delay, never bunch up.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            // Each tick is its own task so a panic anywhere below (pipeline,
            // adapter, plugin) comes back as a join error instead of unwinding
            // the supervisor and silently stopping the scheduler forever.
            let handle = tauri::async_runtime::spawn({
                let app = app.clone();
                async move { tick(&app).await }
            });
            match handle.await {
                Ok(Ok(())) => {}
                Ok(Err(err)) => warn!(error = %err, "scheduler: tick failed"),
                Err(err) => warn!(error = %err, "scheduler: tick task did not complete"),
            }
        }
    });
}

/// Report the scheduler state: enablement, source, cron, next run, last run.
///
/// # Errors
///
/// Returns [`AppError::Config`] when the config store cannot be read.
pub fn status(app: &AppHandle) -> Result<SchedulerStatus, AppError> {
    let config = crate::commands::load_config(app)?;
    let last = read_last_run(&crate::commands::state_dir());
    let source = resolve_source(install::detect(), config.scheduler.enabled);
    Ok(build_status(&config.scheduler, last, Utc::now(), source))
}

/// Validate `cron`, persist it, and bring the system unit in line with it.
///
/// The contract (`docs/tauri/02-ipc-contracts.md`, `set_scheduler_schedule`)
/// defines this command as "persist cron + reinstall system unit", and this is
/// the *only* place autostand installs one — an explicit user action on the
/// Settings page, never a background side effect.
///
/// # Errors
///
/// [`AppError::Invalid`] when the expression does not parse or can never fire;
/// [`AppError::Config`] when the store cannot be read or written. A failed
/// *install* is not an error: the cron is persisted either way and the
/// in-process runtime still honours it, so refusing to save the user's schedule
/// because `launchctl` was unhappy would be the worse outcome.
pub fn set_cron(app: &AppHandle, cron: &str) -> Result<(), AppError> {
    let normalized = validate_cron(cron)?;
    let mut config = crate::commands::load_config(app)?;
    config.scheduler.cron.clone_from(&normalized);
    let enabled = config.scheduler.enabled;
    crate::commands::save_config(app, &config)?;
    sync_system_unit(&normalized, enabled);
    Ok(())
}

/// Install (or remove) the platform unit so it matches the persisted schedule.
///
/// Best effort by design — see [`set_cron`]. An expression the unit formats
/// cannot express is logged loudly rather than swallowed: the user needs to
/// know their schedule only applies while the app is open.
fn sync_system_unit(cron: &str, enabled: bool) {
    if !enabled {
        if let Err(err) = install::uninstall() {
            info!(error = %err, "scheduler: no system unit to remove");
        }
        return;
    }
    let exe = match std::env::current_exe() {
        Ok(path) => path,
        Err(err) => {
            warn!(error = %err, "scheduler: cannot locate this executable; system unit not installed");
            return;
        }
    };
    match install::install(cron, &exe) {
        Ok(kind) => info!(source = kind.wire_label(), %cron, "scheduler: system unit installed"),
        Err(err) => warn!(
            error = %err,
            "scheduler: could not install the system unit; the in-process runtime still applies while the app is open"
        ),
    }
}

/// Which source `SchedulerStatus` reports.
///
/// A detected unit always wins: it is the thing that will actually fire at
/// 07:00 tomorrow. With nothing installed the answer depends on whether the
/// user turned the scheduler on at all — an enabled schedule is being served by
/// the in-process tick, a disabled one by nothing.
fn resolve_source(installed: SchedulerKind, enabled: bool) -> SchedulerSource {
    match installed {
        SchedulerKind::Launchd => SchedulerSource::Launchd,
        SchedulerKind::Systemd => SchedulerSource::Systemd,
        SchedulerKind::TaskScheduler => SchedulerSource::TaskScheduler,
        SchedulerKind::InProcess => SchedulerSource::InProcess,
        SchedulerKind::None => {
            if enabled {
                SchedulerSource::InProcess
            } else {
                SchedulerSource::None
            }
        }
    }
}

/// Record a run in the durable last-run file (best effort; logs on failure).
///
/// Exposed so the pipeline runner can stamp manual and self-heal runs too —
/// `SchedulerStatus.last_run_at` means "when did this machine last compile",
/// not "when did cron last fire".
pub fn record_run(trigger: LastTrigger) {
    write_run_record(&crate::commands::state_dir(), Utc::now(), trigger);
}

/// Validate a 5-field cron expression and return its trimmed form.
///
/// Validation goes through [`cron::next_run`] rather than [`cron::parse`] on
/// purpose: an expression such as `0 0 30 2 *` parses but matches no instant,
/// and a schedule that can never fire is a silent misconfiguration.
///
/// # Errors
///
/// [`AppError::Invalid`] with the offending expression echoed back.
pub fn validate_cron(cron: &str) -> Result<String, AppError> {
    let trimmed = cron.trim();
    cron::next_run(trimmed, Utc::now())
        .map_err(|e| AppError::Invalid(format!("invalid cron '{trimmed}': {e}")))?;
    Ok(trimmed.to_string())
}

/// Is a run due at `now`, given the schedule and when the last run happened?
///
/// Pure and side-effect free so the tick's decision can be tested without a
/// runtime. The rule is "the first scheduled instant after the last run has
/// already passed", which makes a missed run (app closed over a boundary) fire
/// once on the next tick instead of being lost.
///
/// An expression that does not parse is never due: an invalid cron must not
/// turn into a run every 60 s. `set_cron` rejects those up front, so this only
/// covers a hand-edited config file.
#[must_use]
pub fn is_due(cron_expr: &str, last_run: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
    let anchor = last_run.unwrap_or_else(|| {
        now.checked_sub_signed(TimeDelta::seconds(CATCHUP_SECONDS))
            .unwrap_or(now)
    });
    match cron::next_run(cron_expr, anchor) {
        Ok(next) => next <= now,
        Err(_) => false,
    }
}

/// One scheduler poll: emit `scheduler-tick`, then run if due.
#[tracing::instrument(skip_all)]
async fn tick(app: &AppHandle) -> Result<(), AppError> {
    let config = crate::commands::load_config(app)?;
    let schedule = &config.scheduler;
    let now = Utc::now();

    match cron::next_run(&schedule.cron, now) {
        Ok(next) => {
            let payload = SchedulerTick {
                next_run_at: iso(next),
                source: resolve_source(install::detect(), schedule.enabled),
            };
            if let Err(err) = app.emit("scheduler-tick", payload) {
                warn!(error = %err, "scheduler: could not emit scheduler-tick");
            }
        }
        // No usable `next_run_at` exists, and the frontend types the field as a
        // non-optional ISO string — emitting a placeholder would be a lie.
        Err(err) => warn!(
            cron = %schedule.cron,
            error = %err,
            "scheduler: persisted cron resolves to no next run; skipping scheduler-tick"
        ),
    }

    if !schedule.enabled {
        return Ok(());
    }
    if !triggers::safe_to_fire() {
        // We are inside a render subprocess (AUTOSTAND_RENDER=1). Firing here
        // is how the App Script's infinite render loop used to start.
        return Ok(());
    }

    let state_dir = crate::commands::state_dir();
    let last_run = read_last_run(&state_dir).and_then(|run| parse_iso(&run.at));
    if !is_due(&schedule.cron, last_run, now) {
        return Ok(());
    }

    run_due(app, &state_dir, now).await;
    Ok(())
}

/// Fire a scheduled run and record the outcome.
///
/// A `lock` error means another run (manual, session-end, an earlier tick) is
/// already in flight: that is the guard working, so it is logged at `info` and
/// deliberately *not* recorded, leaving the run due on the next tick. Any other
/// failure **is** recorded, so a broken pipeline is retried on the next cron
/// boundary rather than every 60 s.
#[tracing::instrument(skip_all)]
async fn run_due(app: &AppHandle, state_dir: &Path, now: DateTime<Utc>) {
    let Some(state) = app.try_state::<AppState>() else {
        warn!("scheduler: AppState is not managed; skipping the scheduled run");
        return;
    };
    info!("scheduler: cron boundary passed, starting a scheduled run");
    match crate::pipeline_runner::trigger(app, state.inner(), TriggerSource::Scheduler, None).await
    {
        Ok(_) => write_run_record(state_dir, now, LastTrigger::Scheduled),
        Err(AppError::Lock(message)) => {
            info!(%message, "scheduler: a run already holds the lock; skipping this tick");
        }
        Err(err) => {
            warn!(error = %err, "scheduler: scheduled run failed");
            write_run_record(state_dir, now, LastTrigger::Scheduled);
        }
    }
}

/// Assemble a [`SchedulerStatus`] from config + the persisted last run.
///
/// Split out of [`status`] so the reporting rules are testable without a Tauri
/// app instance. `source` is passed in rather than detected here for the same
/// reason: detection reads the developer's real `~/Library/LaunchAgents`, and a
/// unit test must not depend on what happens to be installed on the machine
/// running it.
fn build_status(
    schedule: &SchedulerConfig,
    last: Option<LastRun>,
    now: DateTime<Utc>,
    source: SchedulerSource,
) -> SchedulerStatus {
    let (last_run_at, last_trigger) = match last {
        Some(run) => (Some(run.at), Some(run.trigger)),
        None => (None, None),
    };
    SchedulerStatus {
        enabled: schedule.enabled,
        source,
        cron: schedule.cron.clone(),
        next_run_at: cron::next_run(&schedule.cron, now).ok().map(iso),
        last_run_at,
        last_trigger,
    }
}

/// Path of the durable last-run record.
fn last_run_path(state_dir: &Path) -> PathBuf {
    state_dir.join(LAST_RUN_FILE)
}

/// Read the persisted last-run record; `None` when absent or unreadable.
///
/// A corrupt file degrades to "never ran" rather than an error: losing the
/// stamp costs at most one extra run, while failing the tick would stop the
/// scheduler until a human deletes the file.
fn read_last_run(state_dir: &Path) -> Option<LastRun> {
    let raw = std::fs::read_to_string(last_run_path(state_dir)).ok()?;
    serde_json::from_str(&raw).ok()
}

/// Persist the last-run record, creating the state dir if needed.
fn write_last_run(state_dir: &Path, run: &LastRun) -> Result<(), AppError> {
    std::fs::create_dir_all(state_dir)?;
    let json = serde_json::to_string_pretty(run)?;
    std::fs::write(last_run_path(state_dir), json)?;
    Ok(())
}

/// Best-effort [`write_last_run`]: a failure here must not fail a run.
fn write_run_record(state_dir: &Path, at: DateTime<Utc>, trigger: LastTrigger) {
    let run = LastRun {
        at: iso(at),
        trigger,
    };
    if let Err(err) = write_last_run(state_dir, &run) {
        warn!(error = %err, "scheduler: could not persist the last-run record");
    }
}

/// Format an instant the way every timestamp on the IPC surface is formatted.
fn iso(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(SecondsFormat::Secs, true)
}

/// Parse an ISO-8601 timestamp written by [`iso`]; `None` if it is not one.
fn parse_iso(raw: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(raw)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

#[cfg(test)]
mod tests {
    use super::{
        build_status, is_due, iso, last_run_path, parse_iso, read_last_run, resolve_source,
        validate_cron, write_last_run, write_run_record, LastRun, SchedulerTick,
    };
    use crate::commands::types::{LastTrigger, SchedulerConfig, SchedulerSource};
    use crate::error::AppError;
    use autostand_scheduler::install::SchedulerKind;
    use chrono::{DateTime, TimeZone, Utc};

    /// Default schedule from `SchedulerConfig::default()`: hourly, 07-19, Mon-Fri.
    const HOURLY_WEEKDAYS: &str = "0 7-19 * * 1-5";

    fn at(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(year, month, day, hour, minute, second)
            .single()
            .expect("unambiguous UTC instant")
    }

    // ---- cron validation ------------------------------------------------

    #[test]
    fn accepts_the_default_schedule() {
        assert_eq!(
            validate_cron(HOURLY_WEEKDAYS).expect("default cron is valid"),
            HOURLY_WEEKDAYS
        );
    }

    #[test]
    fn trims_the_stored_expression() {
        // The Settings field hands us whatever the user typed; storing
        // "  0 * * * *  " would round-trip into the UI with the padding.
        assert_eq!(
            validate_cron("  0 * * * *  ").expect("whitespace is not a syntax error"),
            "0 * * * *"
        );
    }

    #[test]
    fn rejects_malformed_expressions() {
        for bad in [
            "",
            "   ",
            "0 7-19 * *",
            "0 7-19 * * 1-5 6",
            "60 * * * *",
            "* 24 * * *",
            "0 0 * * 7",
            "every hour",
            "*/0 * * * *",
            "0 19-7 * * *",
        ] {
            match validate_cron(bad) {
                Ok(kept) => panic!("{bad:?} must be rejected, kept as {kept:?}"),
                Err(err) => assert!(matches!(err, AppError::Invalid(_)), "{bad:?} → {err}"),
            }
        }
    }

    #[test]
    fn rejects_a_schedule_that_can_never_fire() {
        // Parses field by field, matches no instant: February 30th.
        let err = validate_cron("0 0 30 2 *").expect_err("February 30th never happens");
        assert!(matches!(err, AppError::Invalid(_)), "{err}");
    }

    #[test]
    fn rejection_reports_the_invalid_code_and_echoes_the_input() {
        let err = validate_cron("60 * * * *").expect_err("minute 60");
        let value = serde_json::to_value(&err).expect("AppError serializes");
        assert_eq!(value["code"], serde_json::json!("invalid"));
        assert!(
            value["message"]
                .as_str()
                .unwrap_or_default()
                .contains("60 * * * *"),
            "message should name the offending expression: {value}"
        );
    }

    // ---- due-time calculation -------------------------------------------

    #[test]
    fn is_due_once_the_next_boundary_after_the_last_run_has_passed() {
        // Monday. Last run 09:00, now 10:00:30 → the 10:00 slot is owed.
        let last = at(2026, 8, 3, 9, 0, 0);
        assert!(is_due(
            HOURLY_WEEKDAYS,
            Some(last),
            at(2026, 8, 3, 10, 0, 30)
        ));
    }

    #[test]
    fn is_not_due_before_the_next_boundary() {
        let last = at(2026, 8, 3, 9, 0, 0);
        assert!(!is_due(
            HOURLY_WEEKDAYS,
            Some(last),
            at(2026, 8, 3, 9, 59, 0)
        ));
    }

    #[test]
    fn is_not_due_again_within_the_same_slot() {
        // Ran at 10:00:05; ticks at 10:01, 10:02 … must not re-fire.
        let last = at(2026, 8, 3, 10, 0, 5);
        for minute in [1, 2, 30, 59] {
            assert!(
                !is_due(HOURLY_WEEKDAYS, Some(last), at(2026, 8, 3, 10, minute, 0)),
                "re-fired at 10:{minute:02}"
            );
        }
    }

    #[test]
    fn catches_up_a_single_missed_boundary_after_a_long_gap() {
        // App was closed for a week: the run is owed exactly once, now.
        let last = at(2026, 7, 27, 9, 0, 0);
        assert!(is_due(
            HOURLY_WEEKDAYS,
            Some(last),
            at(2026, 8, 3, 9, 30, 0)
        ));
    }

    #[test]
    fn without_a_last_run_only_a_boundary_inside_the_catch_up_window_fires() {
        // A fresh install must not fire just because the app started.
        assert!(!is_due(HOURLY_WEEKDAYS, None, at(2026, 8, 3, 9, 30, 0)));
        // …but a boundary that passed within the last tick is honoured.
        assert!(is_due(HOURLY_WEEKDAYS, None, at(2026, 8, 3, 9, 0, 30)));
    }

    #[test]
    fn is_not_due_outside_the_schedules_hours_or_days() {
        // Saturday 10:00 with a Mon-Fri schedule, last run Friday evening.
        let friday = at(2026, 8, 7, 19, 0, 0);
        assert!(!is_due(
            HOURLY_WEEKDAYS,
            Some(friday),
            at(2026, 8, 8, 10, 0, 0)
        ));
        // Weekday, but 03:00 is outside 07-19 and the 19:00 slot already ran.
        let evening = at(2026, 8, 3, 19, 0, 0);
        assert!(!is_due(
            HOURLY_WEEKDAYS,
            Some(evening),
            at(2026, 8, 4, 3, 0, 0)
        ));
    }

    #[test]
    fn a_last_run_in_the_future_never_fires() {
        // Clock skew (or a hand-edited stamp) must not put the scheduler into a
        // fire-every-tick loop.
        let future = at(2027, 1, 1, 9, 0, 0);
        assert!(!is_due(
            HOURLY_WEEKDAYS,
            Some(future),
            at(2026, 8, 3, 10, 0, 0)
        ));
    }

    #[test]
    fn an_unparseable_expression_is_never_due() {
        let now = at(2026, 8, 3, 10, 0, 30);
        for bad in ["", "not a cron", "0 7-19 * *", "0 0 30 2 *"] {
            assert!(
                !is_due(bad, Some(at(2026, 8, 3, 9, 0, 0)), now),
                "{bad:?} must not fire"
            );
            assert!(
                !is_due(bad, None, now),
                "{bad:?} must not fire (no history)"
            );
        }
    }

    // ---- last-run persistence -------------------------------------------

    #[test]
    fn last_run_round_trips_through_the_state_dir() {
        let dir = tempfile::tempdir().expect("temp state dir");
        let run = LastRun {
            at: iso(at(2026, 8, 3, 10, 0, 0)),
            trigger: LastTrigger::Scheduled,
        };
        write_last_run(dir.path(), &run).expect("write");
        let read = read_last_run(dir.path()).expect("record exists");
        assert_eq!(read.at, "2026-08-03T10:00:00Z");
        assert_eq!(read.trigger, LastTrigger::Scheduled);
        assert_eq!(parse_iso(&read.at), Some(at(2026, 8, 3, 10, 0, 0)));
    }

    #[test]
    fn writing_creates_a_missing_state_dir() {
        // First run on a fresh machine: the state dir may not exist yet.
        let dir = tempfile::tempdir().expect("temp dir");
        let nested = dir.path().join("state").join("autostand");
        write_run_record(&nested, at(2026, 8, 3, 10, 0, 0), LastTrigger::SelfHeal);
        assert_eq!(
            read_last_run(&nested).map(|r| r.trigger),
            Some(LastTrigger::SelfHeal)
        );
    }

    #[test]
    fn a_missing_or_corrupt_record_reads_as_never_ran() {
        let dir = tempfile::tempdir().expect("temp state dir");
        assert!(read_last_run(dir.path()).is_none(), "missing file");
        std::fs::write(last_run_path(dir.path()), "{ not json").expect("write garbage");
        assert!(read_last_run(dir.path()).is_none(), "corrupt file");
        std::fs::write(
            last_run_path(dir.path()),
            r#"{"at":"2026-08-03T10:00:00Z"}"#,
        )
        .expect("write partial record");
        assert!(read_last_run(dir.path()).is_none(), "missing trigger field");
    }

    #[test]
    fn a_later_run_overwrites_the_previous_stamp() {
        let dir = tempfile::tempdir().expect("temp state dir");
        write_run_record(dir.path(), at(2026, 8, 3, 10, 0, 0), LastTrigger::Scheduled);
        write_run_record(dir.path(), at(2026, 8, 3, 11, 0, 0), LastTrigger::Manual);
        let read = read_last_run(dir.path()).expect("record exists");
        assert_eq!(read.at, "2026-08-03T11:00:00Z");
        assert_eq!(read.trigger, LastTrigger::Manual);
    }

    #[test]
    fn the_persisted_record_is_a_two_field_json_object() {
        // Guard against a field rename silently orphaning existing records.
        let dir = tempfile::tempdir().expect("temp state dir");
        write_run_record(dir.path(), at(2026, 8, 3, 10, 0, 0), LastTrigger::Scheduled);
        let raw = std::fs::read_to_string(last_run_path(dir.path())).expect("read");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("valid JSON");
        assert_eq!(
            value,
            serde_json::json!({ "at": "2026-08-03T10:00:00Z", "trigger": "scheduled" })
        );
    }

    // ---- status reporting -----------------------------------------------

    #[test]
    fn status_reports_the_persisted_schedule_and_the_in_process_source() {
        let schedule = SchedulerConfig {
            enabled: true,
            cron: HOURLY_WEEKDAYS.to_string(),
            self_heal: true,
        };
        let status = build_status(
            &schedule,
            None,
            at(2026, 8, 3, 6, 30, 0),
            SchedulerSource::InProcess,
        );
        assert!(status.enabled);
        assert_eq!(status.source, SchedulerSource::InProcess);
        assert_eq!(status.cron, HOURLY_WEEKDAYS);
        assert_eq!(status.next_run_at.as_deref(), Some("2026-08-03T07:00:00Z"));
        assert!(status.last_run_at.is_none());
        assert!(status.last_trigger.is_none());
    }

    #[test]
    fn status_reports_an_installed_system_unit_verbatim() {
        // The whole point of installing one: a machine with autostand closed
        // must not still claim `in-process`.
        let schedule = SchedulerConfig::default();
        for source in [
            SchedulerSource::Launchd,
            SchedulerSource::Systemd,
            SchedulerSource::TaskScheduler,
            SchedulerSource::None,
        ] {
            let status = build_status(&schedule, None, at(2026, 8, 3, 6, 30, 0), source);
            assert_eq!(status.source, source);
        }
    }

    #[test]
    fn status_surfaces_the_persisted_last_run() {
        let schedule = SchedulerConfig::default();
        let last = LastRun {
            at: "2026-08-03T09:00:00Z".to_string(),
            trigger: LastTrigger::Scheduled,
        };
        let status = build_status(
            &schedule,
            Some(last),
            at(2026, 8, 3, 9, 30, 0),
            SchedulerSource::InProcess,
        );
        assert_eq!(status.last_run_at.as_deref(), Some("2026-08-03T09:00:00Z"));
        assert_eq!(status.last_trigger, Some(LastTrigger::Scheduled));
        assert_eq!(status.next_run_at.as_deref(), Some("2026-08-03T10:00:00Z"));
        assert!(!status.enabled, "the default schedule is opt-in");
    }

    #[test]
    fn status_omits_next_run_when_the_cron_can_never_fire() {
        let schedule = SchedulerConfig {
            enabled: true,
            cron: "0 0 30 2 *".to_string(),
            self_heal: true,
        };
        let status = build_status(
            &schedule,
            None,
            at(2026, 8, 3, 6, 30, 0),
            SchedulerSource::InProcess,
        );
        assert!(status.next_run_at.is_none());
        assert_eq!(status.cron, "0 0 30 2 *", "the bad value is still reported");
    }

    #[test]
    fn status_serializes_with_the_contract_wire_values() {
        let status = build_status(
            &SchedulerConfig::default(),
            None,
            at(2026, 8, 3, 6, 30, 0),
            SchedulerSource::InProcess,
        );
        let value = serde_json::to_value(&status).expect("SchedulerStatus serializes");
        assert_eq!(value["source"], serde_json::json!("in-process"));
        let mut keys: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "cron",
                "enabled",
                "last_run_at",
                "last_trigger",
                "next_run_at",
                "source",
            ]
        );
    }

    // ---- scheduler source resolution -------------------------------------

    #[test]
    fn an_installed_unit_is_reported_however_the_schedule_is_configured() {
        // The unit fires whether or not the app is open, so it is the truth
        // about "what will run at 07:00 tomorrow".
        for (kind, expected) in [
            (SchedulerKind::Launchd, SchedulerSource::Launchd),
            (SchedulerKind::Systemd, SchedulerSource::Systemd),
            (SchedulerKind::TaskScheduler, SchedulerSource::TaskScheduler),
        ] {
            for enabled in [true, false] {
                assert_eq!(
                    resolve_source(kind, enabled),
                    expected,
                    "{kind:?}/{enabled}"
                );
            }
        }
    }

    #[test]
    fn with_nothing_installed_an_enabled_schedule_is_served_in_process() {
        assert_eq!(
            resolve_source(SchedulerKind::None, true),
            SchedulerSource::InProcess
        );
    }

    #[test]
    fn with_nothing_installed_and_nothing_enabled_the_source_is_none() {
        // Reporting `in-process` here would claim a scheduler that is not going
        // to run anything.
        assert_eq!(
            resolve_source(SchedulerKind::None, false),
            SchedulerSource::None
        );
    }

    #[test]
    fn every_kind_maps_onto_the_wire_value_the_contract_uses() {
        // Two enums, one vocabulary: a rename on either side must break here
        // rather than on the frontend.
        for kind in [
            SchedulerKind::Launchd,
            SchedulerKind::Systemd,
            SchedulerKind::TaskScheduler,
            SchedulerKind::InProcess,
        ] {
            let wire = serde_json::to_value(resolve_source(kind, true))
                .expect("SchedulerSource serializes");
            assert_eq!(wire, serde_json::json!(kind.wire_label()), "{kind:?}");
        }
        let none = serde_json::to_value(resolve_source(SchedulerKind::None, false))
            .expect("SchedulerSource serializes");
        assert_eq!(none, serde_json::json!(SchedulerKind::None.wire_label()));
    }

    // ---- event payload ---------------------------------------------------

    #[test]
    fn tick_payload_matches_the_event_contract() {
        let value = serde_json::to_value(SchedulerTick {
            next_run_at: iso(at(2026, 8, 4, 9, 0, 0)),
            source: SchedulerSource::InProcess,
        })
        .expect("SchedulerTick serializes");
        assert_eq!(
            value,
            serde_json::json!({
                "next_run_at": "2026-08-04T09:00:00Z",
                "source": "in-process",
            })
        );
    }

    #[test]
    fn timestamps_are_z_suffixed_second_precision_iso_8601() {
        // `new Date(...)` in the frontend parses both forms, but the frontend
        // tests and fixtures all use the `Z` form; keep one shape on the wire.
        let formatted = iso(at(2026, 8, 3, 10, 0, 0));
        assert_eq!(formatted, "2026-08-03T10:00:00Z");
        assert_eq!(parse_iso(&formatted), Some(at(2026, 8, 3, 10, 0, 0)));
        assert!(parse_iso("2026-08-03 10:00").is_none());
    }
}
