//! Data source IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` rows `list_data_sources`, `toggle_data_source`.

use tauri::AppHandle;

use crate::commands::{load_config, save_config, types::DataSourceConfig};
use crate::error::AppError;

/// Static descriptor for each of the 8 data sources.
///
/// Deliberately carries no default-enabled flag: the defaults live in
/// `DataSourceConfigs::default()`, and duplicating them here would let the
/// catalogue and the persisted config drift apart.
struct SourceDef {
    id: &'static str,
    label: &'static str,
    description: &'static str,
}

/// The 8 sources per `docs/data-sources/00-sources-overview.md`.
const SOURCES: &[SourceDef] = &[
    SourceDef {
        id: "local-git",
        label: "Local Git",
        description: "Commits, ticket keys, branch, file scope, anti-backdating map from local .git under GITHUB_DIR.",
    },
    SourceDef {
        id: "github",
        label: "GitHub (gh CLI)",
        description: "PRs opened/merged, review/issue comments, review states, recent-PR tickets via gh CLI.",
    },
    SourceDef {
        id: "claude-code",
        label: "Claude Code sessions",
        description: "Conversation digest + edited file attribution from ~/.claude/projects/*/*.jsonl and ~/.claude/plans/*.md.",
    },
    SourceDef {
        id: "remember-plugin",
        label: "Remember plugin",
        description: "Narrative non-commit work notes from <repo>/.remember/*.md and $GITHUB_DIR/.remember/now.md.",
    },
    SourceDef {
        id: "opencode",
        label: "OpenCode",
        description: "Session history + edited file attribution from ~/.local/share/opencode/opencode.db (SQLite) and legacy JSON.",
    },
    SourceDef {
        id: "codex",
        label: "Codex CLI",
        description: "Session history + edited file attribution from ~/.codex/sessions/YYYY/MM/DD/*.jsonl and ~/.codex/history.jsonl.",
    },
    SourceDef {
        id: "gemini-cli",
        label: "Gemini CLI",
        description: "Session history + edited file attribution from ~/.gemini/**/*.jsonl.",
    },
    SourceDef {
        id: "grok-cli",
        label: "Grok CLI",
        description: "Session history + edited file attribution from ~/.grok/ or ~/.config/grok-cli/ (variant-dependent).",
    },
];

/// Look up the enabled flag from `AppConfig.data_sources` for a source id.
fn is_enabled(cfg: &crate::commands::types::DataSourceConfigs, id: &str) -> bool {
    match id {
        "local-git" => cfg.local_git,
        "github" => cfg.github,
        "claude-code" => cfg.claude_code,
        "remember-plugin" => cfg.remember,
        "opencode" => cfg.opencode,
        "codex" => cfg.codex,
        "gemini-cli" => cfg.gemini_cli,
        "grok-cli" => cfg.grok_cli,
        _ => false,
    }
}

/// Set the enabled flag for a source id in `AppConfig.data_sources`.
fn set_enabled(
    cfg: &mut crate::commands::types::DataSourceConfigs,
    id: &str,
    enabled: bool,
) -> bool {
    match id {
        "local-git" => {
            cfg.local_git = enabled;
            true
        }
        "github" => {
            cfg.github = enabled;
            true
        }
        "claude-code" => {
            cfg.claude_code = enabled;
            true
        }
        "remember-plugin" => {
            cfg.remember = enabled;
            true
        }
        "opencode" => {
            cfg.opencode = enabled;
            true
        }
        "codex" => {
            cfg.codex = enabled;
            true
        }
        "gemini-cli" => {
            cfg.gemini_cli = enabled;
            true
        }
        "grok-cli" => {
            cfg.grok_cli = enabled;
            true
        }
        _ => false,
    }
}

/// List the 8 data sources with their current enablement state.
#[tauri::command]
pub async fn list_data_sources(app_handle: AppHandle) -> Result<Vec<DataSourceConfig>, AppError> {
    let config = load_config(&app_handle)?;
    let out = SOURCES
        .iter()
        .map(|s| DataSourceConfig {
            id: s.id.to_string(),
            label: s.label.to_string(),
            enabled: is_enabled(&config.data_sources, s.id),
            description: s.description.to_string(),
        })
        .collect();
    Ok(out)
}

/// Flip a source flag and persist the config.
#[tauri::command]
pub async fn toggle_data_source(
    app_handle: AppHandle,
    id: String,
    enabled: bool,
) -> Result<(), AppError> {
    let mut config = load_config(&app_handle)?;
    if !set_enabled(&mut config.data_sources, &id, enabled) {
        return Err(AppError::Invalid(format!("unknown data source id: {id}")));
    }
    save_config(&app_handle, &config)
}

#[cfg(test)]
mod tests {
    use super::{is_enabled, set_enabled, SOURCES};
    use crate::commands::types::DataSourceConfigs;

    #[test]
    fn exposes_the_eight_documented_sources() {
        let ids: Vec<&str> = SOURCES.iter().map(|s| s.id).collect();
        assert_eq!(
            ids,
            [
                "local-git",
                "github",
                "claude-code",
                "remember-plugin",
                "opencode",
                "codex",
                "gemini-cli",
                "grok-cli",
            ]
        );
    }

    /// Every catalogue id must be routable; a typo here would silently render
    /// a source as permanently disabled and make its toggle a no-op error.
    #[test]
    fn every_catalogue_id_is_wired_to_a_config_flag() {
        let mut cfg = DataSourceConfigs::default();
        for source in SOURCES {
            assert!(
                set_enabled(&mut cfg, source.id, true),
                "{} has no config flag",
                source.id
            );
            assert!(
                is_enabled(&cfg, source.id),
                "{} did not read back",
                source.id
            );
            assert!(set_enabled(&mut cfg, source.id, false));
            assert!(!is_enabled(&cfg, source.id));
        }
    }

    #[test]
    fn unknown_ids_are_rejected_rather_than_ignored() {
        let mut cfg = DataSourceConfigs::default();
        assert!(!set_enabled(&mut cfg, "remember", false));
        assert!(!set_enabled(&mut cfg, "local_git", false));
        assert!(!is_enabled(&cfg, "nope"));
        // A rejected toggle must not have mutated anything.
        assert!(cfg.local_git && cfg.remember);
    }

    #[test]
    fn defaults_match_the_documented_opt_in_set() {
        let cfg = DataSourceConfigs::default();
        for on in [
            "local-git",
            "github",
            "claude-code",
            "remember-plugin",
            "opencode",
        ] {
            assert!(is_enabled(&cfg, on), "{on} defaults on");
        }
        for off in ["codex", "gemini-cli", "grok-cli"] {
            assert!(!is_enabled(&cfg, off), "{off} defaults off");
        }
    }
}
