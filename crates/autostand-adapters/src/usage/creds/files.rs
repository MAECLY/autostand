//! Read-only access to credential files that already exist in the user's home.
//!
//! autostand **never** writes, refreshes, rotates or deletes a third-party
//! credential — there is no write function in this module, and adding one would
//! break the decision this whole subsystem rests on. An expired token is
//! reported as `auth_required`, not renewed.
//!
//! This is the portable half of credential discovery: the paths resolve on
//! macOS, Windows and Linux alike. The keychain half (see
//! [`super::keychain`]) is macOS-only and degrades cleanly elsewhere.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;

use crate::usage::model::UsageError;
use crate::usage::parse;

/// The user's home directory, or `None` on a host that does not report one.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    dirs::home_dir()
}

/// Resolve a `~`-relative or absolute path.
///
/// A bare relative path is returned unchanged so a caller can pass an already
/// absolute `PathBuf` through without special-casing.
#[must_use]
pub fn expand_home(path: &str) -> Option<PathBuf> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Some(home_dir()?.join(rest));
    }
    if trimmed == "~" {
        return home_dir();
    }
    Some(PathBuf::from(trimmed))
}

/// A path under the user's home: `home_relative(".claude/.credentials.json")`.
#[must_use]
pub fn home_relative(relative: &str) -> Option<PathBuf> {
    Some(home_dir()?.join(relative))
}

/// An environment variable's value, trimmed, with empty treated as unset.
///
/// Providers use these for home overrides (`CLAUDE_CONFIG_DIR`, `CODEX_HOME`),
/// where an exported-but-blank value means "not configured", not "the empty
/// directory".
#[must_use]
pub fn env_text(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

/// Read a file that may not exist.
///
/// A missing file is `Ok(None)` — the user is simply not logged in with this
/// provider. Any other failure (permissions, an unreadable device) is
/// [`UsageError::CredentialStoreUnavailable`]: broken storage is not the same
/// fact as a logout, and reporting it as one sends the user to re-authenticate
/// for no reason.
pub fn read_bytes_if_present(path: &Path) -> Result<Option<Vec<u8>>, UsageError> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err(UsageError::CredentialStoreUnavailable),
    }
}

/// Read a UTF-8 file that may not exist. Same error contract as
/// [`read_bytes_if_present`]; invalid UTF-8 is lossy-decoded rather than fatal,
/// because a credential blob is JSON or hex either way.
pub fn read_text_if_present(path: &Path) -> Result<Option<String>, UsageError> {
    Ok(read_bytes_if_present(path)?.map(|bytes| String::from_utf8_lossy(&bytes).into_owned()))
}

/// Deserialize a credential file, accepting plain or hex-encoded JSON.
///
/// `Ok(None)` when the file is absent. A file that exists but does not decode is
/// [`UsageError::UnsupportedPayload`] — the credential store changed shape, and
/// silently treating that as "logged out" would hide the real problem.
pub fn read_json_if_present<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, UsageError> {
    let Some(bytes) = read_bytes_if_present(path)? else {
        return Ok(None);
    };
    parse::decode_json_with_hex_fallback(&bytes)
        .map(Some)
        .ok_or(UsageError::UnsupportedPayload)
}

/// The first path in `candidates` that exists.
///
/// Candidate order is a provider decision (`$CODEX_HOME` before
/// `~/.config/codex` before `~/.codex`); this only walks it.
#[must_use]
pub fn first_existing(candidates: &[PathBuf]) -> Option<PathBuf> {
    candidates.iter().find(|path| path.is_file()).cloned()
}

/// Whether any candidate path exists.
///
/// This is the file half of `has_local_credentials`: a metadata check, no read,
/// no network, no prompt.
#[must_use]
pub fn any_exists(candidates: &[PathBuf]) -> bool {
    candidates.iter().any(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::{
        any_exists, env_text, expand_home, first_existing, home_dir, home_relative,
        read_bytes_if_present, read_json_if_present, read_text_if_present,
    };
    use crate::usage::model::UsageError;
    use std::path::PathBuf;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[derive(serde::Deserialize, Debug, PartialEq)]
    struct Creds {
        token: String,
    }

    #[test]
    fn a_missing_file_is_not_an_error() {
        let path = PathBuf::from("/definitely/not/a/real/path/creds.json");
        assert_eq!(read_bytes_if_present(&path).unwrap(), None);
        assert_eq!(read_text_if_present(&path).unwrap(), None);
        assert_eq!(read_json_if_present::<Creds>(&path).unwrap(), None);
    }

    #[test]
    fn a_present_file_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creds.json");
        std::fs::write(&path, br#"{"token":"abc"}"#).unwrap();
        assert_eq!(
            read_text_if_present(&path).unwrap().as_deref(),
            Some(r#"{"token":"abc"}"#)
        );
        let decoded: Creds = read_json_if_present(&path).unwrap().unwrap();
        assert_eq!(decoded.token, "abc");
    }

    #[test]
    fn a_hex_encoded_credential_file_still_decodes() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creds.hex");
        std::fs::write(&path, b"7b22746f6b656e223a22616263227d").unwrap();
        let decoded: Creds = read_json_if_present(&path).unwrap().unwrap();
        assert_eq!(decoded.token, "abc");
    }

    #[test]
    fn a_malformed_credential_file_is_not_read_as_a_logout() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("creds.json");
        std::fs::write(&path, b"this is not json").unwrap();
        assert_eq!(
            read_json_if_present::<Creds>(&path).unwrap_err(),
            UsageError::UnsupportedPayload
        );
    }

    #[test]
    fn candidate_order_is_walked_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        let present = dir.path().join("present.json");
        let also_present = dir.path().join("also.json");
        std::fs::write(&present, b"{}").unwrap();
        std::fs::write(&also_present, b"{}").unwrap();

        let candidates = vec![missing.clone(), present.clone(), also_present];
        assert_eq!(first_existing(&candidates), Some(present));
        assert!(any_exists(&candidates));
        assert!(!any_exists(&[missing]));
    }

    #[test]
    fn a_directory_is_not_mistaken_for_a_credential_file() {
        let dir = tempfile::tempdir().unwrap();
        let candidates = vec![dir.path().to_path_buf()];
        assert_eq!(first_existing(&candidates), None);
        assert!(!any_exists(&candidates));
    }

    #[test]
    fn tilde_paths_resolve_against_home() {
        let Some(home) = home_dir() else { return };
        assert_eq!(expand_home("~/.claude"), Some(home.join(".claude")));
        assert_eq!(expand_home("~"), Some(home.clone()));
        assert_eq!(
            home_relative(".claude/.credentials.json"),
            Some(home.join(".claude/.credentials.json"))
        );
    }

    #[test]
    fn absolute_paths_pass_through_and_blanks_are_rejected() {
        assert_eq!(expand_home("/etc/hosts"), Some(PathBuf::from("/etc/hosts")));
        assert_eq!(expand_home("  "), None);
        assert_eq!(expand_home(""), None);
    }

    #[test]
    fn an_exported_but_blank_env_var_counts_as_unset() {
        let _guard = ENV_LOCK.lock().unwrap();
        let name = "AUTOSTAND_TEST_USAGE_HOME_OVERRIDE";
        std::env::set_var(name, "   ");
        assert_eq!(env_text(name), None);
        std::env::set_var(name, " /tmp/config ");
        assert_eq!(env_text(name).as_deref(), Some("/tmp/config"));
        std::env::remove_var(name);
        assert_eq!(env_text(name), None);
    }
}
