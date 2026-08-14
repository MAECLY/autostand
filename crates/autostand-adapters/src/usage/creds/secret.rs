//! A credential string that cannot be printed by accident.
//!
//! Every probe holds a bearer token for the duration of one request. The rule
//! for this subsystem is absolute — *no token, header, URL or response body ever
//! reaches a log, an error or a DTO* — and the cheapest way to enforce it is to
//! make the leak impossible to write: [`Secret`] has a hand-written `Debug` that
//! prints a placeholder, and deliberately implements neither `Display` nor
//! `Serialize`.
//!
//! There is no constructor that accepts a blank string: an empty credential is
//! "not signed in", never a value to send in an `Authorization` header.

/// A secret held only long enough to authenticate one request.
#[derive(Clone, PartialEq, Eq)]
pub struct Secret(String);

// Hand-written: a derived `Debug` would print the token the first time someone
// adds `{:?}` to a trace line.
impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

impl Secret {
    /// Wrap a credential, trimming surrounding whitespace.
    ///
    /// `None` for a blank string — a credential file containing only a newline
    /// is a logout, not a token.
    #[must_use]
    pub fn new(raw: &str) -> Option<Self> {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(Self(trimmed.to_string()))
        }
    }

    /// The raw value, for the one place it belongs: an outgoing request header.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// The value of an `Authorization` header.
    #[must_use]
    pub fn bearer(&self) -> String {
        format!("Bearer {}", self.0)
    }

    /// SHA-256 fingerprint — the only derivative of a credential autostand ever
    /// persists (see [`super::fingerprint`]).
    #[must_use]
    pub fn fingerprint(&self) -> String {
        super::fingerprint(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::Secret;

    #[test]
    fn a_secret_never_reaches_a_debug_line() {
        let secret = Secret::new("sk-or-super-secret").unwrap();
        let rendered = format!("{secret:?}");
        assert!(!rendered.contains("sk-or"), "{rendered}");
        assert_eq!(rendered, "Secret(<redacted>)");
    }

    #[test]
    fn a_blank_credential_is_not_a_credential() {
        assert!(Secret::new("").is_none());
        assert!(Secret::new("   \n").is_none());
    }

    #[test]
    fn surrounding_whitespace_is_trimmed_before_use() {
        // A key file written by `echo` ends in a newline; the header must not.
        let secret = Secret::new("  sk-or-key\n").unwrap();
        assert_eq!(secret.as_str(), "sk-or-key");
        assert_eq!(secret.bearer(), "Bearer sk-or-key");
    }

    #[test]
    fn the_fingerprint_is_irreversible_and_stable() {
        let secret = Secret::new("sk-or-key").unwrap();
        assert_eq!(secret.fingerprint().len(), 64);
        assert!(!secret.fingerprint().contains("sk-or"));
        assert_eq!(
            secret.fingerprint(),
            Secret::new("sk-or-key").unwrap().fingerprint()
        );
        assert_ne!(
            secret.fingerprint(),
            Secret::new("sk-or-other").unwrap().fingerprint()
        );
    }
}
