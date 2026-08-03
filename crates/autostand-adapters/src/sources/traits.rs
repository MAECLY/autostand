//! `DataSource` trait + shared types.
//!
//! See `docs/data-sources/00-sources-overview.md`.

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};

/// The date window for a compile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateWindow {
    pub start: NaiveDate,
    pub end: NaiveDate,
    pub dates: Vec<NaiveDate>,
}

/// Data gathered by a source.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SourceData {
    /// Structured facts (e.g. git commit blocks).
    pub facts: Option<String>,
    /// Narrative notes (e.g. .remember).
    pub notes: Option<String>,
    /// Conversation/file context (e.g. Claude Code digest).
    pub enrichment: Option<String>,
    /// Edited file basenames per repo.
    pub files: Vec<String>,
}

/// Data source errors.
#[derive(Debug, thiserror::Error)]
pub enum DataSourceError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("not available: {0}")]
    NotAvailable(String),
    #[error("parse: {0}")]
    Parse(String),
    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}

/// Config passed to data sources (placeholder — full `AppConfig` comes later).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataSourceConfig {
    pub github_dir: std::path::PathBuf,
    pub authors: Vec<String>,
    pub git_refs: String,
}

/// The trait every data source implements.
#[async_trait]
pub trait DataSource: Send + Sync {
    fn id(&self) -> &str;
    fn display_name(&self) -> &str;
    fn is_available(&self) -> bool;

    /// Gather data for the window.
    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError>;
}
