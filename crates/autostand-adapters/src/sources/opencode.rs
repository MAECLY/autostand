//! `OpenCode` data source (`SQLite` + legacy `JSON`).
//! See `docs/data-sources/05-opencode.md`.

use std::path::Path;

use async_trait::async_trait;
use rusqlite::OpenFlags;
use serde_json::Value;
use tokio::fs;

use super::helpers::{attribute_file, PromptCollector};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// `OpenCode` (`SQLite` + legacy JSON) data source.
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
        if let Some(d) = dirs::data_local_dir() {
            if d.join("opencode").join("opencode.db").exists() {
                return true;
            }
            if d.join("opencode").join("storage").join("session").is_dir() {
                return true;
            }
        }
        false
    }

    #[allow(clippy::too_many_lines)]
    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        let Some(local) = dirs::data_local_dir() else {
            return Ok(SourceData::default());
        };
        let oc_dir = local.join("opencode");
        let db_path = oc_dir.join("opencode.db");
        let legacy_dir = oc_dir.join("storage").join("session");

        let mut collector = PromptCollector::default();
        let mut files: Vec<(String, String)> = Vec::new();

        if db_path.exists() {
            if let Err(e) = read_sqlite(
                &db_path,
                window,
                &mut collector,
                &mut files,
                &config.github_dir,
            ) {
                tracing::warn!("opencode: sqlite read failed: {e}");
            }
        } else if legacy_dir.is_dir() {
            read_legacy(&legacy_dir, &mut collector, &mut files, &config.github_dir).await;
        }

        if collector.snippets().is_empty() && files.is_empty() {
            return Ok(SourceData::default());
        }

        let enrichment = collector.render_context(&[]);
        let files_list: Vec<String> = files.iter().map(|(r, f)| format!("{r}/{f}")).collect();

        Ok(SourceData {
            facts: None,
            notes: None,
            enrichment: Some(enrichment),
            files: files_list,
        })
    }
}

fn read_sqlite(
    db_path: &Path,
    window: &DateWindow,
    collector: &mut PromptCollector,
    files: &mut Vec<(String, String)>,
    github_dir: &Path,
) -> Result<(), String> {
    let conn = rusqlite::Connection::open_with_flags(db_path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .map_err(|e| e.to_string())?;

    let start_ts = window
        .start
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc()
        .timestamp();
    let end_ts = window
        .end
        .and_hms_opt(23, 59, 59)
        .unwrap()
        .and_utc()
        .timestamp();

    let mut stmt = conn
        .prepare(
            "SELECT p.text FROM part p \
             JOIN message m ON p.message_id = m.id \
             JOIN session s ON m.session_id = s.id \
             WHERE m.role = 'user' \
             AND s.created_at >= ?1 AND s.created_at <= ?2",
        )
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map(rusqlite::params![start_ts, end_ts], |row| {
            row.get::<_, Option<String>>(0)
        })
        .map_err(|e| e.to_string())?;
    for t in rows.flatten().flatten() {
        collector.add(&t);
    }

    let mut fstmt = conn
        .prepare("SELECT p.text, p.type FROM part p JOIN message m ON p.message_id = m.id")
        .map_err(|e| e.to_string())?;
    let frows = fstmt
        .query_map([], |row| {
            Ok((
                row.get::<_, Option<String>>(0).unwrap_or_default(),
                row.get::<_, Option<String>>(1).unwrap_or_default(),
            ))
        })
        .map_err(|e| e.to_string())?;
    for fr in frows.flatten() {
        let (text, ty) = fr;
        if !matches!(ty.as_deref(), Some("tool" | "tool_call" | "file")) {
            continue;
        }
        if let Some(t) = text {
            if let Ok(v) = serde_json::from_str::<Value>(&t) {
                for key in ["file_path", "path"] {
                    if let Some(p) = v.get(key).and_then(|p| p.as_str()) {
                        if let Some(attr) = attribute_file(p, github_dir) {
                            if !files.contains(&attr) {
                                files.push(attr);
                            }
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

async fn read_legacy(
    dir: &Path,
    collector: &mut PromptCollector,
    files: &mut Vec<(String, String)>,
    github_dir: &Path,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.extension().and_then(|x| x.to_str()) != Some("json") {
            continue;
        }
        if let Ok(body) = fs::read_to_string(&p).await {
            if let Ok(v) = serde_json::from_str::<Value>(&body) {
                if let Some(messages) = v.get("messages").and_then(|m| m.as_array()) {
                    for msg in messages {
                        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                        if role == "user" {
                            if let Some(text) = msg
                                .get("content")
                                .and_then(|c| c.as_str())
                                .or_else(|| msg.get("text").and_then(|t| t.as_str()))
                            {
                                collector.add(text);
                            }
                        }
                        if let Some(parts) = msg.get("parts").and_then(|p| p.as_array()) {
                            for part in parts {
                                let ty = part.get("type").and_then(|t| t.as_str()).unwrap_or("");
                                if !matches!(ty, "tool" | "tool_call" | "file") {
                                    continue;
                                }
                                for key in ["file_path", "path"] {
                                    if let Some(p) = part.get(key).and_then(|p| p.as_str()) {
                                        if let Some(attr) = attribute_file(p, github_dir) {
                                            if !files.contains(&attr) {
                                                files.push(attr);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
