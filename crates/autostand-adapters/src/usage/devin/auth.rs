//! Read-only discovery of the Devin session already on this machine.
//!
//! Two sources, tried in this order and both prompt-free:
//!
//! 1. `~/.local/share/devin/credentials.toml` — what `devin auth login` writes.
//!    Carries `windsurf_api_key` and an optional `api_server_url`.
//! 2. The Devin editor's `state.vscdb`, key `windsurfAuthStatus`, whose value is
//!    a JSON object with an `apiKey`.
//!
//! Neither is ever written back. Devin's key does not expire on a clock we can
//! read, so an invalid one is discovered by the endpoint rejecting it and is
//! reported as `session_expired` — never refreshed.

use std::path::PathBuf;

use crate::usage::creds::{files, vscdb, CredentialSource, Secret};
use crate::usage::model::UsageError;

/// The editor directory the Devin app keeps its global storage under.
const STATE_DB_APP: &str = "Devin";

/// `ItemTable` key holding the app's signed-in status blob.
const APP_AUTH_KEY: &str = "windsurfAuthStatus";

/// Where the CLI writes its credentials, relative to the user's home.
const CREDENTIALS_RELATIVE: &str = ".local/share/devin/credentials.toml";

/// The XDG variable that relocates the CLI credentials directory.
const DATA_HOME_ENV: &str = "XDG_DATA_HOME";

/// Devin's own API host, used whenever the credentials file names none.
pub const DEFAULT_API_SERVER_URL: &str = "https://server.codeium.com";

/// One usable Devin login.
///
/// The key is a [`Secret`], so `Debug` cannot print it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DevinAuth {
    /// The API key, held only for the duration of one probe.
    pub api_key: Secret,
    /// Host to call, already validated as `https`.
    pub api_server_url: String,
    pub source: CredentialSource,
}

/// Candidate paths for the CLI credentials file.
#[must_use]
pub fn credentials_paths() -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(data_home) = files::env_text(DATA_HOME_ENV) {
        candidates.push(PathBuf::from(data_home).join("devin/credentials.toml"));
    }
    if let Some(path) = files::home_relative(CREDENTIALS_RELATIVE) {
        candidates.push(path);
    }
    candidates
}

/// Candidate paths for the Devin app's `state.vscdb`.
#[must_use]
pub fn state_db_paths() -> Vec<PathBuf> {
    vscdb::state_db_paths(STATE_DB_APP)
}

/// Whether a Devin credential exists at all, without opening one.
///
/// Metadata only: no file is parsed, no database is opened, nothing is read that
/// could raise a prompt.
#[must_use]
pub fn any_credential_file_exists() -> bool {
    files::any_exists(&credentials_paths()) || files::any_exists(&state_db_paths())
}

/// Every login this machine offers, best source first, de-duplicated.
///
/// Blocking (a file read and a `SQLite` open); call from
/// [`tokio::task::spawn_blocking`].
pub fn load_all() -> Result<Vec<DevinAuth>, UsageError> {
    let mut found = Vec::new();
    if let Some(auth) = load_credentials_file()? {
        found.push(auth);
    }
    if let Some(auth) = load_app_auth()? {
        // The app and the CLI usually hold the same key; a second identical
        // request would only spend the user's rate limit.
        if !found.iter().any(|existing| {
            existing.api_key == auth.api_key && existing.api_server_url == auth.api_server_url
        }) {
            found.push(auth);
        }
    }
    Ok(found)
}

fn load_credentials_file() -> Result<Option<DevinAuth>, UsageError> {
    let Some(path) = files::first_existing(&credentials_paths()) else {
        return Ok(None);
    };
    let Some(text) = files::read_text_if_present(&path)? else {
        return Ok(None);
    };
    let Some(api_key) = toml_string(&text, "windsurf_api_key")
        .as_deref()
        .and_then(Secret::new)
    else {
        return Ok(None);
    };
    Ok(Some(DevinAuth {
        api_key,
        api_server_url: toml_string(&text, "api_server_url")
            .as_deref()
            .and_then(clean_api_server_url)
            .unwrap_or_else(|| DEFAULT_API_SERVER_URL.to_string()),
        source: CredentialSource::File,
    }))
}

fn load_app_auth() -> Result<Option<DevinAuth>, UsageError> {
    let Some(raw) = vscdb::read_item_from_any(&state_db_paths(), APP_AUTH_KEY)? else {
        return Ok(None);
    };
    let Ok(status) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Ok(None);
    };
    let Some(api_key) = status
        .get("apiKey")
        .and_then(crate::usage::parse::text)
        .and_then(Secret::new)
    else {
        return Ok(None);
    };
    Ok(Some(DevinAuth {
        api_key,
        // The app blob names no host: it always talks to Devin's own.
        api_server_url: DEFAULT_API_SERVER_URL.to_string(),
        source: CredentialSource::File,
    }))
}

/// Read a top-level `key = value` string out of the credentials TOML.
///
/// Hand-rolled rather than pulling a TOML parser into this crate: the file is
/// two flat keys, and a parse failure here must degrade to "no credential"
/// rather than fail the probe.
#[must_use]
pub fn toml_string(text: &str, key: &str) -> Option<String> {
    for line in text.lines() {
        // A line without `=` is a comment or a table header; skip it rather than
        // abandoning the search, or a leading comment would hide every key.
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        if name.trim() != key {
            continue;
        }
        return toml_value(value.trim());
    }
    None
}

fn toml_value(raw: &str) -> Option<String> {
    let mut characters = raw.chars();
    let value = match characters.next()? {
        quote @ ('"' | '\'') => {
            let mut out = String::new();
            let mut escaped = false;
            for character in characters {
                if character == quote && !escaped {
                    break;
                }
                escaped = character == '\\' && !escaped;
                out.push(character);
            }
            out
        }
        // Unquoted: everything up to a trailing comment.
        _ => raw.split('#').next().unwrap_or(raw).to_string(),
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Accept a configured API host only when it is `https`, with trailing slashes
/// removed.
///
/// A plaintext host in a local file would send the user's API key over the wire
/// in the clear, so it is dropped in favour of Devin's own host rather than
/// honoured.
#[must_use]
pub fn clean_api_server_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim();
    if !trimmed.starts_with("https://") {
        return None;
    }
    let without_slashes = trimmed.trim_end_matches('/');
    if without_slashes.is_empty() {
        None
    } else {
        Some(without_slashes.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{clean_api_server_url, credentials_paths, state_db_paths, toml_string, DevinAuth};
    use crate::usage::creds::{CredentialSource, Secret};

    #[test]
    fn reads_quoted_and_unquoted_toml_values() {
        let text = "windsurf_api_key = \"devin-session-token$cli\"\napi_server_url = https://server.codeium.test  # comment\n";
        assert_eq!(
            toml_string(text, "windsurf_api_key").as_deref(),
            Some("devin-session-token$cli")
        );
        assert_eq!(
            toml_string(text, "api_server_url").as_deref(),
            Some("https://server.codeium.test")
        );
        assert_eq!(toml_string(text, "absent"), None);
    }

    #[test]
    fn an_empty_or_missing_value_is_no_credential() {
        assert_eq!(
            toml_string("windsurf_api_key = \"\"\n", "windsurf_api_key"),
            None
        );
        assert_eq!(
            toml_string("windsurf_api_key =\n", "windsurf_api_key"),
            None
        );
    }

    #[test]
    fn only_an_https_api_host_is_honoured() {
        assert_eq!(
            clean_api_server_url("https://server.codeium.test/").as_deref(),
            Some("https://server.codeium.test")
        );
        // Plaintext would put the API key on the wire in the clear.
        assert_eq!(clean_api_server_url("http://server.codeium.test"), None);
        assert_eq!(clean_api_server_url("  "), None);
    }

    #[test]
    fn candidate_paths_name_devin_only() {
        for path in credentials_paths().iter().chain(state_db_paths().iter()) {
            let shown = path.to_string_lossy().to_lowercase();
            assert!(shown.contains("devin"), "{shown}");
        }
    }

    #[test]
    fn debug_never_prints_the_api_key() {
        let auth = DevinAuth {
            api_key: Secret::new("devin-session-token$cli").unwrap(),
            api_server_url: "https://server.codeium.test".to_string(),
            source: CredentialSource::File,
        };
        let shown = format!("{auth:?}");
        assert!(!shown.contains("devin-session-token"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }
}
