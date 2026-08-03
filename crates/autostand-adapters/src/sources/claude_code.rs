//! `Claude` `Code` data source (sessions + edited files).
//! See `docs/data-sources/03-claude-code.md`.

use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use super::helpers::{attribute_file, read_jsonl_paths, PromptCollector};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// Claude Code sessions data source.
pub struct ClaudeCodeDataSource;

const EDIT_TOOLS: &[&str] = &[
    "Edit",
    "Write",
    "MultiEdit",
    "NotebookEdit",
    "Update",
    "Create",
];

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

    #[allow(clippy::too_many_lines)]
    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        let Some(home) = dirs::home_dir() else {
            return Ok(SourceData::default());
        };
        let claude_dir = home.join(".claude");
        let projects_dir = claude_dir.join("projects");
        let plans_dir = claude_dir.join("plans");

        let mut collector = PromptCollector::default();
        let mut files: Vec<(String, String)> = Vec::new();

        if projects_dir.is_dir() {
            if let Ok(subdirs) = std::fs::read_dir(&projects_dir) {
                for sub in subdirs.flatten() {
                    if !sub.path().is_dir() {
                        continue;
                    }
                    for jsonl in read_jsonl_paths(&sub.path()) {
                        if collector.is_full() && !files.is_empty() {
                            break;
                        }
                        if let Ok(body) = fs::read_to_string(&jsonl).await {
                            parse_jsonl(&body, &mut collector, &mut files, &config.github_dir);
                        }
                    }
                }
            }
        }

        let mut plans: Vec<String> = Vec::new();
        if plans_dir.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&plans_dir) {
                let mut plan_files: Vec<(std::path::PathBuf, std::time::SystemTime)> = Vec::new();
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) == Some("md") {
                        if let Ok(meta) = e.metadata() {
                            if let Ok(mtime) = meta.modified() {
                                plan_files.push((p, mtime));
                            }
                        }
                    }
                }
                plan_files.sort_by_key(|(_, m)| *m);
                for (p, mtime) in plan_files {
                    let date = system_time_to_date(mtime);
                    if !window_contains(window, date) {
                        continue;
                    }
                    if let Ok(body) = fs::read_to_string(&p).await {
                        if let Some(title) = body.lines().next() {
                            let title = title.trim_start_matches('#').trim().to_string();
                            plans.push(title);
                        }
                    }
                }
            }
        }

        let enrichment = collector.render_context(&plans);
        let files_list: Vec<String> = files.iter().map(|(r, f)| format!("{r}/{f}")).collect();

        Ok(SourceData {
            facts: None,
            notes: None,
            enrichment: Some(enrichment),
            files: files_list,
        })
    }
}

fn window_contains(window: &DateWindow, date: chrono::NaiveDate) -> bool {
    date >= window.start && date <= window.end
}

fn system_time_to_date(t: std::time::SystemTime) -> chrono::NaiveDate {
    let dt = chrono::DateTime::<chrono::Utc>::from(t);
    dt.date_naive()
}

fn parse_jsonl(
    body: &str,
    collector: &mut PromptCollector,
    files: &mut Vec<(String, String)>,
    github_dir: &Path,
) {
    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        match ty {
            "user" => {
                if let Some(text) = extract_user_text(&v) {
                    collector.add(&text);
                }
            }
            "assistant" => {
                extract_tool_paths(&v, files, github_dir);
            }
            _ => {}
        }
    }
}

fn extract_user_text(v: &Value) -> Option<String> {
    let msg = v.get("message")?;
    let content = msg.get("content")?;
    if let Some(s) = content.as_str() {
        return Some(s.to_string());
    }
    if let Some(arr) = content.as_array() {
        let mut parts = Vec::new();
        for item in arr {
            let item_ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
            if item_ty == "text" {
                if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                    parts.push(t.to_string());
                }
            }
        }
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }
    None
}

fn extract_tool_paths(v: &Value, files: &mut Vec<(String, String)>, github_dir: &Path) {
    let Some(msg) = v.get("message") else {
        return;
    };
    let Some(content) = msg.get("content").and_then(|c| c.as_array()) else {
        return;
    };
    for item in content {
        let item_ty = item.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if item_ty != "tool_use" {
            continue;
        }
        let tool_name = item.get("name").and_then(|n| n.as_str()).unwrap_or("");
        if !EDIT_TOOLS.contains(&tool_name) {
            continue;
        }
        let Some(input) = item.get("input") else {
            continue;
        };
        for key in ["file_path", "notebook_path", "path"] {
            if let Some(p) = input.get(key).and_then(|p| p.as_str()) {
                if let Some(attr) = attribute_file(p, github_dir) {
                    if !files.contains(&attr) {
                        files.push(attr);
                    }
                }
            }
        }
    }
}
