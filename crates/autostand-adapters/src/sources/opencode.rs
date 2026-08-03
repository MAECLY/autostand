//! `OpenCode` data source (`SQLite` + legacy `JSON`).
//! See `docs/data-sources/05-opencode.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct OpenCodeDataSource;

#[async_trait]
impl DataSource for OpenCodeDataSource {
    fn id(&self) -> &'static str {
        "opencode"
    }
    fn display_name(&self) -> &'static str {
        "OpenCode"
    }
    fn is_available(&self) -> bool {
        dirs::data_local_dir().is_some_and(|d| d.join("opencode").exists())
    }

    async fn gather(
        &self,
        _window: &DateWindow,
        _config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        Ok(SourceData::default())
    }
}
