//! Terminal-routed run log: the neutral half of "every action is visible".
//!
//! This crate deliberately knows nothing about Tauri. The domain crates
//! (`autostand-core`, `autostand-adapters`, `autostand-scheduler`) can log into
//! the user's Terminal panel by depending on this crate alone; the app crate
//! supplies the sink that turns a [`LogLine`] into a Tauri event.
//!
//! # The three pieces
//!
//! * [`LogLine`] / [`LogLevel`] — one rendered line. The wire values of
//!   `LogLevel` are pinned to the frontend's `PipelineLogLevel`
//!   (`"info" | "warn" | "error" | "done"`).
//! * [`RunSink`] — where lines go. [`NullSink`] is the headless default.
//! * [`proc`] — the single process spawner. Every child process in the
//!   workspace goes through it, so the Terminal cannot miss one.
//!
//! # The `tokio::spawn` trap
//!
//! The current sink lives in a **Tokio task-local**, and task-locals are *not*
//! inherited by [`tokio::spawn`]. A task spawned bare logs into [`NullSink`]
//! and vanishes from the Terminal:
//!
//! ```no_run
//! # async fn work() {}
//! // WRONG — the child task has no sink.
//! tokio::spawn(async move { work().await });
//!
//! // RIGHT — `inherit` captures the caller's sink and reinstalls it inside.
//! tokio::spawn(autostand_runlog::inherit(async move { work().await }));
//! ```
//!
//! The same applies to `JoinSet::spawn`, `tauri::async_runtime::spawn` and
//! `std::thread::spawn` (the last one cannot inherit at all — pass the
//! [`SinkRef`] explicitly and re-enter with [`scoped`] inside the thread's
//! runtime). `tests` in this module prove both halves.

#![forbid(unsafe_code)]

pub mod proc;

use std::future::Future;
use std::sync::{Arc, OnceLock};

use serde::{Deserialize, Serialize};

/// Severity of a [`LogLine`].
///
/// The wire values are pinned to `commands::types::PipelineLogLevel` in the app
/// crate and to `TerminalViewer.tsx`'s `LEVEL_CLASS` map. Renaming a variant
/// here silently drops the line's colour in the UI, so the app crate carries a
/// test asserting the two enums serialize identically.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    /// Step started, or neutral progress.
    #[default]
    Info,
    /// Non-fatal: a fallback was taken, a source was skipped.
    Warn,
    /// The step failed.
    Error,
    /// The step finished successfully.
    Done,
}

impl LogLevel {
    /// Wire string, identical to the serde representation.
    ///
    /// Exposed so a sink can build a payload without a serde round-trip.
    pub fn label(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
            Self::Done => "done",
        }
    }
}

/// One line in the Terminal panel.
///
/// `step` is the pipeline step (or pseudo-step) the line belongs to; the viewer
/// colours by it. `detail` is the dim right-hand column — counts, durations and
/// exit codes, never payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LogLine {
    /// Step label (`gather`, `render_llm`, `repo_sync`, …).
    pub step: String,
    /// Severity.
    pub level: LogLevel,
    /// Human-readable text. English, present/past tense, no secrets.
    pub message: String,
    /// Optional dim suffix (durations, counts, exit codes).
    pub detail: Option<String>,
}

impl LogLine {
    /// Build a line with no detail.
    pub fn new(step: impl Into<String>, level: LogLevel, message: impl Into<String>) -> Self {
        Self {
            step: step.into(),
            level,
            message: message.into(),
            detail: None,
        }
    }

    /// Attach the dim right-hand detail column.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// `LogLevel::Info` line.
    pub fn info(step: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(step, LogLevel::Info, message)
    }

    /// `LogLevel::Warn` line.
    pub fn warn(step: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(step, LogLevel::Warn, message)
    }

    /// `LogLevel::Error` line.
    pub fn error(step: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(step, LogLevel::Error, message)
    }

    /// `LogLevel::Done` line.
    pub fn done(step: impl Into<String>, message: impl Into<String>) -> Self {
        Self::new(step, LogLevel::Done, message)
    }
}

/// Destination for [`LogLine`]s.
///
/// Implementations must be cheap and infallible: a sink is called from inside
/// the pipeline, and a failed log line must never fail a compile. Swallow
/// transport errors (or downgrade them to `tracing`) instead of propagating.
///
/// `Debug` is a supertrait so a struct that merely *holds* a [`SinkRef`] can
/// still derive `Debug` — which most of them want to.
pub trait RunSink: std::fmt::Debug + Send + Sync + 'static {
    /// Deliver one line.
    fn log(&self, line: LogLine);
}

/// Sink that drops everything.
///
/// The default when no run is open: headless `--compile` runs, unit tests and
/// any code path reached before a [`scoped`] block.
#[derive(Debug, Clone, Copy, Default)]
pub struct NullSink;

impl RunSink for NullSink {
    fn log(&self, _line: LogLine) {}
}

/// Shared handle to the sink of the current run.
pub type SinkRef = Arc<dyn RunSink>;

tokio::task_local! {
    /// Sink of the run the current task belongs to.
    static CURRENT_SINK: SinkRef;
}

/// The process-wide [`NullSink`], allocated once.
fn null_sink() -> SinkRef {
    static NULL: OnceLock<SinkRef> = OnceLock::new();
    Arc::clone(NULL.get_or_init(|| Arc::new(NullSink)))
}

/// Sink of the current task, or [`NullSink`] when no run is open.
///
/// Never panics and never returns `None`: logging is a courtesy, so code that
/// runs outside a run (headless compile, unit test) keeps working unchanged.
pub fn current_sink() -> SinkRef {
    CURRENT_SINK
        .try_with(Arc::clone)
        .unwrap_or_else(|_| null_sink())
}

/// True when a real (non-null) sink is installed for the current task.
///
/// Only useful to skip building an expensive message; logging into the null
/// sink is already free.
pub fn has_sink() -> bool {
    CURRENT_SINK.try_with(|_| ()).is_ok()
}

/// Run `future` with `sink` installed as the current sink.
///
/// This is how a run is opened. Everything awaited inside — including
/// [`proc::run_process`] — logs to `sink`.
pub fn scoped<F>(sink: SinkRef, future: F) -> impl Future<Output = F::Output>
where
    F: Future,
{
    CURRENT_SINK.scope(sink, future)
}

/// Wrap `future` so it keeps the *caller's* sink once it is handed to
/// [`tokio::spawn`].
///
/// The sink is captured at call time (on the parent task, which still has the
/// task-local) and reinstalled inside the future. Wrapping at the spawn site is
/// mandatory: a bare `tokio::spawn` starts a task with an empty task-local map,
/// so its logs go to [`NullSink`] and disappear from the Terminal.
pub fn inherit<F>(future: F) -> impl Future<Output = F::Output>
where
    F: Future,
{
    scoped(current_sink(), future)
}

/// Send one line to the current sink.
pub fn log(line: LogLine) {
    current_sink().log(line);
}

/// `LogLevel::Info` shorthand for [`log`].
pub fn info(step: impl Into<String>, message: impl Into<String>) {
    log(LogLine::info(step, message));
}

/// `LogLevel::Warn` shorthand for [`log`].
pub fn warn(step: impl Into<String>, message: impl Into<String>) {
    log(LogLine::warn(step, message));
}

/// `LogLevel::Error` shorthand for [`log`].
pub fn error(step: impl Into<String>, message: impl Into<String>) {
    log(LogLine::error(step, message));
}

/// `LogLevel::Done` shorthand for [`log`].
pub fn done(step: impl Into<String>, message: impl Into<String>) {
    log(LogLine::done(step, message));
}

/// In-memory sink for tests and for asserting the routing of a real run.
///
/// Shipped in the library (not behind `cfg(test)`) because the app crate and
/// the integration tests need it too.
#[derive(Debug, Default)]
pub struct RecordingSink {
    lines: std::sync::Mutex<Vec<LogLine>>,
}

impl RecordingSink {
    /// Empty recorder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot of everything recorded so far.
    ///
    /// A poisoned lock degrades to an empty snapshot: a broken recorder must
    /// not panic a test's teardown path.
    pub fn lines(&self) -> Vec<LogLine> {
        self.lines.lock().map(|l| l.clone()).unwrap_or_default()
    }

    /// Just the messages, in order.
    pub fn messages(&self) -> Vec<String> {
        self.lines().into_iter().map(|l| l.message).collect()
    }
}

impl RunSink for RecordingSink {
    fn log(&self, line: LogLine) {
        if let Ok(mut lines) = self.lines.lock() {
            lines.push(line);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        current_sink, done, has_sink, info, inherit, scoped, LogLevel, LogLine, RecordingSink,
        SinkRef,
    };
    use std::sync::Arc;

    fn recorder() -> (Arc<RecordingSink>, SinkRef) {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        (sink, as_ref)
    }

    #[test]
    fn level_wire_values_match_the_frontend_contract() {
        let cases = [
            (LogLevel::Info, "info"),
            (LogLevel::Warn, "warn"),
            (LogLevel::Error, "error"),
            (LogLevel::Done, "done"),
        ];
        for (level, expected) in cases {
            assert_eq!(
                serde_json::to_string(&level).unwrap(),
                format!("\"{expected}\"")
            );
            assert_eq!(level.label(), expected);
        }
    }

    #[test]
    fn a_line_serializes_with_the_terminal_viewer_field_names() {
        let value =
            serde_json::to_value(LogLine::done("gather", "gathered").with_detail("3 repo(s)"))
                .unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "step": "gather",
                "level": "done",
                "message": "gathered",
                "detail": "3 repo(s)",
            })
        );
    }

    #[tokio::test]
    async fn without_a_run_the_sink_is_null_and_logging_is_a_no_op() {
        assert!(!has_sink());
        // Must not panic: headless compiles run entirely outside a scope.
        info("gather", "no run open");
    }

    #[tokio::test]
    async fn scoped_installs_the_sink_for_everything_it_awaits() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            assert!(has_sink());
            info("gather", "first");
            nested().await;
            done("gather", "last");
        })
        .await;
        assert_eq!(sink.messages(), ["first", "nested", "last"]);
    }

    async fn nested() {
        // Yield first: the sink must survive a real suspension point, not just
        // a synchronous call that happens to be spelled `async`.
        tokio::task::yield_now().await;
        info("gather", "nested");
    }

    #[tokio::test]
    async fn the_scope_ends_with_the_future() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async { info("gather", "inside") }).await;
        info("gather", "outside");
        assert_eq!(sink.messages(), ["inside"]);
    }

    /// The trap that will break the migration if it is forgotten.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tokio_spawn_does_not_inherit_the_sink() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            let handle = tokio::spawn(async {
                assert!(
                    !has_sink(),
                    "if this ever starts passing, tokio began inheriting task-locals"
                );
                info("gather", "lost");
            });
            handle.await.unwrap();
        })
        .await;
        assert!(
            sink.lines().is_empty(),
            "a bare tokio::spawn silently drops its Terminal output"
        );
    }

    /// …and the fix for it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn inherit_carries_the_sink_into_a_spawned_task() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            let handle = tokio::spawn(inherit(async {
                assert!(has_sink());
                info("gather", "kept");
            }));
            handle.await.unwrap();
        })
        .await;
        assert_eq!(sink.messages(), ["kept"]);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_runs_do_not_cross_talk() {
        let (left, left_ref) = recorder();
        let (right, right_ref) = recorder();
        let a = tokio::spawn(scoped(left_ref, async {
            info("gather", "left");
        }));
        let b = tokio::spawn(scoped(right_ref, async {
            info("gather", "right");
        }));
        a.await.unwrap();
        b.await.unwrap();
        assert_eq!(left.messages(), ["left"]);
        assert_eq!(right.messages(), ["right"]);
    }

    #[tokio::test]
    async fn current_sink_is_cloneable_for_a_thread_that_cannot_inherit() {
        let (sink, as_ref) = recorder();
        let captured = scoped(as_ref, async { current_sink() }).await;
        captured.log(LogLine::info("gather", "handed over"));
        assert_eq!(sink.messages(), ["handed over"]);
    }
}
