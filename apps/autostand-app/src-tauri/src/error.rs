//! Application-level error type shared by all Tauri IPC commands.
//!
//! `AppError` serializes to `{ code, message }` so the frontend can branch on `code`.
//! See `docs/tauri/02-ipc-contracts.md` § Error handling.

use serde::Serialize;
use thiserror::Error;

/// Single error enum for all IPC command failures.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", content = "message")]
pub enum AppError {
    /// Configuration load/save/validate failure.
    #[error("config: {0}")]
    Config(String),
    /// Filesystem / IO failure.
    #[error("io: {0}")]
    Io(String),
    /// Git CLI invocation failure.
    #[error("git: {0}")]
    Git(String),
    /// LLM provider failure (CLI/API).
    #[error("llm: {0}")]
    Llm(String),
    /// Pipeline lock contention (another compile in progress).
    #[error("lock: {0}")]
    Lock(String),
    /// Resource not found (file, slug, provider, etc).
    #[error("not_found: {0}")]
    NotFound(String),
    /// Invalid input (slug, cron, provider id, date).
    #[error("invalid: {0}")]
    Invalid(String),
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::Config(format!("serde: {err}"))
    }
}