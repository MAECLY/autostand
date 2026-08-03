//! `Remember` plugin data source (narrative notes, last resort).
//! See `docs/data-sources/04-remember-plugin.md`.

use std::path::{Path, PathBuf};

use async_trait::async_trait;
use chrono::Local;
use tokio::fs;

use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// Remember-plugin note reader.
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
        DataSourceConfig::from_env().github_dir.is_dir()
    }

    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        if !config.github_dir.is_dir() {
            return Ok(SourceData::default());
        }
        let mut notes: Vec<String> = Vec::new();

        let mut remember_dirs: Vec<PathBuf> = Vec::new();
        scan_remember_dirs(&config.github_dir, 0, 3, &mut remember_dirs);

        for dir in &remember_dirs {
            for date in &window.dates {
                let ds = date.format("%Y-%m-%d").to_string();
                for fname in [format!("today-{ds}.md"), format!("today-{ds}.done.md")] {
                    let p = dir.join(&fname);
                    if let Ok(body) = fs::read_to_string(&p).await {
                        if !body.trim().is_empty() {
                            notes.push(format!("## {ds}\n{}", body.trim()));
                        }
                    }
                }
            }
        }

        let today = Local::now().date_naive();
        if window.end == today {
            let now_md = config.github_dir.join(".remember").join("now.md");
            if let Ok(body) = fs::read_to_string(&now_md).await {
                if !body.trim().is_empty() {
                    notes.push(format!("## {}\n{}", today.format("%Y-%m-%d"), body.trim()));
                }
            }
        }

        if notes.is_empty() {
            return Ok(SourceData::default());
        }

        Ok(SourceData {
            facts: None,
            notes: Some(notes.join("\n\n")),
            enrichment: None,
            files: Vec::new(),
        })
    }
}

fn scan_remember_dirs(dir: &Path, depth: usize, max_depth: usize, out: &mut Vec<PathBuf>) {
    if depth >= max_depth {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        if p.file_name().and_then(|n| n.to_str()) == Some(".remember") {
            out.push(p);
            continue;
        }
        scan_remember_dirs(&p, depth + 1, max_depth, out);
    }
}
