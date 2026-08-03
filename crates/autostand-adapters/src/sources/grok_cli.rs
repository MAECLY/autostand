//! `Grok` `CLI` data source (sessions + edited files).
//! See `docs/data-sources/08-grok-cli.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct GrokCliDataSource;

#[async_trait]
impl DataSource for GrokCliDataSource {
    fn id(&self) -> &'static str {
        "grok-cli"
    }
    fn display_name(&self) -> &'static str {
        "Grok CLI"
    }
    fn is_available(&self) -> bool {
        // Probe candidate paths: ~/.grok, ~/.config/grok, ~/.config/grok-cli
        let home = dirs::home_dir();
        let mut found = false;
        if let Some(h) = &home {
            for p in [".grok", ".config/grok", ".config/grok-cli", ".grok-cli"] {
                if h.join(p).exists() {
                    found = true;
                    break;
                }
            }
        }
        found
    }

    async fn gather(
        &self,
        _window: &DateWindow,
        _config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        Ok(SourceData::default())
    }
}
