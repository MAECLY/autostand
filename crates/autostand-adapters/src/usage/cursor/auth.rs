//! Read-only discovery of the Cursor session already on this machine.
//!
//! Cursor keeps its session in two places: the editor's `state.vscdb`
//! (`cursorAuth/accessToken`) and, for some installs, the macOS keychain
//! (`cursor-access-token`). Both are read through the shared read-only helpers —
//! the `SQLite` handle is opened `SQLITE_OPEN_READ_ONLY`, so this module could not
//! write even if it tried.
//!
//! **The token is never refreshed.** `OpenUsage` exchanges the refresh token and
//! writes the rotated access token back into Cursor's own database; autostand
//! does not touch another app's credential store, so an expired token is
//! reported as `session_expired` and the user signs in through Cursor again.
//! That is also why the refresh token is never read at all: nothing here could
//! use it.

use std::path::PathBuf;

use chrono::{DateTime, Utc};

use crate::usage::creds::keychain::{self, KeychainAccess};
use crate::usage::creds::{files, vscdb, CredentialSource, Secret};
use crate::usage::model::UsageError;
use crate::usage::parse;

/// The editor directory Cursor keeps its global storage under.
const STATE_DB_APP: &str = "Cursor";

/// `ItemTable` key holding the access token.
pub const ACCESS_TOKEN_KEY: &str = "cursorAuth/accessToken";

/// `ItemTable` key holding the plan the editor last saw.
pub const MEMBERSHIP_TYPE_KEY: &str = "cursorAuth/stripeMembershipType";

/// Keychain service some Cursor installs store the access token under.
pub const KEYCHAIN_ACCESS_TOKEN_SERVICE: &str = "cursor-access-token";

/// One usable Cursor session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CursorCredential {
    pub access_token: Secret,
    pub source: CredentialSource,
}

/// The web session derived from an access token.
///
/// `cursor.com`'s REST endpoints authenticate with a cookie rather than a bearer
/// header, and the cookie is the account id joined to the token. `Debug` shows
/// only the account id — the cookie carries the token verbatim.
#[derive(Clone, PartialEq, Eq)]
pub struct CursorSession {
    pub user_id: String,
    cookie_value: String,
}

impl std::fmt::Debug for CursorSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorSession")
            .field("user_id", &self.user_id)
            .field("cookie_value", &"<redacted>")
            .finish()
    }
}

impl CursorSession {
    /// The `Cookie` header value for a `cursor.com` REST call.
    #[must_use]
    pub fn cookie_header(&self) -> String {
        format!("WorkosCursorSessionToken={}", self.cookie_value)
    }
}

/// Candidate paths for Cursor's `state.vscdb`.
#[must_use]
pub fn state_db_paths() -> Vec<PathBuf> {
    vscdb::state_db_paths(STATE_DB_APP)
}

/// Whether a Cursor session exists, without reading a secret.
///
/// The database is opened read-only and asked for one key; the keychain is only
/// asked whether an item *exists*, which cannot raise an unlock dialog.
pub async fn has_local_credentials() -> bool {
    let paths = state_db_paths();
    let in_state_db = tokio::task::spawn_blocking(move || {
        vscdb::read_item_from_any(&paths, ACCESS_TOKEN_KEY).unwrap_or(None)
    })
    .await
    .ok()
    .flatten()
    .is_some();
    if in_state_db {
        return true;
    }
    keychain::generic_password_exists(KEYCHAIN_ACCESS_TOKEN_SERVICE, None)
        .await
        .unwrap_or(false)
}

/// What the editor database holds, read in one pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StateDbAuth {
    pub access_token: Option<Secret>,
    /// Lower-cased plan name the editor last recorded.
    pub membership_type: Option<String>,
}

/// Read the editor database. Blocking; call from
/// [`tokio::task::spawn_blocking`].
pub fn load_state_db() -> Result<StateDbAuth, UsageError> {
    let paths = state_db_paths();
    Ok(StateDbAuth {
        access_token: vscdb::read_item_from_any(&paths, ACCESS_TOKEN_KEY)?
            .as_deref()
            .and_then(Secret::new),
        membership_type: vscdb::read_item_from_any(&paths, MEMBERSHIP_TYPE_KEY)?
            .map(|value| value.trim().to_ascii_lowercase()),
    })
}

/// The session to probe with, or `None` when this machine has no Cursor login.
///
/// The keychain is only consulted when `access` allows a secret read, so a
/// background pass silently uses the editor database alone.
pub async fn load(access: KeychainAccess) -> Result<Option<CursorCredential>, UsageError> {
    let state = match tokio::task::spawn_blocking(load_state_db).await {
        Ok(result) => result?,
        // A panicked blocking read is a broken store, not a logout.
        Err(_) => return Err(UsageError::CredentialStoreUnavailable),
    };
    let from_keychain =
        keychain::read_generic_password(KEYCHAIN_ACCESS_TOKEN_SERVICE, None, access)
            .await
            .found()
            .as_deref()
            .and_then(Secret::new);

    Ok(choose(&state, from_keychain.as_ref()))
}

/// Pick between the editor database and the keychain.
///
/// The database normally wins. The exception is the account-mismatch case
/// `OpenUsage` documents: a database that says the plan is `free` while the
/// keychain holds a token for a *different* account means the editor is signed
/// in to a second, unpaid account, and the keychain token is the one whose quota
/// the user wants to see.
#[must_use]
pub fn choose(state: &StateDbAuth, from_keychain: Option<&Secret>) -> Option<CursorCredential> {
    if let Some(from_state) = state.access_token.as_ref() {
        if let Some(keychain_token) = from_keychain {
            let state_subject = token_subject(from_state.as_str());
            let keychain_subject = token_subject(keychain_token.as_str());
            let accounts_differ = state_subject.is_some()
                && keychain_subject.is_some()
                && state_subject != keychain_subject;
            if state.membership_type.as_deref() == Some("free") && accounts_differ {
                return Some(CursorCredential {
                    access_token: keychain_token.clone(),
                    source: CredentialSource::KeychainLegacy,
                });
            }
        }
        return Some(CursorCredential {
            access_token: from_state.clone(),
            source: CredentialSource::File,
        });
    }
    from_keychain.map(|token| CursorCredential {
        access_token: token.clone(),
        source: CredentialSource::KeychainLegacy,
    })
}

/// Whether the token has already expired.
///
/// No safety buffer: autostand cannot refresh, so the only question worth asking
/// is whether the token is still valid *now*. A token with no `exp` claim is
/// tried, and the endpoint decides.
#[must_use]
pub fn is_expired(token: &str, now: DateTime<Utc>) -> bool {
    parse::jwt_expiry(token).is_some_and(|expiry| expiry <= now)
}

/// The `sub` claim, which identifies the signed-in account.
#[must_use]
pub fn token_subject(token: &str) -> Option<String> {
    parse::jwt_payload(token)
        .as_ref()
        .and_then(|payload| payload.get("sub"))
        .and_then(parse::text)
        .map(str::to_string)
}

/// The web session a `cursor.com` REST call needs.
///
/// The account id is the part of `sub` after the identity-provider prefix
/// (`google-oauth2|user_abc` → `user_abc`), and the cookie is that id joined to
/// the token by an encoded `::`. `None` when the token carries no usable
/// subject, or one outside the character set a URL query can hold — which is
/// what keeps a value read from a token out of a request line.
#[must_use]
pub fn session(token: &str) -> Option<CursorSession> {
    let subject = token_subject(token)?;
    // `sub` is `<identity-provider>|<account id>`; a subject with no separator is
    // the account id itself. The second segment is taken rather than the last, to
    // match the account id Cursor's own dashboard uses.
    let mut segments = subject.split('|');
    let first = segments.next().unwrap_or_default();
    let user_id = segments.next().unwrap_or(first).trim();
    if user_id.is_empty()
        || !user_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return None;
    }
    Some(CursorSession {
        cookie_value: format!("{user_id}%3A%3A{token}"),
        user_id: user_id.to_string(),
    })
}

/// Whether any Cursor credential file exists at all — metadata only.
#[must_use]
pub fn any_state_db_exists() -> bool {
    files::any_exists(&state_db_paths())
}

#[cfg(test)]
mod tests {
    use super::{choose, is_expired, session, state_db_paths, token_subject, StateDbAuth};
    use crate::usage::creds::{CredentialSource, Secret};
    use chrono::{TimeZone, Utc};

    /// A JWT with the given claims. Only the payload segment is ever read: the
    /// token is the vendor's own and no signature is verified.
    fn jwt(claims: &str) -> String {
        let encoded = base64url(claims.as_bytes());
        format!("header.{encoded}.signature")
    }

    fn base64url(bytes: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in bytes.chunks(3) {
            let mut buffer = [0u8; 3];
            buffer[..chunk.len()].copy_from_slice(chunk);
            let value =
                (u32::from(buffer[0]) << 16) | (u32::from(buffer[1]) << 8) | u32::from(buffer[2]);
            for index in 0..=chunk.len() {
                let shift = 18 - 6 * index;
                let sextet = u8::try_from((value >> shift) & 0x3f).expect("six bits fit in a byte");
                out.push(char::from(ALPHABET[usize::from(sextet)]));
            }
        }
        out
    }

    #[test]
    fn reads_the_account_from_the_token_subject() {
        let token = jwt(r#"{"sub":"google-oauth2|user_abc123","exp":9999999999}"#);
        assert_eq!(
            token_subject(&token).as_deref(),
            Some("google-oauth2|user_abc123")
        );
        assert_eq!(session(&token).unwrap().user_id, "user_abc123");
    }

    #[test]
    fn the_session_cookie_joins_the_account_to_the_token() {
        let token = jwt(r#"{"sub":"user_abc123"}"#);
        let session = session(&token).unwrap();
        assert_eq!(
            session.cookie_header(),
            format!("WorkosCursorSessionToken=user_abc123%3A%3A{token}")
        );
    }

    #[test]
    fn a_subject_outside_the_url_alphabet_yields_no_session() {
        // The account id is spliced into a query string, so it never escapes it.
        let token = jwt(r#"{"sub":"auth0|user abc&admin=1"}"#);
        assert!(session(&token).is_none());
        assert!(session("not-a-jwt").is_none());
    }

    #[test]
    fn debug_never_prints_the_session_cookie() {
        let token = jwt(r#"{"sub":"user_abc123"}"#);
        let shown = format!("{:?}", session(&token).unwrap());
        assert!(!shown.contains(&token), "{shown}");
        assert!(shown.contains("user_abc123"), "{shown}");
    }

    #[test]
    fn an_expired_token_is_reported_rather_than_refreshed() {
        let now = Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap();
        let past = jwt(r#"{"sub":"user_a","exp":1000000000}"#);
        let future = jwt(r#"{"sub":"user_a","exp":9999999999}"#);
        assert!(is_expired(&past, now));
        assert!(!is_expired(&future, now));
        // No `exp` claim: try it and let the endpoint decide.
        assert!(!is_expired(&jwt(r#"{"sub":"user_a"}"#), now));
    }

    #[test]
    fn the_editor_database_normally_wins() {
        let state = StateDbAuth {
            access_token: Secret::new(&jwt(r#"{"sub":"auth0|user_state"}"#)),
            membership_type: Some("pro".to_string()),
        };
        let keychain = Secret::new(&jwt(r#"{"sub":"auth0|user_keychain"}"#));

        let chosen = choose(&state, keychain.as_ref()).unwrap();

        assert_eq!(chosen.source, CredentialSource::File);
        assert_eq!(chosen.access_token, state.access_token.unwrap());
    }

    #[test]
    fn a_free_editor_account_defers_to_a_keychain_token_for_another_account() {
        let state = StateDbAuth {
            access_token: Secret::new(&jwt(r#"{"sub":"auth0|user_state"}"#)),
            membership_type: Some("free".to_string()),
        };
        let keychain = Secret::new(&jwt(r#"{"sub":"auth0|user_keychain"}"#));

        let chosen = choose(&state, keychain.as_ref()).unwrap();

        assert_eq!(chosen.source, CredentialSource::KeychainLegacy);
        assert_eq!(chosen.access_token, keychain.unwrap());
    }

    #[test]
    fn a_free_editor_account_keeps_its_own_token_when_the_account_matches() {
        let token = jwt(r#"{"sub":"auth0|user_same"}"#);
        let state = StateDbAuth {
            access_token: Secret::new(&token),
            membership_type: Some("free".to_string()),
        };

        let chosen = choose(&state, Secret::new(&token).as_ref()).unwrap();

        assert_eq!(chosen.source, CredentialSource::File);
    }

    #[test]
    fn the_keychain_answers_when_the_editor_database_has_nothing() {
        let keychain = Secret::new(&jwt(r#"{"sub":"auth0|user_keychain"}"#));
        let chosen = choose(&StateDbAuth::default(), keychain.as_ref()).unwrap();
        assert_eq!(chosen.source, CredentialSource::KeychainLegacy);

        assert!(choose(&StateDbAuth::default(), None).is_none());
    }

    #[test]
    fn candidate_paths_name_cursor_only() {
        for path in state_db_paths() {
            assert!(path.to_string_lossy().contains("Cursor"), "{path:?}");
        }
    }
}
