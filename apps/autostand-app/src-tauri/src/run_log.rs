//! Run identity + the Tauri sink: how work becomes visible in the Terminal.
//!
//! One rule drives this module: **every action that starts work opens a
//! [`Run`]**. A run has an identity (`run_id`, [`RunKind`], date, host), it
//! streams [`LogLine`]s to the Terminal panel while it works, and it closes
//! exactly once. Nothing may spawn a process, call a provider or touch git
//! outside one.
//!
//! ```ignore
//! let run = Run::open(&app_handle, RunKind::RepoSetup);
//! let outcome = run.scope(async { do_the_work().await }).await;
//! run.settle(outcome, "repository ready")
//! ```
//!
//! [`Run::scope`] is the load-bearing call: it installs the run's sink as the
//! Tokio task-local that `autostand_runlog::proc` reads, so a `git` spawned six
//! frames deep in a domain crate lands in the user's panel without any of those
//! frames knowing what Tauri is.
//!
//! # Events
//!
//! | Event | Payload | Meaning |
//! |---|---|---|
//! | `run-started` | [`RunStarted`] | a run opened; the panel clears and opens |
//! | `pipeline-log` | `PipelineLogLine` | one line, unchanged from the existing contract |
//! | `run-finished` | [`RunFinished`] | the run closed; `ok` says how |
//!
//! `pipeline-log` is deliberately reused rather than replaced: `TerminalViewer`
//! and `use-pipeline-log` already consume it, and a run's lines must show up
//! there today, not after a frontend migration.
//!
//! `pipeline_runner::run` still emits `pipeline-started` after the compile's
//! [`Run`] has opened, so the two events would have cleared the log buffer
//! twice — wiping the run header and the lock/sync lines in between. Resolved
//! on the frontend side: `use-pipeline-status.ts` clears on `run-started` only,
//! and the `pipeline-started` handler just sets the trigger and the status.
//! `run-started` is the single owner of "a run opened → clear and open".
//!
//! # The `tokio::spawn` trap
//!
//! Task-locals do not cross [`tokio::spawn`] / `tauri::async_runtime::spawn` /
//! `JoinSet::spawn`. Wrap the spawned future in `autostand_runlog::inherit(…)`
//! or the task logs into the null sink and its work disappears from the panel.
//! See `crates/autostand-runlog/src/lib.rs` for the pair of tests that pin it.

use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter};

use autostand_runlog::{LogLevel, LogLine, RunSink, SinkRef};

use crate::commands::types::{PipelineLogLevel, PipelineLogLine};
use crate::error::AppError;

/// Date format shared with the rest of the IPC contract.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Emitted when a run opens.
const RUN_STARTED_EVENT: &str = "run-started";

/// Emitted when a run closes, exactly once per [`RunStarted`].
const RUN_FINISHED_EVENT: &str = "run-finished";

/// Per-line event. Same name and payload as before this module existed.
const RUN_LOG_EVENT: &str = "pipeline-log";

/// Monotonic part of a `run_id`.
static RUN_COUNTER: AtomicU64 = AtomicU64::new(1);

/// What kind of work a [`Run`] represents.
///
/// One variant per user-visible action that starts work. The wire values are
/// `snake_case` so the frontend can switch on them; adding a variant is
/// backwards compatible, renaming one is not.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunKind {
    /// The full pipeline: gather → render → write → commit.
    Compile,
    /// "Compile now": a candidate rendered in isolation for review.
    Regenerate,
    /// Read-only gather preview (debug UI).
    GatherPreview,
    /// `git pull` / `commit` / `push` on the dailies repo.
    RepoSync,
    /// First-time Repo Sync setup (`gh repo create`, initial push).
    RepoSetup,
    /// Scanning the GitHub directory for repositories.
    RepoDiscovery,
    /// Preparing a cloud-synced dailies root.
    CloudSync,
    /// "Test provider" from Settings.
    ProviderTest,
    /// The provider health sweep.
    ProviderHealth,
    /// Locating a provider CLI binary on disk.
    CliDetect,
    /// Downloading a local GGUF model.
    ModelDownload,
    /// Deleting a local GGUF model.
    ModelDelete,
    /// Starting, probing or unloading the local inference runtime.
    LocalRuntime,
    /// Checking installed prerequisites (git, gh, CLIs).
    DependencyCheck,
    /// Installing a missing prerequisite.
    DependencyRemediation,
    /// Installing / reconciling the platform scheduler unit.
    SchedulerUnit,
    /// Delivering a desktop notification.
    Notification,
}

impl RunKind {
    /// Step label the run's own lines are filed under.
    ///
    /// Reuses the pipeline's vocabulary where one already exists so
    /// `TerminalViewer`'s `STEP_COLOR` map keeps colouring them; new kinds get
    /// their own label and fall back to the neutral colour.
    pub fn step(self) -> &'static str {
        match self {
            Self::Compile => "compile",
            Self::Regenerate => "regenerate",
            Self::GatherPreview => "gather",
            Self::RepoSync => "repo_sync",
            Self::RepoSetup => "repo_setup",
            Self::RepoDiscovery => "repo_discovery",
            Self::CloudSync => "cloud_sync",
            Self::ProviderTest => "provider_test",
            Self::ProviderHealth => "provider_health",
            Self::CliDetect => "cli_detect",
            Self::ModelDownload => "model_download",
            Self::ModelDelete => "model_delete",
            Self::LocalRuntime => "local_runtime",
            Self::DependencyCheck => "dependency_check",
            Self::DependencyRemediation => "dependency_fix",
            Self::SchedulerUnit => "scheduler_unit",
            Self::Notification => "notification",
        }
    }

    /// English header shown when the run opens. UI copy.
    pub fn title(self) -> &'static str {
        match self {
            Self::Compile => "Compile standup",
            Self::Regenerate => "Regenerate standup",
            Self::GatherPreview => "Preview gathered data",
            Self::RepoSync => "Sync dailies repository",
            Self::RepoSetup => "Set up Repo Sync",
            Self::RepoDiscovery => "Discover repositories",
            Self::CloudSync => "Configure cloud sync",
            Self::ProviderTest => "Test provider",
            Self::ProviderHealth => "Refresh provider health",
            Self::CliDetect => "Detect CLI",
            Self::ModelDownload => "Download local model",
            Self::ModelDelete => "Delete local model",
            Self::LocalRuntime => "Local runtime",
            Self::DependencyCheck => "Check requirements",
            Self::DependencyRemediation => "Install requirement",
            Self::SchedulerUnit => "Update scheduled runs",
            Self::Notification => "Send notification",
        }
    }

    /// Whether this run owns the `PipelineStatus` the dashboard renders.
    ///
    /// Only the three kinds that walk the real pipeline emit
    /// `pipeline-progress`; everything else must leave the progress bar alone,
    /// so a provider test cannot make the dashboard look like it is compiling.
    pub fn drives_pipeline_status(self) -> bool {
        matches!(self, Self::Compile | Self::Regenerate | Self::GatherPreview)
    }
}

/// Payload of `run-started`.
#[derive(Debug, Clone, Serialize)]
pub struct RunStarted {
    /// Correlates with the matching [`RunFinished`].
    pub run_id: String,
    /// What kind of work opened.
    pub kind: RunKind,
    /// English header for the panel.
    pub title: &'static str,
    /// Filing date this run is about (`YYYY-MM-DD`).
    pub date: String,
    /// Host slug.
    pub host: String,
    /// Whether this run drives `PipelineStatus` — see
    /// [`RunKind::drives_pipeline_status`].
    pub pipeline: bool,
}

/// Payload of `run-finished`.
#[derive(Debug, Clone, Serialize)]
pub struct RunFinished {
    /// Correlates with the matching [`RunStarted`].
    pub run_id: String,
    /// What kind of work closed.
    pub kind: RunKind,
    /// Whether the run succeeded.
    pub ok: bool,
    /// Short English outcome. On failure this is the error's message, which is
    /// already secret-free by `AppError`'s own contract.
    pub message: String,
    /// Wall-clock duration.
    pub duration_ms: u64,
    /// Whether this run drives `PipelineStatus`.
    pub pipeline: bool,
}

impl From<LogLevel> for PipelineLogLevel {
    fn from(level: LogLevel) -> Self {
        match level {
            LogLevel::Info => Self::Info,
            LogLevel::Warn => Self::Warn,
            LogLevel::Error => Self::Error,
            LogLevel::Done => Self::Done,
        }
    }
}

/// [`RunSink`] that turns a [`LogLine`] into a `pipeline-log` Tauri event.
///
/// Holds the run's `date` / `host` because `PipelineLogLine` carries them and
/// the domain code that produces the line does not know either.
#[derive(Debug)]
pub struct TauriSink {
    app: AppHandle,
    date: String,
    host: String,
}

impl TauriSink {
    /// Sink that stamps every line with `date` + `host`.
    pub fn new(app: AppHandle, date: impl Into<String>, host: impl Into<String>) -> Self {
        Self {
            app,
            date: date.into(),
            host: host.into(),
        }
    }
}

impl RunSink for TauriSink {
    fn log(&self, line: LogLine) {
        let payload = PipelineLogLine {
            date: self.date.clone(),
            host: self.host.clone(),
            step: line.step,
            level: line.level.into(),
            message: line.message,
            detail: line.detail,
        };
        // A closed window must never fail the work it was watching.
        if let Err(err) = self.app.emit(RUN_LOG_EVENT, payload) {
            tracing::warn!(error = %err, "could not emit a run log line");
        }
    }
}

/// One open unit of work, from the button press to the outcome.
///
/// Dropping a `Run` without calling [`Run::settle`] / [`Run::finish_ok`] /
/// [`Run::finish_err`] emits a `run-finished` with `ok: false` so a panicking
/// or early-returning command cannot leave the panel spinning forever.
#[derive(Debug)]
pub struct Run {
    app: AppHandle,
    kind: RunKind,
    id: String,
    date: String,
    host: String,
    sink: SinkRef,
    started: Instant,
    closed: bool,
}

impl Run {
    /// Open a run for today, resolving the host slug from disk.
    pub fn open(app: &AppHandle, kind: RunKind) -> Self {
        let date = chrono::Local::now()
            .date_naive()
            .format(DATE_FORMAT)
            .to_string();
        let host = autostand_core::host::load_or_detect(&crate::commands::state_dir())
            .unwrap_or_else(|_| "unknown-host".to_string());
        Self::open_for(app, kind, date, host)
    }

    /// Open a run whose date and host the caller already resolved.
    ///
    /// Preferred inside the pipeline: `RunEnv` has both, and re-detecting the
    /// host would let a run's lines disagree with the file it is writing.
    pub fn open_for(
        app: &AppHandle,
        kind: RunKind,
        date: impl Into<String>,
        host: impl Into<String>,
    ) -> Self {
        let date = date.into();
        let host = host.into();
        let id = next_run_id(kind);
        let sink: SinkRef = Arc::new(TauriSink::new(app.clone(), date.clone(), host.clone()));
        let run = Self {
            app: app.clone(),
            kind,
            id: id.clone(),
            date: date.clone(),
            host: host.clone(),
            sink,
            started: Instant::now(),
            closed: false,
        };
        run.emit(
            RUN_STARTED_EVENT,
            RunStarted {
                run_id: id,
                kind,
                title: kind.title(),
                date,
                host,
                pipeline: kind.drives_pipeline_status(),
            },
        );
        run.log(LogLine::info(kind.step(), kind.title()));
        run
    }

    /// Stable identifier, correlating `run-started` with `run-finished`.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// What kind of work this run represents.
    pub fn kind(&self) -> RunKind {
        self.kind
    }

    /// Filing date the run is about.
    pub fn date(&self) -> &str {
        &self.date
    }

    /// Host slug the run is about.
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The run's sink, for a task that cannot inherit the task-local (a
    /// `std::thread`, or a future built before the scope is entered).
    pub fn sink(&self) -> SinkRef {
        Arc::clone(&self.sink)
    }

    /// Run `future` with this run's sink installed as the current sink.
    ///
    /// Everything awaited inside — including `autostand_runlog::proc` spawns —
    /// logs to the Terminal. Tasks handed to `tokio::spawn` inside the scope
    /// must still be wrapped in `autostand_runlog::inherit`.
    pub fn scope<F>(&self, future: F) -> impl Future<Output = F::Output>
    where
        F: Future,
    {
        autostand_runlog::scoped(self.sink(), future)
    }

    /// Send one line, bypassing the task-local.
    pub fn log(&self, line: LogLine) {
        self.sink.log(line);
    }

    /// `info` line filed under this run's default step.
    pub fn info(&self, message: impl Into<String>) {
        self.log(LogLine::info(self.kind.step(), message));
    }

    /// `warn` line filed under this run's default step.
    pub fn warn(&self, message: impl Into<String>) {
        self.log(LogLine::warn(self.kind.step(), message));
    }

    /// `done` line filed under this run's default step.
    pub fn done(&self, message: impl Into<String>) {
        self.log(LogLine::done(self.kind.step(), message));
    }

    /// Close the run as a success.
    pub fn finish_ok(mut self, message: impl Into<String>) {
        let message = message.into();
        self.log(LogLine::done(self.kind.step(), message.clone()));
        self.close(true, message);
    }

    /// Close the run as a failure.
    pub fn finish_err(mut self, message: impl Into<String>) {
        let message = message.into();
        self.log(LogLine::error(self.kind.step(), message.clone()));
        self.close(false, message);
    }

    /// Close the run from a command's `Result` and hand the `Result` back.
    ///
    /// The intended shape of every migrated call site:
    /// `run.settle(run.scope(work()).await, "done")`.
    pub fn settle<T>(self, outcome: Result<T, AppError>, ok_message: &str) -> Result<T, AppError> {
        match outcome {
            Ok(value) => {
                self.finish_ok(ok_message);
                Ok(value)
            }
            Err(err) => {
                self.finish_err(err.to_string());
                Err(err)
            }
        }
    }

    /// Emit `run-finished` once.
    fn close(&mut self, ok: bool, message: String) {
        if self.closed {
            return;
        }
        self.closed = true;
        let duration_ms = u64::try_from(self.started.elapsed().as_millis()).unwrap_or(u64::MAX);
        self.emit(
            RUN_FINISHED_EVENT,
            RunFinished {
                run_id: self.id.clone(),
                kind: self.kind,
                ok,
                message,
                duration_ms,
                pipeline: self.kind.drives_pipeline_status(),
            },
        );
    }

    /// Emit an event, downgrading a dead window to a warning.
    fn emit<P: Serialize + Clone>(&self, event: &str, payload: P) {
        if let Err(err) = self.app.emit(event, payload) {
            tracing::warn!(event, error = %err, "could not emit a run event");
        }
    }
}

impl Drop for Run {
    fn drop(&mut self) {
        // An early `?` return or a panic must not leave the panel believing the
        // run is still going.
        self.close(false, "run ended unexpectedly".to_string());
    }
}

/// `<kind>-<counter>` — unique per process, readable in a log.
fn next_run_id(kind: RunKind) -> String {
    let counter = RUN_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{}-{counter}", kind.step())
}

#[cfg(test)]
mod tests {
    use super::{next_run_id, RunFinished, RunKind, RunStarted};
    use crate::commands::types::PipelineLogLevel;
    use autostand_runlog::LogLevel;

    /// Every kind, so the table-driven tests below cannot silently miss one.
    const ALL: [RunKind; 17] = [
        RunKind::Compile,
        RunKind::Regenerate,
        RunKind::GatherPreview,
        RunKind::RepoSync,
        RunKind::RepoSetup,
        RunKind::RepoDiscovery,
        RunKind::CloudSync,
        RunKind::ProviderTest,
        RunKind::ProviderHealth,
        RunKind::CliDetect,
        RunKind::ModelDownload,
        RunKind::ModelDelete,
        RunKind::LocalRuntime,
        RunKind::DependencyCheck,
        RunKind::DependencyRemediation,
        RunKind::SchedulerUnit,
        RunKind::Notification,
    ];

    /// The contract that keeps `TerminalViewer.tsx` rendering runlog lines: the
    /// neutral `LogLevel` and the IPC `PipelineLogLevel` must be the same four
    /// wire strings, in both directions.
    #[test]
    fn the_neutral_level_and_the_ipc_level_share_one_wire_vocabulary() {
        let pairs = [
            (LogLevel::Info, PipelineLogLevel::Info),
            (LogLevel::Warn, PipelineLogLevel::Warn),
            (LogLevel::Error, PipelineLogLevel::Error),
            (LogLevel::Done, PipelineLogLevel::Done),
        ];
        for (neutral, ipc) in pairs {
            assert_eq!(
                serde_json::to_string(&neutral).unwrap(),
                serde_json::to_string(&ipc).unwrap(),
            );
            assert_eq!(PipelineLogLevel::from(neutral), ipc);
        }
    }

    #[test]
    fn every_kind_has_a_distinct_snake_case_step_and_an_english_title() {
        let mut steps: Vec<&str> = ALL.iter().map(|kind| kind.step()).collect();
        let count = steps.len();
        steps.sort_unstable();
        steps.dedup();
        assert_eq!(steps.len(), count, "step labels must be unique");
        for kind in ALL {
            let step = kind.step();
            assert!(
                step.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "{step} is not snake_case"
            );
            let title = kind.title();
            assert!(!title.is_empty());
            assert!(
                title.starts_with(|c: char| c.is_ascii_uppercase()),
                "UI copy is sentence case English: {title}"
            );
        }
    }

    #[test]
    fn only_the_pipeline_kinds_own_the_dashboard_progress_bar() {
        let owners: Vec<RunKind> = ALL
            .into_iter()
            .filter(|kind| kind.drives_pipeline_status())
            .collect();
        assert_eq!(
            owners,
            [
                RunKind::Compile,
                RunKind::Regenerate,
                RunKind::GatherPreview
            ],
            "a provider test must not make the dashboard look like a compile"
        );
    }

    #[test]
    fn kinds_serialize_snake_case_for_the_frontend() {
        assert_eq!(
            serde_json::to_string(&RunKind::GatherPreview).unwrap(),
            "\"gather_preview\""
        );
        assert_eq!(
            serde_json::to_string(&RunKind::ProviderHealth).unwrap(),
            "\"provider_health\""
        );
    }

    #[test]
    fn run_ids_are_unique_and_name_their_kind() {
        let first = next_run_id(RunKind::Regenerate);
        let second = next_run_id(RunKind::Regenerate);
        assert_ne!(first, second);
        assert!(first.starts_with("regenerate-"), "{first}");
    }

    #[test]
    fn started_payload_matches_the_event_contract() {
        let value = serde_json::to_value(RunStarted {
            run_id: "regenerate-1".to_string(),
            kind: RunKind::Regenerate,
            title: RunKind::Regenerate.title(),
            date: "2026-08-14".to_string(),
            host: "MacStudio-de-Miguel".to_string(),
            pipeline: true,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "run_id": "regenerate-1",
                "kind": "regenerate",
                "title": "Regenerate standup",
                "date": "2026-08-14",
                "host": "MacStudio-de-Miguel",
                "pipeline": true,
            })
        );
    }

    #[test]
    fn finished_payload_matches_the_event_contract() {
        let value = serde_json::to_value(RunFinished {
            run_id: "provider_test-2".to_string(),
            kind: RunKind::ProviderTest,
            ok: false,
            message: "llm: provider timed out".to_string(),
            duration_ms: 1234,
            pipeline: false,
        })
        .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "run_id": "provider_test-2",
                "kind": "provider_test",
                "ok": false,
                "message": "llm: provider timed out",
                "duration_ms": 1234,
                "pipeline": false,
            })
        );
    }
}
