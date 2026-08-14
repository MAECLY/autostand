//! Read-only discovery of the `OpenCode` Go credential already on the machine.
//!
//! `OpenCode` keeps one `auth.json` per data directory, holding an entry per
//! provider it has logged into. Only the `opencode-go` entry matters here: its
//! `key` is both the "has the user signed into Go" signal and the bearer token
//! for `GET /zen/go/v1/usage`, so one loader serves both.
//!
//! Directory resolution mirrors `OpenCode` itself: an explicit
//! `OPENCODE_DATA_DIR` wins, then `$XDG_DATA_HOME/opencode`, then the default
//! `~/.local/share/opencode`. All three are portable, so this provider works the
//! same on macOS, Windows and Linux.

use std::path::PathBuf;

use serde_json::Value;

use crate::usage::creds::{files, Secret};
use crate::usage::model::UsageError;
use crate::usage::parse;

/// Explicit override for the data directory.
pub const DATA_DIR_ENV: &str = "OPENCODE_DATA_DIR";

/// XDG base directory, honoured the way `OpenCode` honours it.
pub const XDG_DATA_HOME_ENV: &str = "XDG_DATA_HOME";

/// Default data directory, relative to the user's home.
pub const DEFAULT_RELATIVE_DIR: &str = ".local/share/opencode";

/// Credential file inside the data directory.
pub const AUTH_FILE_NAME: &str = "auth.json";

/// The `auth.json` entry holding the Go subscription key.
pub const GO_ENTRY_KEY: &str = "opencode-go";

/// Where `OpenCode` keeps its local data on this machine.
#[must_use]
pub fn data_dir() -> Option<PathBuf> {
    if let Some(explicit) = files::env_text(DATA_DIR_ENV) {
        return files::expand_home(&explicit);
    }
    if let Some(xdg) = files::env_text(XDG_DATA_HOME_ENV) {
        return files::expand_home(&xdg).map(|base| base.join("opencode"));
    }
    files::home_relative(DEFAULT_RELATIVE_DIR)
}

/// The credential file's path.
#[must_use]
pub fn auth_path() -> Option<PathBuf> {
    Some(data_dir()?.join(AUTH_FILE_NAME))
}

/// Whether `OpenCode` has ever written a credential file here.
///
/// A metadata check only. A file that exists but cannot be parsed still counts:
/// it is an `OpenCode` footprint, and listing the provider is what lets
/// [`crate::usage::UsageProbe::probe`] surface the actionable reason instead of
/// hiding the row.
#[must_use]
pub fn has_credentials() -> bool {
    auth_path().is_some_and(|path| files::any_exists(&[path]))
}

/// The non-empty `opencode-go` key, or `None` when the user has not signed into
/// `OpenCode` Go.
///
/// Reads that one entry and tolerates unrelated siblings — another provider's
/// entry, or a future non-object field — so one odd value cannot hide a valid
/// key.
///
/// A present file that cannot be read or parsed is
/// [`UsageError::CredentialStoreUnavailable`]: broken storage is never mistaken
/// for a logout.
pub fn go_api_key() -> Result<Option<Secret>, UsageError> {
    let Some(path) = auth_path() else {
        return Ok(None);
    };
    let stored: Option<Value> =
        files::read_json_if_present(&path).map_err(|error| match error {
            UsageError::UnsupportedPayload => UsageError::CredentialStoreUnavailable,
            other => other,
        })?;
    let Some(stored) = stored else {
        return Ok(None);
    };
    Ok(read_go_key(&stored))
}

/// Pull the Go key out of a parsed `auth.json`. Pure, so the shape is testable.
#[must_use]
pub fn read_go_key(auth: &Value) -> Option<Secret> {
    let raw = auth.get(GO_ENTRY_KEY)?.get("key")?;
    parse::text(raw).and_then(Secret::new)
}

#[cfg(test)]
mod tests {
    use super::{data_dir, read_go_key, AUTH_FILE_NAME, DEFAULT_RELATIVE_DIR, GO_ENTRY_KEY};
    use crate::usage::creds::files;
    use serde_json::json;
    use std::sync::Mutex;

    /// `std::env` is process-global; the directory-resolution tests mutate it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear() {
        std::env::remove_var(super::DATA_DIR_ENV);
        std::env::remove_var(super::XDG_DATA_HOME_ENV);
    }

    #[test]
    fn the_go_key_is_read_from_its_own_entry() {
        let auth = json!({ GO_ENTRY_KEY: { "key": "oc-go-key", "type": "api" } });
        assert_eq!(read_go_key(&auth).unwrap().as_str(), "oc-go-key");
    }

    #[test]
    fn unrelated_entries_cannot_hide_a_valid_key() {
        let auth = json!({
            "anthropic": { "key": "sk-ant" },
            "schema": 3,
            GO_ENTRY_KEY: { "key": "  oc-go-key\n" }
        });
        assert_eq!(read_go_key(&auth).unwrap().as_str(), "oc-go-key");
    }

    #[test]
    fn a_missing_or_blank_key_is_not_a_login() {
        assert!(read_go_key(&json!({})).is_none());
        assert!(read_go_key(&json!({ "anthropic": { "key": "sk-ant" } })).is_none());
        assert!(read_go_key(&json!({ GO_ENTRY_KEY: {} })).is_none());
        assert!(read_go_key(&json!({ GO_ENTRY_KEY: { "key": "   " } })).is_none());
        assert!(read_go_key(&json!({ GO_ENTRY_KEY: "not-an-object" })).is_none());
    }

    #[test]
    fn the_default_data_directory_is_the_portable_xdg_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear();
        let Some(home) = files::home_dir() else {
            return;
        };
        assert_eq!(data_dir(), Some(home.join(DEFAULT_RELATIVE_DIR)));
        assert_eq!(
            super::auth_path(),
            Some(home.join(DEFAULT_RELATIVE_DIR).join(AUTH_FILE_NAME))
        );
        clear();
    }

    #[test]
    fn an_explicit_override_wins_over_xdg_and_the_default() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear();
        std::env::set_var(super::XDG_DATA_HOME_ENV, "/tmp/xdg");
        assert_eq!(
            data_dir(),
            Some(std::path::PathBuf::from("/tmp/xdg/opencode"))
        );
        std::env::set_var(super::DATA_DIR_ENV, "/tmp/explicit");
        assert_eq!(data_dir(), Some(std::path::PathBuf::from("/tmp/explicit")));
        clear();
    }

    #[test]
    fn an_exported_but_blank_override_falls_through() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear();
        std::env::set_var(super::DATA_DIR_ENV, "   ");
        let Some(home) = files::home_dir() else {
            clear();
            return;
        };
        assert_eq!(data_dir(), Some(home.join(DEFAULT_RELATIVE_DIR)));
        clear();
    }
}
