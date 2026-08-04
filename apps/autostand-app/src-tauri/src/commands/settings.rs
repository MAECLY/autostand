//! Settings-path IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `get_settings_paths`, `validate_paths`.

use std::path::PathBuf;

use tauri::AppHandle;

use crate::commands::{
    load_config, repos::resolve_github_dir, standup::resolve_dailies_dir, state_dir,
    types::PathValidation, types::SettingsPaths,
};
use crate::error::AppError;

/// Resolve `~/.local/share/opencode` (Linux/BSD) or `~/.config/opencode` (macOS).
fn opencode_dir() -> PathBuf {
    if cfg!(target_os = "macos") {
        dirs::home_dir()
            .map(|h| h.join(".config").join("opencode"))
            .unwrap_or_default()
    } else {
        dirs::data_local_dir()
            .map(|d| d.join("opencode"))
            .unwrap_or_default()
    }
}

/// Render a path as the `String` the contract expects.
fn as_string(path: &std::path::Path) -> String {
    path.to_string_lossy().to_string()
}

/// Resolve a dot-directory directly under the user's home (e.g. `~/.claude`).
fn home_dot_dir(name: &str) -> String {
    as_string(&dirs::home_dir().map(|h| h.join(name)).unwrap_or_default())
}

/// Resolve all configured paths.
pub(crate) fn resolve_paths(app: &impl tauri::Manager<tauri::Wry>) -> SettingsPaths {
    let config = load_config(app).ok();
    let home = dirs::home_dir();
    let state = state_dir();
    SettingsPaths {
        github_dir: as_string(&resolve_github_dir(
            config.as_ref().map(|c| c.github_dir.as_str()),
            home.as_deref(),
        )),
        dailies_dir: as_string(&resolve_dailies_dir(
            config.as_ref().map(|c| c.dailies_dir.as_str()),
            home.as_deref(),
        )),
        claude_dir: home_dot_dir(".claude"),
        codex_dir: home_dot_dir(".codex"),
        gemini_dir: home_dot_dir(".gemini"),
        opencode_dir: as_string(&opencode_dir()),
        audit_dir: as_string(&state.join("audit")),
        state_dir: as_string(&state),
        config_dir: as_string(
            &dirs::config_dir()
                .map(|d| d.join("autostand"))
                .unwrap_or_default(),
        ),
    }
}

/// Return all configured paths.
#[tauri::command]
pub async fn get_settings_paths(app_handle: AppHandle) -> Result<SettingsPaths, AppError> {
    Ok(resolve_paths(&app_handle))
}

/// Check each path exists + is readable.
#[tauri::command]
pub async fn validate_paths(app_handle: AppHandle) -> Result<Vec<PathValidation>, AppError> {
    let paths = resolve_paths(&app_handle);
    let items = [
        ("github_dir", &paths.github_dir),
        ("dailies_dir", &paths.dailies_dir),
        ("claude_dir", &paths.claude_dir),
        ("codex_dir", &paths.codex_dir),
        ("gemini_dir", &paths.gemini_dir),
        ("opencode_dir", &paths.opencode_dir),
        ("state_dir", &paths.state_dir),
        ("config_dir", &paths.config_dir),
        ("audit_dir", &paths.audit_dir),
    ];
    let mut out = Vec::with_capacity(items.len());
    for (label, path) in items {
        let exists = std::path::Path::new(path).exists();
        let readable = exists
            && std::fs::metadata(path).is_ok_and(|m| !m.permissions().readonly() || m.is_dir());
        let message = if exists {
            None
        } else {
            Some(format!("path does not exist: {path}"))
        };
        out.push(PathValidation {
            path: path.clone(),
            label: label.to_string(),
            exists,
            readable,
            message,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{as_string, home_dot_dir};
    use crate::commands::types::SettingsPaths;
    use std::path::Path;

    /// The labels `validate_paths` emits must be exactly the `SettingsPaths`
    /// field names — the UI keys its per-path rows off them.
    #[test]
    fn validate_paths_labels_match_the_settings_paths_fields() {
        let value = serde_json::to_value(SettingsPaths::default()).unwrap();
        let obj = value.as_object().expect("SettingsPaths is an object");
        let mut fields: Vec<&str> = obj.keys().map(String::as_str).collect();
        fields.sort_unstable();
        let mut labels = vec![
            "github_dir",
            "dailies_dir",
            "claude_dir",
            "codex_dir",
            "gemini_dir",
            "opencode_dir",
            "state_dir",
            "config_dir",
            "audit_dir",
        ];
        labels.sort_unstable();
        assert_eq!(fields, labels);
    }

    #[test]
    fn as_string_renders_a_path_verbatim() {
        assert_eq!(
            as_string(Path::new("/home/tester/.claude")),
            "/home/tester/.claude"
        );
    }

    #[test]
    fn home_dot_dir_hangs_the_name_off_the_home_directory() {
        let claude = home_dot_dir(".claude");
        assert!(
            claude.ends_with(".claude"),
            "expected a ~/.claude path, got {claude}"
        );
    }
}
