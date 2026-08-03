//! `Claude` `Code` data source (sessions + edited files).
//! See `docs/data-sources/03-claude-code.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct ClaudeCodeDataSource;

#[async_trait]
impl DataSource for ClaudeCodeDataSource {
    fn id(&self) -> &'static str {
        "claude-code"
    }
    fn display_name(&self) -> &'static str {
        "Claude Code"
    }
    fn is_available(&self) -> bool {
        dirs::home_dir().is_some_and(|h| h.join(".claude").exists())
    }

    async fn gather(
        &self,
        _window: &DateWindow,
        _config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        Ok(SourceData::default())
    }
}
