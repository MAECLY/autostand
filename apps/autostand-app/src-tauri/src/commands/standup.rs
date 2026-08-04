//! Standup file + audit sidecar IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `read_standup_file`,
//! `add_manual_item`, `list_audit_sidecars`, `read_audit_sidecar`.

use std::path::PathBuf;

use tauri::AppHandle;

use crate::commands::{
    load_config, state_dir,
    types::{AuditData, AuditSidecar, RenderUsed, StandupFileContent},
};
use crate::error::AppError;

/// Resolve the dailies directory from config (default `<repo>/dailies/`).
fn dailies_dir(app: &impl tauri::Manager<tauri::Wry>) -> PathBuf {
    let config = load_config(app).ok();
    let dir = config
        .map(|c| c.dailies_dir)
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .map(|h| h.join("Sync").join("Github_Dailies").join("dailies"))
                .unwrap_or_default()
        });
    dir
}

/// Audit sidecar dir (`<state_dir>/audit`).
fn audit_dir() -> PathBuf {
    state_dir().join("audit")
}

/// Read and parse a standup file for a date.
#[tauri::command]
pub async fn read_standup_file(
    app_handle: AppHandle,
    date: String,
) -> Result<StandupFileContent, AppError> {
    let parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|e| AppError::Invalid(format!("invalid date '{date}': {e}")))?;
    let path = dailies_dir(&app_handle).join(format!("{parsed}.md"));
    let content = std::fs::read_to_string(&path).map_err(|e| {
        AppError::NotFound(format!(
            "standup file {} not found: {e}",
            path.display()
        ))
    })?;
    let file = autostand_core::format::parse(&content)
        .map_err(|e| AppError::Config(format!("parse standup file: {e}")))?;
    let auto_blocks = file
        .auto_blocks
        .into_iter()
        .map(|b| crate::commands::types::AutoBlockDto {
            host: b.host,
            body: b.body,
        })
        .collect();
    Ok(StandupFileContent {
        date,
        title: file.title,
        subtitle: file.subtitle,
        auto_blocks,
        manual_region: file.manual.body,
    })
}

/// Append an item to the MANUAL region of a standup file (atomic).
#[tauri::command]
pub async fn add_manual_item(
    app_handle: AppHandle,
    date: String,
    item: String,
) -> Result<(), AppError> {
    let parsed = chrono::NaiveDate::parse_from_str(&date, "%Y-%m-%d")
        .map_err(|e| AppError::Invalid(format!("invalid date '{date}': {e}")))?;
    let dir = dailies_dir(&app_handle);
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{parsed}.md"));
    autostand_core::fileops::add_manual(&path, &item)
        .map_err(|e| AppError::Config(format!("add_manual: {e}")))
}

/// List audit sidecars for a date.
#[tauri::command]
pub async fn list_audit_sidecars(date: String) -> Result<Vec<AuditSidecar>, AppError> {
    let dir = audit_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let prefix = format!("{date}-");
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with(&prefix) || !name.ends_with(".json") {
            continue;
        }
        let full = path.to_string_lossy().to_string();
        let host = name
            .strip_prefix(&prefix)
            .and_then(|s| s.strip_suffix(".json"))
            .unwrap_or("")
            .to_string();
        let rendered_at;
        let render_used;
        match parse_sidecar_fields(&path) {
            Ok((at, used)) => {
                rendered_at = at;
                render_used = used;
            }
            Err(_) => {
                rendered_at = String::new();
                render_used = RenderUsed::Det;
            }
        }
        out.push(AuditSidecar {
            path: full,
            date: date.clone(),
            host,
            rendered_at,
            render_used,
        });
    }
    out.sort_by(|a, b| a.host.cmp(&b.host));
    Ok(out)
}

/// Read and parse a single audit sidecar JSON.
#[tauri::command]
pub async fn read_audit_sidecar(path: String) -> Result<AuditData, AppError> {
    let bytes = std::fs::read(&path)
        .map_err(|e| AppError::NotFound(format!("audit sidecar {path}: {e}")))?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Config(format!("parse audit sidecar: {e}")))?;
    let window = value
        .get("window")
        .and_then(|w| {
            Some(crate::commands::types::DateRangeDto {
                range_start: w
                    .get("start")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
                range_end: w
                    .get("end")
                    .and_then(|s| s.as_str())
                    .unwrap_or("")
                    .to_string(),
            })
        })
        .unwrap_or_default();
    let render_mode = value
        .get("render_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("auto")
        .to_string();
    let render_used = match value
        .get("render_used")
        .and_then(|v| v.as_str())
        .unwrap_or("det")
    {
        "llm" => RenderUsed::Llm,
        "llm_fallback" => RenderUsed::LlmFallback,
        _ => RenderUsed::Det,
    };
    Ok(AuditData {
        file: value
            .get("file")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        host: value
            .get("host")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        rendered_at: value
            .get("rendered_at")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        window,
        facts: Vec::new(),
        notes: Vec::new(),
        github: value
            .get("github")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        conv: value
            .get("conv")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        prrev: value
            .get("prrev")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        claude_files: Vec::new(),
        opencode_sessions: Vec::new(),
        codex_sessions: Vec::new(),
        gemini_sessions: Vec::new(),
        grok_sessions: Vec::new(),
        forbidden_tickets: Vec::new(),
        covered_tickets: Vec::new(),
        skew: Vec::new(),
        ticket_days: std::collections::BTreeMap::new(),
        render_mode,
        render_used,
        provider: value
            .get("provider")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        model: value
            .get("model")
            .and_then(|v| v.as_str())
            .map(str::to_string),
        fellback: value
            .get("fellback")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        hash: value
            .get("hash")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string(),
        accumulated_count: value
            .get("accumulated_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
    })
}

/// Extract `rendered_at` + `render_used` from a sidecar without a full parse.
fn parse_sidecar_fields(path: &std::path::Path) -> Result<(String, RenderUsed), AppError> {
    let bytes = std::fs::read(path)?;
    let value: serde_json::Value = serde_json::from_slice(&bytes)?;
    let rendered_at = value
        .get("rendered_at")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let render_used = match value
        .get("render_used")
        .and_then(|v| v.as_str())
        .unwrap_or("det")
    {
        "llm" => RenderUsed::Llm,
        "llm_fallback" => RenderUsed::LlmFallback,
        _ => RenderUsed::Det,
    };
    Ok((rendered_at, render_used))
}