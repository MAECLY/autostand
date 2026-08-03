//! Data source adapters. See `docs/data-sources/`.

pub mod claude_code;
pub mod codex;
pub mod gemini_cli;
pub mod github;
pub mod grok_cli;
pub mod local_git;
pub mod opencode;
pub mod remember;
pub mod traits;

pub use traits::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
