//! Safe, two-phase Compile Now regeneration.
//!
//! `preview_regeneration` runs the real single-date pipeline in an isolated
//! directory and persists only a short-lived candidate. `apply_regeneration`
//! accepts an explicit resolution, checks that the live file still matches the
//! preview base hash, and surgically updates this machine's AUTO block. The
//! MANUAL region and other hosts are never part of the editable payload.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tauri::AppHandle;

use autostand_scheduler::lock;
use autostand_scheduler::triggers::TriggerSource;

use super::types::{
    CompileStatus, RegenerationApplied, RegenerationPreview, RegenerationResolution, RenderUsed,
};
use crate::error::AppError;
use crate::pipeline_runner::{CompilePurpose, RunEnv};
use crate::state::AppState;

const DATE_FORMAT: &str = "%Y-%m-%d";
const PENDING_SUBDIR: &str = "pending-regenerations";
const WORK_SUBDIR: &str = "regeneration-work";
const LOCK_SUBDIR: &str = "lock";
const PREVIEW_TTL_MINUTES: i64 = 30;
const MAX_MERGED_CHARS: usize = 50_000;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingRegeneration {
    token: String,
    date: String,
    host: String,
    base_hash: String,
    current_auto: String,
    candidate_auto: String,
    expires_at: String,
    render_used: RenderUsed,
    fellback: bool,
    message: String,
    audit_json: Option<String>,
}

struct WorkDir(PathBuf);

impl Drop for WorkDir {
    fn drop(&mut self) {
        if let Err(error) = std::fs::remove_dir_all(&self.0) {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(path = %self.0.display(), %error, "could not clean regeneration work directory");
            }
        }
    }
}

/// Generate a fresh candidate for exactly one filing date without modifying,
/// auditing, committing, or pushing the current standup.
#[tauri::command]
pub async fn preview_regeneration(
    app_handle: AppHandle,
    date: Option<String>,
) -> Result<RegenerationPreview, AppError> {
    let date = resolve_date(date.as_deref())?;
    let env = RunEnv::load(Some(&app_handle))?;
    let _guard = lock::acquire(&env.state_dir.join(LOCK_SUBDIR))
        .map_err(|error| AppError::Lock(format!("another compile is already running ({error})")))?;

    cleanup_expired(&env.state_dir);
    let current_bytes = read_or_empty(&env.file_path(date))?;
    let current_auto = auto_body(&current_bytes, &env.host)?;
    let base_hash = digest(&current_bytes);
    let token = preview_token(date, &env.host, &base_hash);
    let work = WorkDir(env.state_dir.join(WORK_SUBDIR).join(&token));
    let mut preview_env = env.clone();
    preview_env.state_dir = work.0.join("state");
    preview_env.dailies_dir = work.0.join("dailies");
    preview_env.repo_dir.clone_from(&preview_env.dailies_dir);
    std::fs::create_dir_all(&preview_env.dailies_dir)?;
    restrict_dir(&work.0)?;

    let isolated_state = AppState::new();
    let result = crate::pipeline_runner::compile_one(
        None,
        &isolated_state,
        &preview_env,
        date,
        TriggerSource::Manual,
        CompilePurpose::Preview,
    )
    .await?;
    if result.status != CompileStatus::Ok {
        return Err(AppError::Llm(format!(
            "could not generate regeneration candidate: {}",
            result.message
        )));
    }

    let candidate_bytes = std::fs::read(preview_env.file_path(date))?;
    let candidate_auto = auto_body(&candidate_bytes, &env.host)?;
    if candidate_auto.trim().is_empty() {
        return Err(AppError::Llm(
            "regeneration candidate did not contain an AUTO body".to_string(),
        ));
    }
    let audit_json = result
        .audit_path
        .as_deref()
        .and_then(|path| std::fs::read_to_string(path).ok());
    let expires_at = (Utc::now() + Duration::minutes(PREVIEW_TTL_MINUTES)).to_rfc3339();
    let pending = PendingRegeneration {
        token: token.clone(),
        date: date.format(DATE_FORMAT).to_string(),
        host: env.host.clone(),
        base_hash: base_hash.clone(),
        current_auto: current_auto.clone(),
        candidate_auto: candidate_auto.clone(),
        expires_at: expires_at.clone(),
        render_used: result.render_used,
        fellback: result.fellback,
        message: result.message.clone(),
        audit_json,
    };
    write_pending(&env.state_dir, &pending)?;

    Ok(RegenerationPreview {
        token,
        date: pending.date,
        host: pending.host,
        current_auto,
        candidate_auto,
        base_hash,
        expires_at,
        render_used: pending.render_used,
        fellback: pending.fellback,
        message: pending.message,
    })
}

/// Apply one explicit resolution to a pending candidate.
///
/// `merged_auto` is required only for `merge`. It is treated as untrusted input:
/// format control markers are forbidden and secrets are redacted before the
/// atomic AUTO-block update.
#[tauri::command]
pub async fn apply_regeneration(
    app_handle: AppHandle,
    token: String,
    resolution: RegenerationResolution,
    merged_auto: Option<String>,
) -> Result<RegenerationApplied, AppError> {
    validate_token(&token)?;
    let env = RunEnv::load(Some(&app_handle))?;
    let _guard = lock::acquire(&env.state_dir.join(LOCK_SUBDIR))
        .map_err(|error| AppError::Lock(format!("another compile is already running ({error})")))?;
    let pending = read_pending(&env.state_dir, &token)?;
    validate_pending(&pending, &env)?;

    let date = resolve_date(Some(&pending.date))?;
    let file_path = env.file_path(date);
    let current_bytes = read_or_empty(&file_path)?;

    if resolution == RegenerationResolution::KeepCurrent {
        let current_auto = auto_body(&current_bytes, &env.host)?;
        if let Err(error) = remove_pending(&env.state_dir, &token) {
            tracing::warn!(%error, "could not remove dismissed regeneration preview");
        }
        return Ok(RegenerationApplied {
            date: pending.date,
            host: pending.host,
            file_path: file_path.display().to_string(),
            resolution,
            auto_body: current_auto,
            committed: false,
            pushed: false,
            message: "kept current standup; candidate discarded".to_string(),
        });
    }
    ensure_base_unchanged(&current_bytes, &pending.base_hash)?;

    let selected = match resolution {
        RegenerationResolution::UseCandidate => pending.candidate_auto.clone(),
        RegenerationResolution::Merge => merged_auto.ok_or_else(|| {
            AppError::Invalid("merged_auto is required when resolution is merge".to_string())
        })?,
        RegenerationResolution::KeepCurrent => unreachable!("handled above"),
    };
    validate_auto_body(&selected)?;
    let selected = autostand_core::redact::redact(selected.trim());
    std::fs::create_dir_all(&env.dailies_dir)?;
    apply_auto(&file_path, &env.host, date, &selected)?;

    if let Some(audit_json) = pending.audit_json.as_deref() {
        if let Err(error) = persist_resolved_audit(
            &env.state_dir,
            &pending.date,
            &env.host,
            audit_json,
            resolution,
        ) {
            tracing::warn!(%error, "regeneration applied but audit sidecar could not be persisted");
        }
    }

    let repo_sync =
        env.config.sync.repo_enabled && crate::commands::sync::repo_sync_prerequisites().await;
    let git = if repo_sync {
        match crate::git_ops::commit_push(&env.repo_dir, std::slice::from_ref(&file_path)).await {
            Ok(outcome) => outcome,
            Err(error) => {
                tracing::warn!(%error, "regeneration saved locally but git sync was skipped");
                crate::git_ops::CommitOutcome {
                    committed: false,
                    pushed: false,
                    message: format!("saved locally; git sync skipped: {error}"),
                }
            }
        }
    } else {
        crate::git_ops::CommitOutcome {
            committed: false,
            pushed: false,
            message: "regeneration applied locally; Repo Sync is disabled or unavailable"
                .to_string(),
        }
    };
    if let Err(error) = remove_pending(&env.state_dir, &token) {
        tracing::warn!(%error, "regeneration applied but pending preview could not be removed");
    }
    Ok(RegenerationApplied {
        date: pending.date,
        host: pending.host,
        file_path: file_path.display().to_string(),
        resolution,
        auto_body: selected,
        committed: git.committed,
        pushed: git.pushed,
        message: if git.message.is_empty() {
            "regeneration applied".to_string()
        } else {
            git.message
        },
    })
}

fn resolve_date(value: Option<&str>) -> Result<NaiveDate, AppError> {
    match value {
        Some(value) => NaiveDate::parse_from_str(value, DATE_FORMAT)
            .map_err(|error| AppError::Invalid(format!("invalid date '{value}': {error}"))),
        None => Ok(
            crate::pipeline_runner::targets(Local::now().date_naive(), None)
                .into_iter()
                .next()
                .unwrap_or_else(|| Local::now().date_naive()),
        ),
    }
}

fn read_or_empty(path: &Path) -> Result<Vec<u8>, AppError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(AppError::Io(format!("read {}: {error}", path.display()))),
    }
}

fn auto_body(bytes: &[u8], host: &str) -> Result<String, AppError> {
    if bytes.is_empty() || String::from_utf8_lossy(bytes).trim().is_empty() {
        return Ok(String::new());
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|error| AppError::Invalid(format!("standup is not UTF-8: {error}")))?;
    let parsed = autostand_core::format::parse(text)
        .map_err(|error| AppError::Invalid(format!("invalid standup format: {error}")))?;
    Ok(parsed
        .auto_blocks
        .into_iter()
        .find(|block| block.host == host)
        .map_or_else(String::new, |block| block.body))
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn ensure_base_unchanged(bytes: &[u8], expected_hash: &str) -> Result<(), AppError> {
    if digest(bytes) == expected_hash {
        Ok(())
    } else {
        Err(AppError::Invalid(
            "standup changed after the preview; generate a new comparison before applying"
                .to_string(),
        ))
    }
}

fn preview_token(date: NaiveDate, host: &str, base_hash: &str) -> String {
    let entropy = format!(
        "{date}:{host}:{base_hash}:{}:{}",
        std::process::id(),
        Utc::now().timestamp_nanos_opt().unwrap_or_default()
    );
    digest(entropy.as_bytes())
}

fn validate_token(token: &str) -> Result<(), AppError> {
    if token.len() == 64 && token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(AppError::Invalid("invalid regeneration token".to_string()))
    }
}

fn pending_path(state_dir: &Path, token: &str) -> PathBuf {
    state_dir.join(PENDING_SUBDIR).join(format!("{token}.json"))
}

fn write_pending(state_dir: &Path, pending: &PendingRegeneration) -> Result<(), AppError> {
    let path = pending_path(state_dir, &pending.token);
    let bytes = serde_json::to_vec_pretty(pending)?;
    write_atomic_bytes(&path, &bytes)
}

fn read_pending(state_dir: &Path, token: &str) -> Result<PendingRegeneration, AppError> {
    let path = pending_path(state_dir, token);
    let bytes = std::fs::read(&path)
        .map_err(|error| AppError::NotFound(format!("regeneration preview: {error}")))?;
    let pending: PendingRegeneration = serde_json::from_slice(&bytes)?;
    if pending.token != token {
        return Err(AppError::Invalid(
            "regeneration token does not match pending state".to_string(),
        ));
    }
    Ok(pending)
}

fn remove_pending(state_dir: &Path, token: &str) -> Result<(), AppError> {
    match std::fs::remove_file(pending_path(state_dir, token)) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn validate_pending(pending: &PendingRegeneration, env: &RunEnv) -> Result<(), AppError> {
    if pending.host != env.host {
        return Err(AppError::Invalid(
            "regeneration preview belongs to a different host".to_string(),
        ));
    }
    let expires = DateTime::parse_from_rfc3339(&pending.expires_at)
        .map_err(|error| AppError::Invalid(format!("invalid preview expiry: {error}")))?
        .with_timezone(&Utc);
    if expires <= Utc::now() {
        return Err(AppError::Invalid(
            "regeneration preview expired; generate a new comparison".to_string(),
        ));
    }
    Ok(())
}

fn validate_auto_body(body: &str) -> Result<(), AppError> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(AppError::Invalid("AUTO body cannot be empty".to_string()));
    }
    if trimmed.chars().count() > MAX_MERGED_CHARS {
        return Err(AppError::Invalid(format!(
            "AUTO body exceeds {MAX_MERGED_CHARS} characters"
        )));
    }
    if !trimmed
        .lines()
        .any(|line| line.trim_start().starts_with("- "))
    {
        return Err(AppError::Invalid(
            "AUTO body must contain at least one bullet".to_string(),
        ));
    }
    for marker in ["<!-- AUTO:", "<!-- MANUAL:START -->", "<!-- MANUAL:END -->"] {
        if trimmed.contains(marker) {
            return Err(AppError::Invalid(format!(
                "AUTO body cannot contain format marker {marker}"
            )));
        }
    }
    Ok(())
}

fn apply_auto(path: &Path, host: &str, date: NaiveDate, body: &str) -> Result<(), AppError> {
    let window = autostand_core::dates::compute_window(date);
    autostand_core::fileops::set_auto(
        path,
        host,
        &crate::pipeline_runner::title_for(date),
        &crate::pipeline_runner::subtitle_for(&window),
        body,
    )
    .map_err(|error| AppError::Io(format!("write {}: {error}", path.display())))
}

fn cleanup_expired(state_dir: &Path) {
    let dir = state_dir.join(PENDING_SUBDIR);
    let Ok(entries) = std::fs::read_dir(&dir) else {
        return;
    };
    for entry in entries.flatten().take(256) {
        let path = entry.path();
        let expiry = std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<PendingRegeneration>(&bytes).ok())
            .and_then(|pending| DateTime::parse_from_rfc3339(&pending.expires_at).ok());
        let should_remove = expiry.map_or(true, |value| value.with_timezone(&Utc) <= Utc::now());
        if should_remove {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn persist_resolved_audit(
    state_dir: &Path,
    date: &str,
    host: &str,
    audit_json: &str,
    resolution: RegenerationResolution,
) -> Result<(), AppError> {
    let mut value: serde_json::Value = serde_json::from_str(audit_json)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "regeneration_resolution".to_string(),
            serde_json::to_value(resolution)?,
        );
        object.insert(
            "regenerated_at".to_string(),
            serde_json::Value::String(Utc::now().to_rfc3339()),
        );
    }
    let path = state_dir.join("audit").join(format!("{date}-{host}.json"));
    write_atomic_bytes(&path, &serde_json::to_vec_pretty(&value)?)
}

fn write_atomic_bytes(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let tmp = path.with_extension("tmp");
    {
        let mut file = std::fs::File::create(&tmp)?;
        restrict_file(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp, path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

#[cfg(unix)]
fn restrict_file(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(unix)]
fn restrict_dir(path: &Path) -> Result<(), AppError> {
    use std::os::unix::fs::PermissionsExt as _;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))?;
    Ok(())
}

#[cfg(not(unix))]
fn restrict_file(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(not(unix))]
fn restrict_dir(_path: &Path) -> Result<(), AppError> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        apply_auto, auto_body, digest, ensure_base_unchanged, validate_auto_body, validate_token,
    };
    use chrono::NaiveDate;

    #[test]
    fn extracts_only_the_requested_host_auto_block() {
        let file = b"# Daily Standup \xe2\x80\x94 Day\n_._\n\n<!-- AUTO:a:START -->\n- A\n<!-- AUTO:a:END -->\n\n<!-- AUTO:b:START -->\n- B\n<!-- AUTO:b:END -->\n\n<!-- MANUAL:START -->\n- Manual\n<!-- MANUAL:END -->\n";
        assert_eq!(auto_body(file, "b").unwrap(), "- B");
        assert_eq!(auto_body(file, "missing").unwrap(), "");
    }

    #[test]
    fn base_hash_covers_the_exact_file_bytes() {
        assert_ne!(digest(b"a"), digest(b"a\n"));
        assert_eq!(digest(b"").len(), 64);
        assert!(ensure_base_unchanged(b"before", &digest(b"before")).is_ok());
        let error = ensure_base_unchanged(b"after", &digest(b"before"))
            .expect_err("concurrent edits invalidate the preview");
        assert!(error.to_string().contains("changed after the preview"));
    }

    #[test]
    fn token_validation_blocks_path_traversal() {
        assert!(validate_token(&"a".repeat(64)).is_ok());
        for token in ["../candidate", "abc", &"g".repeat(64)] {
            assert!(validate_token(token).is_err());
        }
    }

    #[test]
    fn merged_body_requires_bullets_and_forbids_control_markers() {
        assert!(validate_auto_body("**Work**\n- Kept this bullet").is_ok());
        assert!(validate_auto_body("plain prose").is_err());
        assert!(validate_auto_body("- ok\n<!-- MANUAL:START -->").is_err());
        assert!(validate_auto_body("- ok\n<!-- AUTO:other:START -->").is_err());
    }

    #[test]
    fn applying_a_resolution_preserves_manual_and_other_hosts() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("2026-08-14.md");
        let original = "# Daily Standup — Old\n_._\n\n\
            <!-- AUTO:mine:START -->\n- old mine\n<!-- AUTO:mine:END -->\n\n\
            <!-- AUTO:other:START -->\n- other body\n<!-- AUTO:other:END -->\n\n\
            <!-- MANUAL:START -->\n- human note\n<!-- MANUAL:END -->\n";
        std::fs::write(&path, original).unwrap();
        let date = NaiveDate::from_ymd_opt(2026, 8, 14).unwrap();

        apply_auto(&path, "mine", date, "- new mine").unwrap();

        let result = std::fs::read_to_string(path).unwrap();
        assert!(result.contains("- new mine"));
        assert!(!result.contains("- old mine"));
        assert!(result.contains("- other body"));
        assert!(result.contains("- human note"));
    }
}
