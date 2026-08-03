//! `Codex` `CLI` data source (sessions + edited files).
//! See `docs/data-sources/06-codex.md`.

use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use super::helpers::{attribute_file, PromptCollector};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// Codex CLI data source.
pub struct CodexDataSource;

#[async_trait]
impl DataSource for CodexDataSource {
    fn id(&self) -> &'static str {
        "codex"
    }
    fn display_name(&self) -> &'static str {
        "Codex CLI"
    }
    fn is_available(&self) -> bool {
        dirs::home_dir().is_some_and(|h| h.join(".codex").exists())
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
        let codex_dir = home.join(".codex");
        let sessions_dir = codex_dir.join("sessions");

        let mut collector = PromptCollector::default();
        let mut files: Vec<(String, String)> = Vec::new();

        for date in &window.dates {
            let y = date.format("%Y").to_string();
            let m = date.format("%m").to_string();
            let d = date.format("%d").to_string();
            let day_dir = sessions_dir.join(&y).join(&m).join(&d);
            if !day_dir.is_dir() {
                continue;
            }
            if let Ok(entries) = std::fs::read_dir(&day_dir) {
                for e in entries.flatten() {
                    let p = e.path();
                    if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                        continue;
                    }
                    if let Ok(body) = fs::read_to_string(&p).await {
                        parse_jsonl(&body, &mut collector, &mut files, &config.github_dir);
                    }
                }
            }
        }

        let history = codex_dir.join("history.jsonl");
        if history.is_file() {
            if let Ok(body) = fs::read_to_string(&history).await {
                for line in body.lines() {
                    if let Ok(v) = serde_json::from_str::<Value>(line) {
                        if let Some(text) = v.get("text").and_then(|t| t.as_str()) {
                            collector.add(text);
                        }
                    }
                }
            }
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
            "user_message" | "event_msg" => {
                if let Some(text) = v.get("text").and_then(|t| t.as_str()).or_else(|| {
                    v.get("content").and_then(|c| c.as_str()).or_else(|| {
                        v.get("payload")
                            .and_then(|p| p.get("text"))
                            .and_then(|t| t.as_str())
                    })
                }) {
                    collector.add(text);
                }
            }
            "function_call" => {
                extract_fn_paths(&v, files, github_dir);
            }
            _ => {}
        }
    }
}

fn extract_fn_paths(v: &Value, files: &mut Vec<(String, String)>, github_dir: &Path) {
    let probe = v
        .get("arguments")
        .cloned()
        .or_else(|| v.get("parameters").cloned())
        .or_else(|| v.get("input").cloned());
    if let Some(args) = probe {
        if let Some(obj) = args.as_object() {
            for key in ["file_path", "path"] {
                if let Some(s) = obj.get(key).and_then(|s| s.as_str()) {
                    if let Some(attr) = attribute_file(s, github_dir) {
                        if !files.contains(&attr) {
                            files.push(attr);
                        }
                    }
                }
            }
        } else if let Some(s) = args.as_str() {
            if let Ok(parsed) = serde_json::from_str::<Value>(s) {
                for key in ["file_path", "path"] {
                    if let Some(p) = parsed.get(key).and_then(|p| p.as_str()) {
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
