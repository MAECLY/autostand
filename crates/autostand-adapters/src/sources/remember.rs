//! `Remember` plugin data source (narrative notes, last resort).
//! See `docs/data-sources/04-remember-plugin.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct RememberDataSource;

#[async_trait]
impl DataSource for RememberDataSource {
    fn id(&self) -> &'static str {
        "remember"
    }
    fn display_name(&self) -> &'static str {
        "Remember plugin"
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn gather(
        &self,
        _window: &DateWindow,
        _config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        Ok(SourceData::default())
    }
}
