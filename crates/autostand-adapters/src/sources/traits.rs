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

/// Config passed to data sources.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DataSourceConfig {
    /// Root directory containing work repos (`GITHUB_DIR`).
    pub github_dir: std::path::PathBuf,
    /// Dailies output directory.
    pub dailies_dir: std::path::PathBuf,
    /// Git authors that count as "me" (regex OR).
    pub authors: Vec<String>,
    /// Git refs to scan (default `--all`).
    pub git_refs: String,
    /// GitHub reviewer login.
    pub gh_reviewer: String,
    /// GitHub org to search PRs.
    pub gh_pr_org: String,
    /// Max PRs fetched for comments.
    pub gh_max_prs: usize,
    /// Max chars per quoted review comment.
    pub gh_comment_len: usize,
    /// Jira ticket base URL.
    pub jira_base: String,
}

impl DataSourceConfig {
    /// Default config derived from env vars (`GITHUB_DIR`, `STANDUP_AUTHORS`, etc.).
    pub fn from_env() -> Self {
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."));
        Self {
            github_dir: std::env::var("GITHUB_DIR").map_or_else(
                |_| home.join("Documents").join("Github"),
                std::path::PathBuf::from,
            ),
            dailies_dir: std::env::var("DAILIES_DIR").map_or_else(
                |_| home.join("Sync").join("Github_Dailies"),
                std::path::PathBuf::from,
            ),
            authors: std::env::var("STANDUP_AUTHORS")
                .unwrap_or_else(|_| "miguel@fiftyflowers.com|miguel50flowers".into())
                .split('|')
                .map(String::from)
                .collect(),
            git_refs: std::env::var("STANDUP_GIT_REFS").unwrap_or_else(|_| "--all".into()),
            gh_reviewer: std::env::var("STANDUP_REVIEWER")
                .unwrap_or_else(|_| "miguel50flowers".into()),
            gh_pr_org: std::env::var("STANDUP_PR_ORG").unwrap_or_else(|_| "fifty-git".into()),
            gh_max_prs: std::env::var("STANDUP_GH_MAX_PRS")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(10),
            gh_comment_len: std::env::var("STANDUP_GH_COMMENT_LEN")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(220),
            jira_base: std::env::var("JIRA_BASE")
                .unwrap_or_else(|_| "https://fiftyflowers.atlassian.net/browse".into()),
        }
    }
}

/// The trait every data source implements.
#[async_trait]
pub trait DataSource: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    fn is_available(&self) -> bool;

    /// Gather data for the window.
    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError>;
}
