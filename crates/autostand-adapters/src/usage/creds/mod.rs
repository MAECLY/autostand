//! Read-only discovery of credentials that already exist on this machine.
//!
//! Two halves, deliberately separated by portability:
//!
//! - [`files`] — home-relative credential files. Works on macOS, Windows and
//!   Linux.
//! - [`keychain`] — macOS keychain items. Returns
//!   [`keychain::KeychainOutcome::Unsupported`] elsewhere, so a provider's
//!   candidate list is the same code on every platform.
//! - [`vscdb`] — read-only lookups in the `state.vscdb` key/value store the VS
//!   Code forks (Cursor, Devin) keep their session in. Portable.
//!
//! Neither half can write. That is the decision this subsystem is built on: a
//! third-party token is read to query that vendor's own usage endpoint and is
//! never written, refreshed, rotated or deleted.

pub mod files;
pub mod keychain;
pub mod secret;
pub mod user_key;
pub mod vscdb;

pub use secret::Secret;

use sha2::{Digest, Sha256};

/// Where a credential came from, for display and cache keying.
///
/// The variants carry no service name and no path — only the *kind* — so a
/// source label is safe to log, to put in an error and to place in a DTO.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CredentialSource {
    /// A file under the user's home (or a home override env var).
    File,
    /// A macOS keychain item scoped to the current user (`-a $USER`).
    KeychainCurrentUser,
    /// A macOS keychain item with no account scope (the legacy shape).
    KeychainLegacy,
    /// An environment variable.
    Environment,
}

impl CredentialSource {
    /// Stable, log-safe label.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::File => "file",
            Self::KeychainCurrentUser => "keychain_current_user",
            Self::KeychainLegacy => "keychain_legacy",
            Self::Environment => "environment",
        }
    }
}

impl std::fmt::Display for CredentialSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// SHA-256 of a secret, lowercase hex.
///
/// The *only* thing autostand ever retains about a third-party credential. It
/// keys the usage cache, so a different login starts with clean state instead
/// of briefly showing the previous account's quota, and it is irreversible so it
/// is safe to persist.
#[must_use]
pub fn fingerprint(secret: &str) -> String {
    let digest = Sha256::digest(secret.as_bytes());
    digest.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write as _;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

/// First eight hex characters of [`fingerprint`].
///
/// Claude Code derives its keychain service suffix this way from
/// `CLAUDE_CONFIG_DIR`. The input is hashed as given: the paths this is applied
/// to are ASCII, where NFC normalisation is the identity.
#[must_use]
pub fn short_fingerprint(secret: &str) -> String {
    fingerprint(secret).chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::{fingerprint, short_fingerprint, CredentialSource};

    #[test]
    fn a_fingerprint_is_stable_lowercase_hex() {
        let digest = fingerprint("token-abc");
        assert_eq!(digest.len(), 64);
        assert!(digest.chars().all(|c| c.is_ascii_hexdigit()));
        assert_eq!(digest, digest.to_lowercase());
        assert_eq!(digest, fingerprint("token-abc"));
    }

    #[test]
    fn a_fingerprint_never_contains_its_input() {
        // It is persisted alongside the cache, so it must be irreversible.
        assert!(!fingerprint("super-secret-token").contains("secret"));
    }

    #[test]
    fn different_logins_fingerprint_differently() {
        assert_ne!(fingerprint("token-a"), fingerprint("token-b"));
        assert_ne!(short_fingerprint("token-a"), short_fingerprint("token-b"));
    }

    #[test]
    fn the_short_form_is_the_first_eight_characters() {
        let full = fingerprint("/Users/dev/.claude");
        assert_eq!(short_fingerprint("/Users/dev/.claude"), full[..8]);
        assert_eq!(short_fingerprint("/Users/dev/.claude").len(), 8);
    }

    #[test]
    fn source_labels_carry_only_a_kind() {
        for source in [
            CredentialSource::File,
            CredentialSource::KeychainCurrentUser,
            CredentialSource::KeychainLegacy,
            CredentialSource::Environment,
        ] {
            let label = source.as_str();
            assert_eq!(label, label.to_lowercase());
            assert!(!label.contains('/'), "{label}");
        }
        assert_eq!(CredentialSource::File.to_string(), "file");
    }
}
