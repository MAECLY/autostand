//! `Gemini` `CLI` data source (sessions + edited files).
//! See `docs/data-sources/07-gemini-cli.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct GeminiCliDataSource;

#[async_trait]
impl DataSource for GeminiCliDataSource {
    fn id(&self) -> &'static str {
        "gemini-cli"
    }
    fn display_name(&self) -> &'static str {
        "Gemini CLI"
    }
    fn is_available(&self) -> bool {
        dirs::home_dir().is_some_and(|h| h.join(".gemini").exists())
    }

    async fn gather(
        &self,
        _window: &DateWindow,
        _config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        Ok(SourceData::default())
    }
}
