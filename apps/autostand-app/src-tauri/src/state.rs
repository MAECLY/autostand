//! Pipeline run state shared across the IPC surface.
//!
//! See `docs/tauri/02-ipc-contracts.md` § `PipelineStatus` and the Event system table.
//! `AppState` is registered as Tauri-managed state and read by the
//! `get_pipeline_status` command; compile commands update it as they progress.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::commands::types::CompileResult;

/// Coarse-grained pipeline run state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PipelineStateKind {
    /// Idle — no run in progress.
    #[default]
    Idle,
    /// Gathering facts/notes/enrichment from data sources.
    Gathering,
    /// Rendering the standup body (LLM or deterministic).
    Rendering,
    /// Run finished successfully.
    Done,
    /// Run failed.
    Error,
}

/// Snapshot of the pipeline state surfaced to the frontend.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PipelineStatus {
    /// Coarse state machine position.
    pub state: PipelineStateKind,
    /// Filing date of the in-progress run (`YYYY-MM-DD`).
    pub current_date: Option<String>,
    /// Host slug of the in-progress run.
    pub current_host: Option<String>,
    /// Human-readable step name (e.g. `gather`, `render_llm`).
    pub step: Option<String>,
    /// Progress percent `0..=100`.
    pub percent: u8,
    /// ISO-8601 timestamp of the last run start.
    pub last_run_at: Option<String>,
    /// Result of the last run (present once a run completes).
    pub last_result: Option<CompileResult>,
    /// Error message if `state == Error`.
    pub error: Option<String>,
}

/// Tauri-managed state wrapper exposing the pipeline status to commands.
#[derive(Debug, Default)]
pub struct AppState {
    status: Mutex<PipelineStatus>,
}

impl AppState {
    /// Create a fresh `AppState` (idle).
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Snapshot the current `PipelineStatus`.
    ///
    /// A poisoned lock degrades to the default (idle) snapshot rather than
    /// panicking — a status read must never take down an IPC command.
    pub fn status(&self) -> PipelineStatus {
        self.status
            .lock()
            .map_or_else(|_| PipelineStatus::default(), |s| s.clone())
    }

    /// Begin a run for `date` + `host`; clears any prior error.
    pub fn set_state(
        &self,
        date: impl Into<Option<String>>,
        host: impl Into<Option<String>>,
        kind: PipelineStateKind,
        step: impl Into<Option<String>>,
    ) {
        if let Ok(mut status) = self.status.lock() {
            status.state = kind;
            status.current_date = date.into();
            status.current_host = host.into();
            status.step = step.into();
            status.percent = 0;
            status.error = None;
            status.last_run_at = Some(chrono::Utc::now().to_rfc3339());
        }
    }

    /// Update the progress percent.
    pub fn set_percent(&self, percent: u8, step: impl Into<Option<String>>) {
        if let Ok(mut status) = self.status.lock() {
            status.percent = percent.min(100);
            if let Some(s) = step.into() {
                status.step = Some(s);
            }
        }
    }

    /// Mark the run as done and store the result.
    pub fn set_done(&self, result: CompileResult) {
        if let Ok(mut status) = self.status.lock() {
            status.state = PipelineStateKind::Done;
            status.percent = 100;
            status.last_result = Some(result);
        }
    }

    /// Mark the run as failed with an error message.
    pub fn set_error(&self, message: impl Into<String>) {
        if let Ok(mut status) = self.status.lock() {
            status.state = PipelineStateKind::Error;
            status.error = Some(message.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AppState, PipelineStateKind, PipelineStatus};
    use crate::commands::types::{CompileResult, CompileStatus};

    fn sample_result() -> CompileResult {
        CompileResult {
            date: "2026-08-03".to_string(),
            host: "MacStudio-de-Miguel".to_string(),
            status: CompileStatus::Ok,
            ..CompileResult::default()
        }
    }

    #[test]
    fn starts_idle_and_empty() {
        let status = AppState::new().status();
        assert_eq!(status.state, PipelineStateKind::Idle);
        assert_eq!(status.percent, 0);
        assert!(status.current_date.is_none());
        assert!(status.current_host.is_none());
        assert!(status.step.is_none());
        assert!(status.last_run_at.is_none());
        assert!(status.last_result.is_none());
        assert!(status.error.is_none());
    }

    #[test]
    fn set_state_records_run_identity_and_stamps_last_run() {
        let state = AppState::new();
        state.set_state(
            "2026-08-03".to_string(),
            "MacStudio-de-Miguel".to_string(),
            PipelineStateKind::Gathering,
            "gather".to_string(),
        );
        let status = state.status();
        assert_eq!(status.state, PipelineStateKind::Gathering);
        assert_eq!(status.current_date.as_deref(), Some("2026-08-03"));
        assert_eq!(status.current_host.as_deref(), Some("MacStudio-de-Miguel"));
        assert_eq!(status.step.as_deref(), Some("gather"));
        assert_eq!(status.percent, 0);
        assert!(status.last_run_at.is_some(), "set_state stamps last_run_at");
    }

    #[test]
    fn set_state_clears_a_previous_error() {
        let state = AppState::new();
        state.set_error("boom");
        state.set_state(None, None, PipelineStateKind::Gathering, None);
        assert!(state.status().error.is_none());
    }

    #[test]
    fn set_percent_updates_progress_and_optional_step() {
        let state = AppState::new();
        state.set_state(
            None,
            None,
            PipelineStateKind::Gathering,
            "gather".to_string(),
        );
        state.set_percent(42, "render_llm".to_string());
        let status = state.status();
        assert_eq!(status.percent, 42);
        assert_eq!(status.step.as_deref(), Some("render_llm"));
    }

    #[test]
    fn set_percent_without_a_step_keeps_the_current_step() {
        let state = AppState::new();
        state.set_state(
            None,
            None,
            PipelineStateKind::Gathering,
            "gather".to_string(),
        );
        state.set_percent(10, None);
        let status = state.status();
        assert_eq!(status.percent, 10);
        assert_eq!(status.step.as_deref(), Some("gather"));
    }

    #[test]
    fn set_percent_clamps_to_the_contracts_0_100_range() {
        let state = AppState::new();
        state.set_percent(240, None);
        assert_eq!(state.status().percent, 100);
    }

    #[test]
    fn set_done_stores_the_result_and_completes_progress() {
        let state = AppState::new();
        state.set_state(None, None, PipelineStateKind::Rendering, None);
        state.set_done(sample_result());
        let status = state.status();
        assert_eq!(status.state, PipelineStateKind::Done);
        assert_eq!(status.percent, 100);
        assert_eq!(
            status.last_result.map(|r| r.date).as_deref(),
            Some("2026-08-03")
        );
    }

    #[test]
    fn set_error_flips_state_and_keeps_the_last_result() {
        let state = AppState::new();
        state.set_done(sample_result());
        state.set_error("gather pipeline not yet wired");
        let status = state.status();
        assert_eq!(status.state, PipelineStateKind::Error);
        assert_eq!(
            status.error.as_deref(),
            Some("gather pipeline not yet wired")
        );
        assert!(
            status.last_result.is_some(),
            "an error must not erase the previous run's result"
        );
    }

    #[test]
    fn state_kind_serializes_lowercase_for_the_frontend() {
        let cases = [
            (PipelineStateKind::Idle, "\"idle\""),
            (PipelineStateKind::Gathering, "\"gathering\""),
            (PipelineStateKind::Rendering, "\"rendering\""),
            (PipelineStateKind::Done, "\"done\""),
            (PipelineStateKind::Error, "\"error\""),
        ];
        for (kind, expected) in cases {
            assert_eq!(serde_json::to_string(&kind).unwrap(), expected);
        }
    }

    #[test]
    fn status_serializes_with_the_contract_field_names() {
        let value = serde_json::to_value(PipelineStatus::default()).unwrap();
        let obj = value.as_object().expect("PipelineStatus is an object");
        let mut keys: Vec<&str> = obj.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(
            keys,
            [
                "current_date",
                "current_host",
                "error",
                "last_result",
                "last_run_at",
                "percent",
                "state",
                "step",
            ]
        );
    }
}
