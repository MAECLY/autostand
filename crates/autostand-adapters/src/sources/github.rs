//! `GitHub` data source (via `gh` `CLI`).
//! See `docs/data-sources/02-github.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct GithubDataSource;

#[async_trait]
impl DataSource for GithubDataSource {
    fn id(&self) -> &'static str {
        "github"
    }
    fn display_name(&self) -> &'static str {
        "GitHub (gh CLI)"
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
