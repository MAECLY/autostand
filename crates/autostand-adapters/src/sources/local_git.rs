//! Local `Git` data source (authoritative commits).
//! See `docs/data-sources/01-local-git.md`.

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};
use async_trait::async_trait;

pub struct LocalGitDataSource;

#[async_trait]
impl DataSource for LocalGitDataSource {
    fn id(&self) -> &'static str {
        "local-git"
    }
    fn display_name(&self) -> &'static str {
        "Local Git"
    }
    fn is_available(&self) -> bool {
        true
    }

    async fn gather(
        &self,
        _window: &DateWindow,
        _config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        // TODO: scan GITHUB_DIR/*/.git, git log --all --no-merges --since/--until --author
        Ok(SourceData::default())
    }
}
