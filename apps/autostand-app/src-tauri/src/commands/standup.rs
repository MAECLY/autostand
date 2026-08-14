//! Standup file + audit sidecar IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `read_standup_file`,
//! `add_manual_item`, `list_standup_dates`, `list_audit_sidecars`,
//! `read_audit_sidecar`.

use std::path::{Path, PathBuf};

use serde_json::Value;

use tauri::AppHandle;

use crate::commands::{
    load_config, resolve_dir, state_dir,
    types::{
        AuditData, AuditRenderMode, AuditSidecar, DateRangeDto, RenderUsed, StandupFileContent,
    },
};
use crate::error::AppError;

/// Fallback location for `dailies_dir` under the user's home.
const DAILIES_DIR_FALLBACK: &[&str] = &["Sync", "Github_Dailies", "dailies"];

/// Resolve the dailies directory: the configured `dailies_dir`, else
/// `<home>/Sync/Github_Dailies/dailies`.
pub(crate) fn resolve_dailies_dir(configured: Option<&str>, home: Option<&Path>) -> PathBuf {
    resolve_dir(configured, home, DAILIES_DIR_FALLBACK)
}

/// Resolve the dailies directory from the persisted config.
fn dailies_dir(app: &impl tauri::Manager<tauri::Wry>) -> PathBuf {
    let config = load_config(app).ok();
    resolve_dailies_dir(
        config.as_ref().map(|c| c.dailies_dir.as_str()),
        dirs::home_dir().as_deref(),
    )
}

/// Audit sidecar dir (`<state_dir>/audit`).
fn audit_dir() -> PathBuf {
    state_dir().join("audit")
}

/// Extract the host slug from an audit sidecar filename `<date>-<host>.json`.
///
/// Returns `None` for anything that is not a `.json` sidecar for `prefix`
/// (which already carries its trailing `-`).
fn sidecar_host(file_name: &str, prefix: &str) -> Option<String> {
    let path = Path::new(file_name);
    if !path
        .extension()
        .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
    {
        return None;
    }
    path.file_stem()?
        .to_str()?
        .strip_prefix(prefix)
        .filter(|host| !host.is_empty())
        .map(str::to_string)
}

/// Map the sidecar's `render_used` string onto the IPC enum.
///
/// Unknown values degrade to `Det`: the deterministic body is always computed,
/// so it is the only safe thing to claim about an unreadable sidecar.
fn parse_render_used(raw: &str) -> RenderUsed {
    match raw {
        "llm" => RenderUsed::Llm,
        "llm_fallback" => RenderUsed::LlmFallback,
        _ => RenderUsed::Det,
    }
}

/// Map the sidecar's `render_mode` string onto the IPC enum (default `auto`).
fn parse_render_mode(raw: &str) -> AuditRenderMode {
    match raw {
        "llm" => AuditRenderMode::Llm,
        "det" => AuditRenderMode::Det,
        _ => AuditRenderMode::Auto,
    }
}

/// Read a string field, defaulting to empty when absent or the wrong type.
fn str_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

/// Read an optional string field (absent / non-string ⇒ `None`).
fn opt_str_field(value: &Value, key: &str) -> Option<String> {
    value.get(key).and_then(Value::as_str).map(str::to_string)
}

/// Map the sidecar's `window: { start, end }` onto the contract's
/// `{ range_start, range_end }`.
fn parse_window(value: &Value) -> DateRangeDto {
    value
        .get("window")
        .map_or_else(DateRangeDto::default, |w| DateRangeDto {
            range_start: str_field(w, "start"),
            range_end: str_field(w, "end"),
        })
}

/// Parse `YYYY-MM-DD.md` (the on-disk standup name) into a filing date.
fn date_from_standup_filename(name: &str) -> Option<chrono::NaiveDate> {
    let stem = name.strip_suffix(".md")?;
    chrono::NaiveDate::parse_from_str(stem, "%Y-%m-%d").ok()
}

/// Keep stems whose date falls in `[start, end]`, sorted and unique.
fn collect_standup_dates<I>(
    names: I,
    start: chrono::NaiveDate,
    end: chrono::NaiveDate,
) -> Vec<String>
where
    I: IntoIterator<Item = String>,
{
    let mut dates: Vec<String> = names
        .into_iter()
        .filter_map(|name| {
            let date = date_from_standup_filename(&name)?;
            (date >= start && date <= end).then(|| date.format("%Y-%m-%d").to_string())
        })
        .collect();
    dates.sort();
    dates.dedup();
    dates
}

/// List standup filing dates in `[since, until]` with a single `read_dir`.
///
/// A missing dailies directory is an empty list, not an error — a fresh
/// install has nothing filed yet. Unknown filenames (notes, `.tmp`, uppercase
/// extensions) are ignored so a messy folder cannot fail the calendar.
#[tauri::command]
pub async fn list_standup_dates(
    app_handle: AppHandle,
    since: String,
    until: String,
) -> Result<Vec<String>, AppError> {
    let start = chrono::NaiveDate::parse_from_str(&since, "%Y-%m-%d")
        .map_err(|e| AppError::Invalid(format!("invalid since '{since}': {e}")))?;
    let end = chrono::NaiveDate::parse_from_str(&until, "%Y-%m-%d")
        .map_err(|e| AppError::Invalid(format!("invalid until '{until}': {e}")))?;
    if end < start {
        return Err(AppError::Invalid(format!(
            "until '{until}' is before since '{since}'"
        )));
    }
    let dir = dailies_dir(&app_handle);
    let entries = match std::fs::read_dir(&dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(err) => return Err(err.into()),
    };
    let mut names = Vec::new();
    for entry in entries {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str() {
            names.push(name.to_string());
        }
    }
    Ok(collect_standup_dates(names, start, end))
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
        AppError::NotFound(format!("standup file {} not found: {e}", path.display()))
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
    for entry in std::fs::read_dir(&dir)? {
        let path = entry?.path();
        let Some(host) = path
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|name| sidecar_host(name, &prefix))
        else {
            continue;
        };
        // An unreadable sidecar still gets listed — the UI needs the path to
        // report it — with empty provenance rather than a hard failure.
        let fields = parse_sidecar_fields(&path).unwrap_or_default();
        out.push(AuditSidecar {
            path: path.to_string_lossy().to_string(),
            date: date.clone(),
            host,
            rendered_at: fields.rendered_at,
            render_used: fields.render_used,
            provider: fields.provider,
            model: fields.model,
            fellback: fields.fellback,
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
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|e| AppError::Config(format!("parse audit sidecar: {e}")))?;
    Ok(audit_data_from_json(&value))
}

/// Project a sidecar JSON document onto the `AuditData` contract shape.
///
/// Deliberately lenient (missing keys default) — sidecars are written by
/// `autostand-core` and by older versions of it, and a partial audit is still
/// worth showing.
fn audit_data_from_json(value: &Value) -> AuditData {
    AuditData {
        file: str_field(value, "file"),
        host: str_field(value, "host"),
        rendered_at: str_field(value, "rendered_at"),
        window: parse_window(value),
        // Collections are not projected yet: the gather pipeline that fills
        // them is still unwired, so an empty vec is honest rather than partial.
        facts: Vec::new(),
        notes: Vec::new(),
        github: opt_str_field(value, "github"),
        conv: opt_str_field(value, "conv"),
        prrev: opt_str_field(value, "prrev"),
        claude_files: Vec::new(),
        opencode_sessions: Vec::new(),
        codex_sessions: Vec::new(),
        gemini_sessions: Vec::new(),
        grok_sessions: Vec::new(),
        forbidden_tickets: Vec::new(),
        covered_tickets: Vec::new(),
        skew: Vec::new(),
        ticket_days: std::collections::BTreeMap::new(),
        render_mode: parse_render_mode(
            value
                .get("render_mode")
                .and_then(Value::as_str)
                .unwrap_or("auto"),
        ),
        render_used: parse_render_used(
            value
                .get("render_used")
                .and_then(Value::as_str)
                .unwrap_or("det"),
        ),
        provider: opt_str_field(value, "provider"),
        model: opt_str_field(value, "model"),
        provider_attempts: value
            .get("provider_attempts")
            .cloned()
            .and_then(|attempts| serde_json::from_value(attempts).ok())
            .unwrap_or_default(),
        fellback: value
            .get("fellback")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        hash: str_field(value, "hash"),
        accumulated_count: value
            .get("accumulated_count")
            .and_then(Value::as_u64)
            .and_then(|n| u32::try_from(n).ok())
            .unwrap_or(0),
    }
}

/// The slice of a sidecar the listing needs, without a full `AuditData` parse.
///
/// `provider` / `model` / `fellback` are render provenance: the Dashboard shows
/// them next to the standup, never inside it.
#[derive(Debug, Default, PartialEq, Eq)]
struct SidecarFields {
    rendered_at: String,
    render_used: RenderUsed,
    provider: Option<String>,
    model: Option<String>,
    fellback: bool,
}

/// Project the listing fields out of an already-parsed sidecar document.
fn sidecar_fields_from_json(value: &Value) -> SidecarFields {
    SidecarFields {
        rendered_at: str_field(value, "rendered_at"),
        render_used: parse_render_used(
            value
                .get("render_used")
                .and_then(Value::as_str)
                .unwrap_or("det"),
        ),
        provider: opt_str_field(value, "provider"),
        model: opt_str_field(value, "model"),
        fellback: value
            .get("fellback")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// Extract the listing fields from a sidecar without a full parse.
fn parse_sidecar_fields(path: &Path) -> Result<SidecarFields, AppError> {
    let bytes = std::fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)?;
    Ok(sidecar_fields_from_json(&value))
}

#[cfg(test)]
mod tests {
    use super::{
        audit_data_from_json, collect_standup_dates, date_from_standup_filename, parse_render_mode,
        parse_render_used, parse_window, resolve_dailies_dir, sidecar_fields_from_json,
        sidecar_host, SidecarFields,
    };
    use crate::commands::types::{AuditRenderMode, RenderUsed};
    use serde_json::json;
    use std::path::{Path, PathBuf};

    #[test]
    fn standup_filename_accepts_iso_markdown_only() {
        assert_eq!(
            date_from_standup_filename("2026-08-03.md").map(|d| d.to_string()),
            Some("2026-08-03".to_string())
        );
        assert_eq!(date_from_standup_filename("2026-08-03.MD"), None);
        assert_eq!(date_from_standup_filename("2026-08-03.json"), None);
        assert_eq!(date_from_standup_filename("notes.md"), None);
        assert_eq!(date_from_standup_filename("2026-13-40.md"), None);
    }

    #[test]
    fn collect_standup_dates_keeps_the_inclusive_range_sorted() {
        let start = chrono::NaiveDate::from_ymd_opt(2026, 8, 1).expect("date");
        let end = chrono::NaiveDate::from_ymd_opt(2026, 8, 3).expect("date");
        let dates = collect_standup_dates(
            [
                "2026-08-03.md".into(),
                "readme.md".into(),
                "2026-08-01.md".into(),
                "2026-07-31.md".into(),
                "2026-08-02.md".into(),
                "2026-08-01.md".into(),
                "2026-08-04.md".into(),
            ],
            start,
            end,
        );
        assert_eq!(dates, ["2026-08-01", "2026-08-02", "2026-08-03"]);
    }

    #[test]
    fn dailies_dir_prefers_config_then_falls_back() {
        let home = Path::new("/home/tester");
        assert_eq!(
            resolve_dailies_dir(Some("/work/dailies"), Some(home)),
            PathBuf::from("/work/dailies")
        );
        assert_eq!(
            resolve_dailies_dir(Some(""), Some(home)),
            PathBuf::from("/home/tester/Sync/Github_Dailies/dailies")
        );
        assert_eq!(
            resolve_dailies_dir(None, Some(home)),
            PathBuf::from("/home/tester/Sync/Github_Dailies/dailies")
        );
    }

    #[test]
    fn sidecar_host_extracts_the_slug() {
        assert_eq!(
            sidecar_host("2026-08-03-MacStudio-de-Miguel.json", "2026-08-03-"),
            Some("MacStudio-de-Miguel".to_string())
        );
    }

    #[test]
    fn sidecar_host_accepts_uppercase_extensions() {
        // The listing must not depend on the case the sidecar was written with;
        // macOS and Windows filesystems are case-insensitive.
        assert_eq!(
            sidecar_host("2026-08-03-host.JSON", "2026-08-03-"),
            Some("host".to_string())
        );
    }

    #[test]
    fn sidecar_host_rejects_other_dates_and_non_json() {
        assert_eq!(sidecar_host("2026-08-02-host.json", "2026-08-03-"), None);
        assert_eq!(
            sidecar_host("2026-08-03-host.json.tmp", "2026-08-03-"),
            None
        );
        assert_eq!(sidecar_host("2026-08-03-host.md", "2026-08-03-"), None);
        assert_eq!(sidecar_host("2026-08-03-.json", "2026-08-03-"), None);
    }

    #[test]
    fn render_used_parses_the_three_contract_values() {
        assert_eq!(parse_render_used("llm"), RenderUsed::Llm);
        assert_eq!(parse_render_used("det"), RenderUsed::Det);
        assert_eq!(parse_render_used("llm_fallback"), RenderUsed::LlmFallback);
        assert_eq!(parse_render_used("nonsense"), RenderUsed::Det);
    }

    #[test]
    fn render_mode_parses_the_three_contract_values() {
        assert_eq!(parse_render_mode("auto"), AuditRenderMode::Auto);
        assert_eq!(parse_render_mode("llm"), AuditRenderMode::Llm);
        assert_eq!(parse_render_mode("det"), AuditRenderMode::Det);
        assert_eq!(parse_render_mode("nonsense"), AuditRenderMode::Auto);
    }

    #[test]
    fn window_is_renamed_from_start_end_to_range_start_range_end() {
        let value = json!({ "window": { "start": "2026-08-01", "end": "2026-08-02" } });
        let window = parse_window(&value);
        assert_eq!(window.range_start, "2026-08-01");
        assert_eq!(window.range_end, "2026-08-02");
    }

    #[test]
    fn window_defaults_when_absent() {
        let window = parse_window(&json!({}));
        assert!(window.range_start.is_empty());
        assert!(window.range_end.is_empty());
    }

    #[test]
    fn audit_data_projects_a_core_sidecar() {
        let value = json!({
            "file": "2026-08-03",
            "host": "MacStudio-de-Miguel",
            "rendered_at": "2026-08-03T07:00:00Z",
            "window": { "start": "2026-08-01", "end": "2026-08-02" },
            "render_mode": "auto",
            "render_used": "llm_fallback",
            "provider": "claude",
            "model": "opus",
            "fellback": true,
            "hash": "abc123",
            "accumulated_count": 3,
        });
        let audit = audit_data_from_json(&value);
        assert_eq!(audit.file, "2026-08-03");
        assert_eq!(audit.host, "MacStudio-de-Miguel");
        assert_eq!(audit.render_mode, AuditRenderMode::Auto);
        assert_eq!(audit.render_used, RenderUsed::LlmFallback);
        assert_eq!(audit.provider.as_deref(), Some("claude"));
        assert!(audit.fellback);
        assert_eq!(audit.accumulated_count, 3);
    }

    #[test]
    fn audit_data_tolerates_an_empty_document() {
        let audit = audit_data_from_json(&json!({}));
        assert!(audit.file.is_empty());
        assert_eq!(audit.render_mode, AuditRenderMode::Auto);
        assert_eq!(audit.render_used, RenderUsed::Det);
        assert!(audit.provider.is_none());
        assert_eq!(audit.accumulated_count, 0);
    }

    #[test]
    fn accumulated_count_saturates_instead_of_wrapping() {
        // A u64 that does not fit in u32 used to wrap via `as`, turning a bogus
        // sidecar value into a plausible-looking bullet count.
        let audit = audit_data_from_json(&json!({ "accumulated_count": u64::from(u32::MAX) + 1 }));
        assert_eq!(audit.accumulated_count, 0);
    }

    #[test]
    fn sidecar_fields_carry_the_render_provenance() {
        let fields = sidecar_fields_from_json(&json!({
            "rendered_at": "2026-08-03T07:00:00Z",
            "render_used": "llm_fallback",
            "provider": "ollama",
            "model": "llama3.1:8b",
            "fellback": true,
        }));
        assert_eq!(
            fields,
            SidecarFields {
                rendered_at: "2026-08-03T07:00:00Z".to_string(),
                render_used: RenderUsed::LlmFallback,
                provider: Some("ollama".to_string()),
                model: Some("llama3.1:8b".to_string()),
                fellback: true,
            }
        );
    }

    #[test]
    fn sidecar_fields_of_a_deterministic_render_name_no_provider() {
        // A deterministic render never contacted a provider, so the listing must
        // say "nothing" rather than invent one.
        let fields = sidecar_fields_from_json(&json!({
            "rendered_at": "2026-08-03T07:00:00Z",
            "render_used": "det",
        }));
        assert!(fields.provider.is_none());
        assert!(fields.model.is_none());
        assert!(!fields.fellback);
    }

    #[test]
    fn sidecar_fields_default_when_the_document_is_empty() {
        assert_eq!(
            sidecar_fields_from_json(&json!({})),
            SidecarFields::default()
        );
        assert_eq!(SidecarFields::default().render_used, RenderUsed::Det);
    }

    #[test]
    fn audit_data_round_trips_to_the_contract_wire_values() {
        let audit = audit_data_from_json(&json!({
            "render_mode": "det",
            "render_used": "llm",
        }));
        let value = serde_json::to_value(&audit).unwrap();
        assert_eq!(value["render_mode"], json!("det"));
        assert_eq!(value["render_used"], json!("llm"));
    }
}
