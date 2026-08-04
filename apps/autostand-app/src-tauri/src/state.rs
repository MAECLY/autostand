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
    pub fn status(&self) -> PipelineStatus {
        self.status
            .lock()
            .map(|s| s.clone())
            .unwrap_or_default()
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