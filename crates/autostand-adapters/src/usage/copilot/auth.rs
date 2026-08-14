//! Read-only discovery of a GitHub token this machine already holds.
//!
//! No login flow and no browser cookies — only tokens some GitHub tool left
//! behind, in a fixed order that puts prompt-free files first and the keychain
//! last:
//!
//! 1. The Copilot editor config (`apps.json`, older `hosts.json`) — the OAuth
//!    token the VS Code / `JetBrains` / Neovim plugins write. File-based and
//!    portable.
//! 2. The GitHub CLI config (`hosts.yml`) — present when `gh` stores its token
//!    in a file.
//! 3. The GitHub CLI keychain item (service `gh:github.com`), `go-keyring`
//!    wrapped — used when `gh` stores the token in the system keyring instead.
//!    macOS only, and only on a manual refresh.
//!
//! Only `github.com` entries are ever used. A GitHub Enterprise token must not
//! be sent to `api.github.com`, so an Enterprise-only config falls through to
//! the next source rather than yielding a token that is guaranteed to 401.
//!
//! Copilot has no refresh path here by design: a rejected token moves to the
//! next source, and when none is left the provider reports `session_expired`.

use std::path::PathBuf;

use crate::usage::creds::keychain::{self, KeychainAccess};
use crate::usage::creds::{files, CredentialSource, Secret};
use crate::usage::model::UsageError;
use crate::usage::parse;

/// Keychain service the GitHub CLI stores its token under.
const GH_KEYCHAIN_SERVICE: &str = "gh:github.com";

/// The only host whose token may be sent to `api.github.com`.
const GITHUB_HOST: &str = "github.com";

/// One GitHub token, with where it came from.
///
/// The value is a [`Secret`], so `Debug` cannot print it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CopilotToken {
    pub value: Secret,
    pub source: CredentialSource,
}

/// Candidate paths for the Copilot editor config, newest shape first.
#[must_use]
pub fn editor_config_paths() -> Vec<PathBuf> {
    config_dirs("github-copilot")
        .into_iter()
        .flat_map(|dir| [dir.join("apps.json"), dir.join("hosts.json")])
        .collect()
}

/// Candidate paths for the GitHub CLI `hosts.yml`.
#[must_use]
pub fn gh_config_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = files::env_text("GH_CONFIG_DIR") {
        candidates.push(PathBuf::from(dir).join("hosts.yml"));
    }
    candidates.extend(
        config_dirs("gh")
            .into_iter()
            .map(|dir| dir.join("hosts.yml")),
    );
    if cfg!(target_os = "windows") {
        if let Some(appdata) = files::env_text("APPDATA") {
            // The Windows GitHub CLI uses a display-cased directory name.
            candidates.push(PathBuf::from(appdata).join("GitHub CLI/hosts.yml"));
        }
    }
    candidates
}

/// Config directories for `app`, in the order they should be tried.
fn config_dirs(app: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(config_home) = files::env_text("XDG_CONFIG_HOME") {
        dirs.push(PathBuf::from(config_home).join(app));
    } else if let Some(path) = files::home_relative(&format!(".config/{app}")) {
        dirs.push(path);
    }
    if cfg!(target_os = "windows") {
        if let Some(appdata) = files::env_text("APPDATA") {
            dirs.push(PathBuf::from(appdata).join(app));
        }
    }
    dirs
}

/// Whether a token source exists at all, without reading a secret.
///
/// Files are parsed (cheap, prompt-free); the keychain is only asked whether an
/// item *exists*, which cannot raise an unlock dialog.
pub async fn has_local_credentials() -> bool {
    let (tokens, username) = scan_files().await;
    if !tokens.unwrap_or_default().is_empty() {
        return true;
    }
    keychain::generic_password_exists(GH_KEYCHAIN_SERVICE, username.as_deref())
        .await
        .unwrap_or(false)
}

/// Every token this machine offers, in source order, de-duplicated.
///
/// The keychain entry is included only when `access` allows a secret read; on a
/// background pass it is skipped rather than prompting the user.
pub async fn load_tokens(access: KeychainAccess) -> Result<Vec<CopilotToken>, UsageError> {
    let (tokens, username) = scan_files().await;
    let mut tokens = tokens?;
    if let Some(secret) = read_gh_keychain(username.as_deref(), access).await {
        push_unique(&mut tokens, secret);
    }
    Ok(tokens)
}

/// The file half of discovery, off the runtime thread: the tokens, and the
/// username the keychain item is scoped to. Both come from the same pass so the
/// configs are read once.
async fn scan_files() -> (Result<Vec<CopilotToken>, UsageError>, Option<String>) {
    tokio::task::spawn_blocking(|| (file_tokens(), gh_username()))
        .await
        // A panicked blocking read is a broken store, not a logout.
        .unwrap_or((Err(UsageError::CredentialStoreUnavailable), None))
}

/// The file-backed half of [`load_tokens`]: editor config, then `gh` config.
fn file_tokens() -> Result<Vec<CopilotToken>, UsageError> {
    let mut tokens = Vec::new();
    for path in editor_config_paths() {
        let Some(text) = files::read_text_if_present(&path)? else {
            continue;
        };
        if let Some(token) = editor_oauth_token(&text).as_deref().and_then(Secret::new) {
            push_unique(
                &mut tokens,
                CopilotToken {
                    value: token,
                    source: CredentialSource::File,
                },
            );
        }
    }
    for path in gh_config_paths() {
        let Some(text) = files::read_text_if_present(&path)? else {
            continue;
        };
        if let Some(token) = yaml_value(&text, "oauth_token", GITHUB_HOST)
            .as_deref()
            .and_then(Secret::new)
        {
            push_unique(
                &mut tokens,
                CopilotToken {
                    value: token,
                    source: CredentialSource::File,
                },
            );
        }
    }
    Ok(tokens)
}

async fn read_gh_keychain(account: Option<&str>, access: KeychainAccess) -> Option<CopilotToken> {
    let (outcome, source) = match account {
        Some(account) => (
            keychain::read_generic_password(GH_KEYCHAIN_SERVICE, Some(account), access).await,
            CredentialSource::KeychainCurrentUser,
        ),
        None => (
            keychain::read_generic_password(GH_KEYCHAIN_SERVICE, None, access).await,
            CredentialSource::KeychainLegacy,
        ),
    };
    let raw = match outcome {
        keychain::KeychainOutcome::Found(secret) => secret,
        // `NotFound` with an account may still have a service-only item.
        keychain::KeychainOutcome::NotFound if account.is_some() => {
            keychain::read_generic_password(GH_KEYCHAIN_SERVICE, None, access)
                .await
                .found()?
        }
        _ => return None,
    };
    Some(CopilotToken {
        value: parse::unwrap_go_keyring(&raw)
            .as_deref()
            .and_then(Secret::new)?,
        source,
    })
}

/// The GitHub username `gh` scopes its keychain item to, read from `hosts.yml`.
fn gh_username() -> Option<String> {
    for path in gh_config_paths() {
        let Ok(Some(text)) = files::read_text_if_present(&path) else {
            continue;
        };
        if let Some(user) = yaml_value(&text, "user", GITHUB_HOST) {
            return Some(user);
        }
    }
    None
}

fn push_unique(tokens: &mut Vec<CopilotToken>, token: CopilotToken) {
    if !tokens.iter().any(|existing| existing.value == token.value) {
        tokens.push(token);
    }
}

/// Pull a `github.com` `oauth_token` out of the Copilot editor config.
///
/// The file is a JSON object keyed by host — `"github.com"` in the older
/// `hosts.json`, `"github.com:<appId>"` in `apps.json` — each value an object
/// carrying `oauth_token`. Any other host is skipped: sending an Enterprise
/// token to `api.github.com` is a guaranteed rejection, and falling through
/// leaves a real `github.com` token in a later source free to win.
#[must_use]
pub fn editor_oauth_token(text: &str) -> Option<String> {
    let object = serde_json::from_str::<serde_json::Value>(text).ok()?;
    for (key, value) in object.as_object()? {
        if key != GITHUB_HOST && !key.starts_with(&format!("{GITHUB_HOST}:")) {
            continue;
        }
        if let Some(token) = value.get("oauth_token").and_then(parse::text) {
            return Some(token.to_string());
        }
    }
    None
}

/// Read an indented `key: value` from inside one host block of `hosts.yml`.
///
/// `gh` keys each host block by an unindented `<host>:` line. The read is scoped
/// to that block because an Enterprise block in the same file would otherwise
/// let its `oauth_token` win and be sent to `api.github.com`. The nested
/// `users:` map does not match `user:` — the colon lands in a different place.
#[must_use]
pub fn yaml_value(text: &str, key: &str, host: &str) -> Option<String> {
    let prefix = format!("{key}:");
    let host_header = format!("{host}:");
    let mut in_host = false;
    for line in text.lines() {
        let first = line.chars().next()?;
        if !first.is_whitespace() {
            in_host = line.trim().starts_with(&host_header);
            continue;
        }
        if !in_host {
            continue;
        }
        let trimmed = line.trim();
        let Some(value) = trimmed.strip_prefix(&prefix) else {
            continue;
        };
        let unquoted = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if !unquoted.is_empty() {
            return Some(unquoted.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{
        editor_config_paths, editor_oauth_token, gh_config_paths, yaml_value, CopilotToken,
    };
    use crate::usage::creds::{CredentialSource, Secret};

    #[test]
    fn reads_the_editor_apps_json_token() {
        let text =
            r#"{ "github.com:Iv1.abc123": { "user": "octocat", "oauth_token": "gho_editor" } }"#;
        assert_eq!(editor_oauth_token(text).as_deref(), Some("gho_editor"));
    }

    #[test]
    fn an_enterprise_only_editor_config_yields_nothing() {
        // Sending an Enterprise token to api.github.com is a guaranteed
        // rejection; falling through lets a real github.com token win.
        let text = r#"{ "ghe.corp.example:Iv1.x": { "oauth_token": "gho_enterprise" } }"#;
        assert_eq!(editor_oauth_token(text), None);
    }

    #[test]
    fn the_github_dot_com_entry_wins_among_hosts() {
        let text = r#"{ "ghe.corp.example:Iv1.x": { "oauth_token": "gho_ent" }, "github.com:Iv1.y": { "oauth_token": "gho_dotcom" } }"#;
        assert_eq!(editor_oauth_token(text).as_deref(), Some("gho_dotcom"));
    }

    #[test]
    fn a_garbled_editor_config_is_no_token_rather_than_a_failure() {
        assert_eq!(editor_oauth_token("not json"), None);
        assert_eq!(
            editor_oauth_token(r#"{ "github.com": { "oauth_token": "" } }"#),
            None
        );
    }

    #[test]
    fn yaml_reads_are_scoped_to_the_github_dot_com_block() {
        let hosts = "ghe.corp.example:\n    oauth_token: gho_enterprise\n    user: ent\ngithub.com:\n    oauth_token: gho_dotcom\n    user: octocat\n";
        assert_eq!(
            yaml_value(hosts, "oauth_token", "github.com").as_deref(),
            Some("gho_dotcom")
        );
        assert_eq!(
            yaml_value(hosts, "user", "github.com").as_deref(),
            Some("octocat")
        );
    }

    #[test]
    fn the_nested_users_map_is_not_mistaken_for_the_user_key() {
        let hosts = "github.com:\n    users:\n        octocat:\n    user: octocat\n";
        assert_eq!(
            yaml_value(hosts, "user", "github.com").as_deref(),
            Some("octocat")
        );
    }

    #[test]
    fn quoted_yaml_values_are_unwrapped() {
        let hosts = "github.com:\n    oauth_token: \"gho_quoted\"\n";
        assert_eq!(
            yaml_value(hosts, "oauth_token", "github.com").as_deref(),
            Some("gho_quoted")
        );
    }

    #[test]
    fn candidate_paths_are_named_after_the_tools_that_write_them() {
        for path in editor_config_paths() {
            assert!(
                path.to_string_lossy().contains("github-copilot"),
                "{path:?}"
            );
        }
        for path in gh_config_paths() {
            assert!(
                path.to_string_lossy().to_lowercase().contains("gh"),
                "{path:?}"
            );
        }
    }

    #[test]
    fn debug_never_prints_the_token() {
        let token = CopilotToken {
            value: Secret::new("gho_supersecret").unwrap(),
            source: CredentialSource::File,
        };
        let shown = format!("{token:?}");
        assert!(!shown.contains("gho_supersecret"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }
}
