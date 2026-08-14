//! Read-only discovery of the Grok CLI's local credential.
//!
//! `grok login` writes `~/.grok/auth.json`, a **map** keyed by login origin
//! (`"https://auth.x.ai::<client id>"`) so one machine can hold several accounts.
//! That shape is preserved here: every entry is a candidate, and the probe picks
//! one rather than assuming a single login.
//!
//! Under the read-only decision autostand never calls the refresh endpoint. The
//! consequence is visible in [`select`]: an expired token is *reported*, and an
//! account whose token still has headroom is preferred over one about to lapse.

use std::collections::BTreeMap;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::usage::creds::{files, Secret};
use crate::usage::model::UsageError;
use crate::usage::parse;

/// Where the Grok CLI keeps its credential, relative to the user's home.
pub const AUTH_RELATIVE_PATH: &str = ".grok/auth.json";

/// A token with less headroom than this is second-best.
///
/// autostand never refreshes, so when several accounts are signed in the one
/// that will still be valid in five minutes is the better bet. A lone token
/// inside the buffer is still used — it works right now, and reporting
/// "signed out" for a request that would have succeeded is the worse error.
const EXPIRY_BUFFER_SECS: i64 = 300;

/// One account entry in `auth.json`.
///
/// Only the fields the probe reads are modelled; unknown siblings
/// (`refresh_token`, `id_token`, `oidc_client_id`, …) are ignored by serde, and
/// the refresh token is deliberately *not* deserialized — there is no code path
/// that could use it.
#[derive(Clone, Deserialize)]
pub struct AuthEntry {
    /// The access token. Absent or blank means this entry is not usable.
    #[serde(default)]
    pub key: Option<String>,
    #[serde(default)]
    pub expires_at: Option<String>,
    #[serde(default)]
    pub expires: Option<String>,
}

// Hand-written: a derived `Debug` would print the access token.
impl std::fmt::Debug for AuthEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthEntry")
            .field("key", &format_args!("<redacted>"))
            .field(
                "has_expiry",
                &(self.expires_at.is_some() || self.expires.is_some()),
            )
            .finish()
    }
}

/// The whole credential file: login origin → entry.
///
/// A `BTreeMap` rather than a `HashMap` so multi-account selection is
/// deterministic; with a hash map the probed account would change between runs.
pub type AuthFile = BTreeMap<String, AuthEntry>;

/// One usable login: a token plus when it stops working.
#[derive(Clone)]
pub struct Candidate {
    /// The `auth.json` key this came from, e.g. `"https://auth.x.ai::<client>"`.
    /// Kept for ordering only; it is never logged or reported.
    pub entry_key: String,
    pub token: Secret,
    /// `None` when neither the JWT nor the entry states an expiry — unknown is
    /// not the same as expired, so such a token is treated as usable.
    pub expires_at: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for Candidate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Candidate")
            // The login origin is an OAuth issuer plus a public client id, not a
            // credential, so it stays visible: multi-account selection is
            // undebuggable without knowing which entry was chosen.
            .field("entry_key", &self.entry_key)
            .field("token", &self.token)
            .field("expires_at", &self.expires_at)
            .finish()
    }
}

impl Candidate {
    /// Seconds until this token expires; `None` when no expiry is stated.
    #[must_use]
    pub fn headroom_secs(&self, now: DateTime<Utc>) -> Option<i64> {
        self.expires_at.map(|at| (at - now).num_seconds())
    }
}

/// The credential file's path, or `None` on a host with no home directory.
#[must_use]
pub fn auth_path() -> Option<PathBuf> {
    files::home_relative(AUTH_RELATIVE_PATH)
}

/// Whether the Grok CLI has ever written a credential here.
///
/// A metadata check: no read, no network, no prompt.
#[must_use]
pub fn has_credentials() -> bool {
    auth_path().is_some_and(|path| files::any_exists(&[path]))
}

/// Read and parse `auth.json`.
///
/// `Ok(None)` when the file is absent. A file that exists but cannot be parsed is
/// [`UsageError::CredentialStoreUnavailable`] rather than
/// `UnsupportedPayload`: broken credential storage is not a payload change, and
/// it is certainly not a logout.
pub fn read_auth_file() -> Result<Option<AuthFile>, UsageError> {
    let Some(path) = auth_path() else {
        return Ok(None);
    };
    files::read_json_if_present::<AuthFile>(&path).map_err(|error| match error {
        UsageError::UnsupportedPayload => UsageError::CredentialStoreUnavailable,
        other => other,
    })
}

/// Every entry that carries a usable token, in deterministic key order.
#[must_use]
pub fn candidates(auth: &AuthFile) -> Vec<Candidate> {
    auth.iter()
        .filter_map(|(entry_key, entry)| {
            let token = Secret::new(entry.key.as_deref()?)?;
            Some(Candidate {
                entry_key: entry_key.clone(),
                expires_at: expiry(&token, entry),
                token,
            })
        })
        .collect()
}

/// Pick the account to probe with.
///
/// Order of preference, and the reason for each step:
///
/// 1. A token with more than five minutes left — it will outlive the request.
/// 2. Any token that has not actually expired — better a short-lived success
///    than a fabricated "signed out".
/// 3. Nothing usable: the expiry is *reported*, never refreshed.
pub fn select(candidates: &[Candidate], now: DateTime<Utc>) -> Result<&Candidate, UsageError> {
    if candidates.is_empty() {
        return Err(UsageError::NotLoggedIn);
    }
    if let Some(found) = candidates.iter().find(|candidate| {
        candidate
            .headroom_secs(now)
            .map_or(true, |secs| secs > EXPIRY_BUFFER_SECS)
    }) {
        return Ok(found);
    }
    if let Some(found) = candidates
        .iter()
        .find(|candidate| candidate.headroom_secs(now).is_some_and(|secs| secs > 0))
    {
        return Ok(found);
    }
    Err(UsageError::SessionExpired)
}

/// Load the token to probe with, in one step.
pub fn load_token(now: DateTime<Utc>) -> Result<Secret, UsageError> {
    let Some(auth) = read_auth_file()? else {
        return Err(UsageError::NotLoggedIn);
    };
    let candidates = candidates(&auth);
    select(&candidates, now).map(|candidate| candidate.token.clone())
}

/// When this login stops working: the JWT's own `exp` first, then whatever the
/// entry recorded. The token is authoritative — the entry's copy can be stale.
fn expiry(token: &Secret, entry: &AuthEntry) -> Option<DateTime<Utc>> {
    parse::jwt_expiry(token.as_str()).or_else(|| {
        entry
            .expires_at
            .as_deref()
            .or(entry.expires.as_deref())
            .and_then(parse::parse_rfc3339)
    })
}

#[cfg(test)]
mod tests {
    use super::{candidates, select, AuthFile, EXPIRY_BUFFER_SECS};
    use crate::usage::model::UsageError;
    use chrono::{DateTime, TimeZone, Utc};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    /// A JWT whose payload is `{"exp": <seconds>}`; header and signature are
    /// throwaway because nothing verifies them.
    fn jwt(exp: i64) -> String {
        let claims = format!(r#"{{"exp":{exp}}}"#);
        format!("aGVhZGVy.{}.c2ln", base64url(claims.as_bytes()))
    }

    fn base64url(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let b = [
                chunk[0],
                *chunk.get(1).unwrap_or(&0),
                *chunk.get(2).unwrap_or(&0),
            ];
            let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            let indices = [n >> 18 & 63, n >> 12 & 63, n >> 6 & 63, n & 63];
            for (position, index) in indices.iter().enumerate() {
                if position <= chunk.len() {
                    out.push(ALPHABET[*index as usize] as char);
                }
            }
        }
        out
    }

    fn auth(json: &str) -> AuthFile {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn a_single_account_is_read_from_the_cli_shape() {
        let auth =
            auth(r#"{"https://auth.x.ai::client":{"key":"token","refresh_token":"refresh"}}"#);
        let candidates = candidates(&auth);
        assert_eq!(candidates.len(), 1);
        assert_eq!(candidates[0].token.as_str(), "token");
        assert_eq!(candidates[0].entry_key, "https://auth.x.ai::client");
        // No expiry stated anywhere — unknown must not read as expired.
        assert_eq!(candidates[0].expires_at, None);
        assert_eq!(select(&candidates, now()).unwrap().token.as_str(), "token");
    }

    #[test]
    fn an_entry_without_a_usable_key_is_not_a_candidate() {
        let auth =
            auth(r#"{"a":{"key":""},"b":{"refresh_token":"only-refresh"},"c":{"key":"  "}}"#);
        assert!(candidates(&auth).is_empty());
        assert_eq!(
            select(&candidates(&auth), now()).unwrap_err(),
            UsageError::NotLoggedIn
        );
    }

    #[test]
    fn candidate_order_is_deterministic_across_runs() {
        // A hash map would probe a different account each launch.
        let auth = auth(r#"{"zeta":{"key":"z"},"alpha":{"key":"a"},"mid":{"key":"m"}}"#);
        let keys: Vec<_> = candidates(&auth)
            .iter()
            .map(|candidate| candidate.entry_key.clone())
            .collect();
        assert_eq!(keys, vec!["alpha", "mid", "zeta"]);
    }

    #[test]
    fn expiry_is_read_from_the_jwt_first() {
        let token = jwt(now().timestamp() + 3_600);
        let auth = auth(&format!(
            r#"{{"a":{{"key":"{token}","expires_at":"2020-01-01T00:00:00Z"}}}}"#
        ));
        let candidates = candidates(&auth);
        // The token outranks the entry's stale copy.
        assert_eq!(candidates[0].headroom_secs(now()), Some(3_600));
    }

    #[test]
    fn expiry_falls_back_to_the_entry_timestamp() {
        let current = auth(r#"{"a":{"key":"opaque-token","expires_at":"2026-08-13T13:00:00Z"}}"#);
        assert_eq!(candidates(&current)[0].headroom_secs(now()), Some(3_600));
        let legacy = auth(r#"{"a":{"key":"opaque-token","expires":"2026-08-13T13:00:00Z"}}"#);
        assert_eq!(candidates(&legacy)[0].headroom_secs(now()), Some(3_600));
    }

    #[test]
    fn a_fresher_account_is_preferred_over_one_about_to_lapse() {
        let nearly = jwt(now().timestamp() + EXPIRY_BUFFER_SECS - 1);
        let fresh = jwt(now().timestamp() + 7_200);
        let auth = auth(&format!(
            r#"{{"a-nearly":{{"key":"{nearly}"}},"b-fresh":{{"key":"{fresh}"}}}}"#
        ));
        let candidates = candidates(&auth);
        assert_eq!(select(&candidates, now()).unwrap().entry_key, "b-fresh");
    }

    #[test]
    fn a_lone_short_lived_token_is_still_used() {
        // It works right now; claiming "signed out" would be the bigger lie.
        let nearly = jwt(now().timestamp() + 60);
        let auth = auth(&format!(r#"{{"a":{{"key":"{nearly}"}}}}"#));
        let candidates = candidates(&auth);
        assert_eq!(select(&candidates, now()).unwrap().entry_key, "a");
    }

    #[test]
    fn every_account_expired_is_reported_never_refreshed() {
        let stale = jwt(now().timestamp() - 60);
        let staler = jwt(now().timestamp() - 86_400);
        let auth = auth(&format!(
            r#"{{"a":{{"key":"{stale}"}},"b":{{"key":"{staler}"}}}}"#
        ));
        let candidates = candidates(&auth);
        assert_eq!(
            select(&candidates, now()).unwrap_err(),
            UsageError::SessionExpired
        );
    }

    #[test]
    fn an_expired_account_does_not_hide_a_working_one() {
        let stale = jwt(now().timestamp() - 60);
        let fresh = jwt(now().timestamp() + 7_200);
        let auth = auth(&format!(
            r#"{{"a-stale":{{"key":"{stale}"}},"b-fresh":{{"key":"{fresh}"}}}}"#
        ));
        let candidates = candidates(&auth);
        assert_eq!(select(&candidates, now()).unwrap().entry_key, "b-fresh");
    }

    #[test]
    fn a_candidate_debug_line_carries_no_token() {
        let auth = auth(r#"{"a":{"key":"super-secret-token"}}"#);
        let rendered = format!("{:?}", candidates(&auth)[0]);
        assert!(!rendered.contains("super-secret-token"), "{rendered}");
    }
}
