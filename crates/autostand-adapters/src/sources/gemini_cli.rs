//! `Gemini` `CLI` data source (sessions + edited files).
//! See `docs/data-sources/07-gemini-cli.md`.

use std::path::Path;

use async_trait::async_trait;
use serde_json::Value;
use tokio::fs;

use super::helpers::{attribute_file, read_jsonl_paths, PromptCollector};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// Gemini CLI data source.
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
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        let Some(home) = dirs::home_dir() else {
            return Ok(SourceData::default());
        };
        let gemini_dir = home.join(".gemini");
        if !gemini_dir.is_dir() {
            return Ok(SourceData::default());
        }

        let mut collector = PromptCollector::default();
        let mut files: Vec<(String, String)> = Vec::new();

        for jsonl in read_jsonl_paths(&gemini_dir) {
            if let Ok(body) = fs::read_to_string(&jsonl).await {
                parse_jsonl(&body, &mut collector, &mut files, &config.github_dir);
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
        let role = v.get("role").and_then(|r| r.as_str()).unwrap_or("");
        let ty = v.get("type").and_then(|t| t.as_str()).unwrap_or("");
        if role == "user" || ty == "user" || ty == "user_message" {
            if let Some(text) = extract_text(&v) {
                collector.add(&text);
            }
        }
        if ty == "tool_use" || ty == "function_call" || ty == "tool" {
            extract_paths(&v, files, github_dir);
        }
    }
}

fn extract_text(v: &Value) -> Option<String> {
    if let Some(s) = v.get("text").and_then(|t| t.as_str()) {
        return Some(s.to_string());
    }
    if let Some(s) = v.get("content").and_then(|c| c.as_str()) {
        return Some(s.to_string());
    }
    if let Some(arr) = v.get("content").and_then(|c| c.as_array()) {
        let mut parts = Vec::new();
        for item in arr {
            if let Some(t) = item.get("text").and_then(|t| t.as_str()) {
                parts.push(t.to_string());
            }
        }
        if !parts.is_empty() {
            return Some(parts.join("\n"));
        }
    }
    None
}

fn extract_paths(v: &Value, files: &mut Vec<(String, String)>, github_dir: &Path) {
    let probe = v
        .get("arguments")
        .cloned()
        .or_else(|| v.get("input").cloned())
        .or_else(|| v.get("parameters").cloned())
        .or_else(|| Some(v.clone()));
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
        }
    }
}
