//! Read-only discovery of the credential the `codex` CLI already wrote.
//!
//! Three properties of this module are load-bearing:
//!
//! 1. **Read only.** There is no write path. `refresh_token`, `id_token` and
//!    `last_refresh` are not even deserialized: autostand never calls the OAuth
//!    refresh endpoint, so holding a refresh token in memory would buy nothing
//!    and risk everything. A token near its expiry is reported as
//!    [`UsageError::SessionExpired`], never renewed.
//! 2. **An API key is not a login.** An `auth.json` carrying only
//!    `OPENAI_API_KEY` yields [`UsageError::UsageRequiresCliLogin`] and the UI
//!    says exactly that — an API key can run inference but cannot see
//!    subscription quota, and answering "Unavailable" there is a support
//!    question waiting to happen.
//! 3. **Nothing secret escapes.** `Debug` is hand-written on every type that
//!    holds a token, so a stray `{:?}` in a trace line cannot print one.
//!
//! The decision itself — which candidate wins, and which failure the row
//! reports when none do — lives in [`decide`], a pure function over the scan
//! results, so every combination is table-testable without a keychain.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::usage::creds::keychain::{self, KeychainAccess, KeychainOutcome};
use crate::usage::creds::{files, CredentialSource};
use crate::usage::model::UsageError;
use crate::usage::parse;

/// Keychain service the Codex CLI stores its credential under.
pub const KEYCHAIN_SERVICE: &str = "Codex Auth";

/// Home override the CLI honours. When it is set it is the *only* location
/// consulted, exactly as the CLI does: reading a default path the CLI itself
/// ignores would report a login that is not the one in use.
const CODEX_HOME_ENV: &str = "CODEX_HOME";

const AUTH_FILE: &str = "auth.json";

/// Default homes, in the CLI's own probe order.
const DEFAULT_AUTH_HOMES: &[&str] = &[".config/codex", ".codex"];

/// A token at, or within this window of, its JWT `exp` is reported as expired.
///
/// The same five minutes of slack the `codex` CLI itself uses before rotating —
/// autostand simply stops there instead of refreshing.
pub const EXPIRY_SLACK_SECS: i64 = 300;

/// The `tokens` object of `auth.json`.
///
/// Only the two fields the usage request needs are read.
#[derive(Clone, Default, Deserialize)]
pub(crate) struct CodexTokens {
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    account_id: Option<String>,
}

// Hand-written: a derived `Debug` would print the access token verbatim.
impl std::fmt::Debug for CodexTokens {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexTokens")
            .field("access_token", &redacted(self.access_token.as_deref()))
            .field("account_id", &redacted(self.account_id.as_deref()))
            .finish()
    }
}

/// The shape of `auth.json`.
#[derive(Clone, Default, Deserialize)]
pub(crate) struct CodexAuth {
    #[serde(default)]
    tokens: Option<CodexTokens>,
    #[serde(default, rename = "OPENAI_API_KEY")]
    api_key: Option<String>,
}

impl std::fmt::Debug for CodexAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexAuth")
            .field("tokens", &self.tokens)
            .field("api_key", &redacted(self.api_key.as_deref()))
            .finish()
    }
}

fn redacted(value: Option<&str>) -> &'static str {
    if value.is_some_and(|text| !text.trim().is_empty()) {
        "<redacted>"
    } else {
        "<none>"
    }
}

/// A trimmed, non-empty view of an optional credential field.
fn present(value: Option<&String>) -> Option<&str> {
    value
        .map(|text| text.trim())
        .filter(|text| !text.is_empty())
}

impl CodexAuth {
    fn access_token(&self) -> Option<&str> {
        present(self.tokens.as_ref()?.access_token.as_ref())
    }

    fn account_id(&self) -> Option<&str> {
        present(self.tokens.as_ref()?.account_id.as_ref())
    }

    fn api_key(&self) -> Option<&str> {
        present(self.api_key.as_ref())
    }

    /// Whether this file carries *any* credential.
    ///
    /// An API-key-only file counts: it must reach the probe so the row can state
    /// `usage_requires_cli_login` instead of the generic `not_logged_in` the
    /// registry short-circuits to when no credential exists.
    pub(crate) fn is_token_like(&self) -> bool {
        self.access_token().is_some() || self.api_key().is_some()
    }
}

/// The credential a probe will actually send, and where it came from.
#[derive(Clone)]
pub struct CodexCredential {
    pub access_token: String,
    /// `ChatGPT-Account-Id`, when the credential names an account.
    pub account_id: Option<String>,
    pub source: CredentialSource,
}

impl std::fmt::Debug for CodexCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CodexCredential")
            .field("access_token", &"<redacted>")
            .field("account_id", &redacted(self.account_id.as_deref()))
            .field("source", &self.source)
            .finish()
    }
}

/// Candidate credential files, in probe order.
#[must_use]
pub fn auth_paths() -> Vec<PathBuf> {
    auth_paths_from(
        files::env_text(CODEX_HOME_ENV).as_deref(),
        files::home_dir().as_deref(),
    )
}

/// The path list, with both inputs injected so the order is testable without
/// mutating the process environment.
fn auth_paths_from(codex_home: Option<&str>, home: Option<&Path>) -> Vec<PathBuf> {
    if let Some(codex_home) = codex_home {
        return expand(codex_home, home)
            .map(|dir| dir.join(AUTH_FILE))
            .into_iter()
            .collect();
    }
    let Some(home) = home else {
        return Vec::new();
    };
    DEFAULT_AUTH_HOMES
        .iter()
        .map(|dir| home.join(dir).join(AUTH_FILE))
        .collect()
}

/// `~`-expansion against an injected home.
///
/// `files::expand_home` resolves against the process's real home, which a test
/// cannot move; this takes the home as an argument for exactly that reason.
fn expand(raw: &str, home: Option<&Path>) -> Option<PathBuf> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    if let Some(rest) = trimmed.strip_prefix("~/") {
        return Some(home?.join(rest));
    }
    if trimmed == "~" {
        return home.map(Path::to_path_buf);
    }
    Some(PathBuf::from(trimmed))
}

/// What one credential source yielded.
#[derive(Debug)]
enum Candidate {
    /// A usable OAuth access token.
    Token(CodexCredential),
    /// Only `OPENAI_API_KEY`: inference works, quota is invisible.
    ApiKeyOnly,
    /// Nothing credential-shaped.
    Absent,
}

fn classify(auth: &CodexAuth, source: CredentialSource) -> Candidate {
    if let Some(access_token) = auth.access_token() {
        return Candidate::Token(CodexCredential {
            access_token: access_token.to_string(),
            account_id: auth.account_id().map(str::to_string),
            source,
        });
    }
    if auth.api_key().is_some() {
        return Candidate::ApiKeyOnly;
    }
    Candidate::Absent
}

/// The result of walking the candidate files.
#[derive(Debug)]
struct FileScan {
    candidate: Candidate,
    /// A problem that is *not* proof of a logout — an unreadable file, a file
    /// whose shape changed. Reported only if nothing else answers: sending the
    /// user to re-authenticate over a permissions error is a wrong instruction.
    pending: Option<UsageError>,
}

impl FileScan {
    /// The "nothing on disk" scan, used to drive [`decide`] straight from a
    /// keychain outcome.
    #[cfg(test)]
    fn empty() -> Self {
        Self {
            candidate: Candidate::Absent,
            pending: None,
        }
    }
}

/// A keychain lookup and the source label it would carry.
#[derive(Debug)]
struct KeychainRead {
    outcome: KeychainOutcome,
    source: CredentialSource,
}

impl KeychainRead {
    /// The neutral read, used when the files already decided.
    fn skipped() -> Self {
        Self {
            outcome: KeychainOutcome::NotFound,
            source: CredentialSource::KeychainLegacy,
        }
    }
}

/// Resolve the credential to probe with: files first, keychain as the fallback.
pub async fn load(
    access: KeychainAccess,
    now: DateTime<Utc>,
) -> Result<CodexCredential, UsageError> {
    load_from(&auth_paths(), access, now).await
}

/// [`load`] with the candidate paths injected.
pub(crate) async fn load_from(
    paths: &[PathBuf],
    access: KeychainAccess,
    now: DateTime<Utc>,
) -> Result<CodexCredential, UsageError> {
    let scan = scan_files(paths);
    // The keychain is consulted only when the files said nothing, so the common
    // case never risks a macOS prompt.
    let keychain = if matches!(scan.candidate, Candidate::Absent) {
        read_keychain(access).await
    } else {
        KeychainRead::skipped()
    };
    decide(scan, keychain, now)
}

/// Walk the candidate files, stopping at the first that carries a credential.
fn scan_files(paths: &[PathBuf]) -> FileScan {
    let mut pending: Option<UsageError> = None;
    for path in paths {
        match files::read_json_if_present::<CodexAuth>(path) {
            Ok(Some(auth)) => match classify(&auth, CredentialSource::File) {
                Candidate::Absent => {}
                candidate => return FileScan { candidate, pending },
            },
            Ok(None) => {}
            Err(error) => remember(&mut pending, error),
        }
    }
    FileScan {
        candidate: Candidate::Absent,
        pending,
    }
}

fn remember(pending: &mut Option<UsageError>, error: UsageError) {
    if pending.is_none() {
        *pending = Some(error);
    }
}

/// Read the keychain item, honouring the manual-refresh gate.
///
/// Current-user scope first (`-a $USER`), then the legacy service-only item —
/// the two shapes vendor CLIs have used.
async fn read_keychain(access: KeychainAccess) -> KeychainRead {
    if let Some(account) = keychain::current_user_account() {
        let outcome =
            keychain::read_generic_password(KEYCHAIN_SERVICE, Some(&account), access).await;
        if !matches!(outcome, KeychainOutcome::NotFound) {
            return KeychainRead {
                outcome,
                source: CredentialSource::KeychainCurrentUser,
            };
        }
    }
    KeychainRead {
        outcome: keychain::read_generic_password(KEYCHAIN_SERVICE, None, access).await,
        source: CredentialSource::KeychainLegacy,
    }
}

/// Turn the two lookups into one credential or one typed failure.
///
/// Pure: every branch is decided from its arguments, which is what makes the
/// whole precedence order testable.
fn decide(
    scan: FileScan,
    keychain: KeychainRead,
    now: DateTime<Utc>,
) -> Result<CodexCredential, UsageError> {
    let FileScan {
        candidate,
        mut pending,
    } = scan;
    match candidate {
        Candidate::Token(credential) => return validate(credential, now),
        Candidate::ApiKeyOnly => return Err(UsageError::UsageRequiresCliLogin),
        Candidate::Absent => {}
    }

    match keychain.outcome {
        KeychainOutcome::Found(secret) => {
            match parse::decode_json_with_hex_fallback::<CodexAuth>(secret.as_bytes()) {
                Some(auth) => match classify(&auth, keychain.source) {
                    Candidate::Token(credential) => return validate(credential, now),
                    Candidate::ApiKeyOnly => return Err(UsageError::UsageRequiresCliLogin),
                    Candidate::Absent => {}
                },
                None => remember(&mut pending, UsageError::UnsupportedPayload),
            }
        }
        // Not a logout: this pass could not consult the store, so the row reads
        // "no data" rather than telling the user to sign in again.
        KeychainOutcome::Deferred | KeychainOutcome::Failed => {
            remember(&mut pending, UsageError::CredentialStoreUnavailable);
        }
        KeychainOutcome::NotFound | KeychainOutcome::Unsupported => {}
    }

    Err(pending.unwrap_or(UsageError::NotLoggedIn))
}

fn validate(
    credential: CodexCredential,
    now: DateTime<Utc>,
) -> Result<CodexCredential, UsageError> {
    if is_expiring(&credential.access_token, now) {
        return Err(UsageError::SessionExpired);
    }
    Ok(credential)
}

/// Whether the access token is at, or within [`EXPIRY_SLACK_SECS`] of, its JWT
/// `exp`.
///
/// A token whose expiry cannot be read is treated as usable: the endpoint is the
/// authority, and a 401 already classifies as `session_expired`.
pub(crate) fn is_expiring(access_token: &str, now: DateTime<Utc>) -> bool {
    parse::jwt_expiry(access_token)
        .is_some_and(|expires_at| (expires_at - now).num_seconds() <= EXPIRY_SLACK_SECS)
}

/// Local-only credential check: files, then keychain *existence*.
///
/// No secret is read and no network is touched, so this is safe on the launch
/// path and on a background pass.
pub async fn has_local_credentials() -> bool {
    if any_token_like_file(&auth_paths()) {
        return true;
    }
    // Attributes only — omitting `-w` cannot raise an unlock prompt. A lookup
    // that itself failed answers `None`; "no" is the safe side for a filter that
    // only decides whether a row is worth showing.
    keychain::generic_password_exists(KEYCHAIN_SERVICE, None)
        .await
        .unwrap_or(false)
}

pub(crate) fn any_token_like_file(paths: &[PathBuf]) -> bool {
    paths.iter().any(|path| {
        matches!(
            files::read_json_if_present::<CodexAuth>(path),
            Ok(Some(auth)) if auth.is_token_like()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::{
        any_token_like_file, auth_paths_from, decide, is_expiring, load_from, scan_files,
        Candidate, CodexAuth, CodexCredential, CodexTokens, FileScan, KeychainRead,
        EXPIRY_SLACK_SECS,
    };
    use crate::usage::creds::keychain::{KeychainAccess, KeychainOutcome};
    use crate::usage::creds::CredentialSource;
    use crate::usage::model::UsageError;
    use chrono::{DateTime, TimeZone, Utc};
    use std::path::{Path, PathBuf};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    /// Base64url without padding, so a test can mint a JWT with a chosen `exp`.
    fn base64url(input: &[u8]) -> String {
        const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
        let mut out = String::new();
        for chunk in input.chunks(3) {
            let bytes = [
                chunk[0],
                chunk.get(1).copied().unwrap_or(0),
                chunk.get(2).copied().unwrap_or(0),
            ];
            let packed =
                (u32::from(bytes[0]) << 16) | (u32::from(bytes[1]) << 8) | u32::from(bytes[2]);
            for (index, shift) in [18, 12, 6, 0].into_iter().enumerate() {
                if index <= chunk.len() {
                    out.push(ALPHABET[((packed >> shift) & 63) as usize] as char);
                }
            }
        }
        out
    }

    fn jwt(expires_at: DateTime<Utc>) -> String {
        let payload = format!(r#"{{"exp":{}}}"#, expires_at.timestamp());
        format!(
            "{}.{}.{}",
            base64url(br#"{"alg":"none"}"#),
            base64url(payload.as_bytes()),
            "signature-not-verified"
        )
    }

    fn live_token() -> String {
        jwt(now() + chrono::Duration::hours(1))
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    fn oauth_file(access_token: &str, account_id: &str) -> String {
        format!(
            r#"{{"tokens":{{"access_token":"{access_token}","refresh_token":"rt","id_token":"it","account_id":"{account_id}"}},"last_refresh":"2026-08-13T11:00:00Z"}}"#
        )
    }

    #[test]
    fn codex_home_replaces_the_defaults_instead_of_extending_them() {
        // The CLI reads exactly one auth.json when CODEX_HOME is set; probing the
        // defaults too would report a login the CLI itself ignores.
        let home = PathBuf::from("/home/dev");
        assert_eq!(
            auth_paths_from(Some("/opt/codex"), Some(&home)),
            vec![PathBuf::from("/opt/codex/auth.json")]
        );
        assert_eq!(
            auth_paths_from(Some("~/custom-codex"), Some(&home)),
            vec![PathBuf::from("/home/dev/custom-codex/auth.json")]
        );
    }

    #[test]
    fn the_default_order_is_config_codex_then_dot_codex() {
        let home = PathBuf::from("/home/dev");
        assert_eq!(
            auth_paths_from(None, Some(&home)),
            vec![
                PathBuf::from("/home/dev/.config/codex/auth.json"),
                PathBuf::from("/home/dev/.codex/auth.json"),
            ]
        );
        assert!(auth_paths_from(None, None).is_empty());
    }

    #[test]
    fn an_expiring_token_is_reported_not_refreshed() {
        // Read-only decision: the 300s window is where the CLI would rotate and
        // where autostand stops.
        assert!(is_expiring(&jwt(now()), now()));
        assert!(is_expiring(
            &jwt(now() + chrono::Duration::seconds(EXPIRY_SLACK_SECS)),
            now()
        ));
        assert!(!is_expiring(
            &jwt(now() + chrono::Duration::seconds(EXPIRY_SLACK_SECS + 1)),
            now()
        ));
    }

    #[test]
    fn a_token_without_a_readable_expiry_is_left_to_the_endpoint() {
        // No `exp`, not a JWT at all: guessing "expired" would blank a working row.
        assert!(!is_expiring("not-a-jwt", now()));
        assert!(!is_expiring("a.b.c", now()));
    }

    #[tokio::test]
    async fn the_first_file_with_an_access_token_wins() {
        let dir = tempfile::tempdir().unwrap();
        let first = write(
            dir.path(),
            "first.json",
            &oauth_file(&live_token(), "acct-1"),
        );
        let second = write(
            dir.path(),
            "second.json",
            &oauth_file(&live_token(), "acct-2"),
        );
        let credential = load_from(&[first, second], KeychainAccess::Deferred, now())
            .await
            .unwrap();
        assert_eq!(credential.account_id.as_deref(), Some("acct-1"));
        assert_eq!(credential.source, CredentialSource::File);
    }

    #[tokio::test]
    async fn a_missing_file_falls_through_to_the_next_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let missing = dir.path().join("missing.json");
        let present = write(
            dir.path(),
            "present.json",
            &oauth_file(&live_token(), "acct-2"),
        );
        let credential = load_from(&[missing, present], KeychainAccess::Deferred, now())
            .await
            .unwrap();
        assert_eq!(credential.account_id.as_deref(), Some("acct-2"));
    }

    #[tokio::test]
    async fn an_api_key_only_file_says_so_instead_of_reporting_unavailable() {
        // The whole point of the typed reason: an API key cannot see subscription
        // quota, and a generic "Unavailable" here is a support ticket.
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "auth.json", r#"{"OPENAI_API_KEY":"sk-test"}"#);
        assert_eq!(
            load_from(&[path], KeychainAccess::Deferred, now())
                .await
                .unwrap_err(),
            UsageError::UsageRequiresCliLogin
        );
    }

    #[tokio::test]
    async fn an_expired_token_reports_a_session_expiry() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "auth.json",
            &oauth_file(&jwt(now() - chrono::Duration::hours(1)), "acct-1"),
        );
        assert_eq!(
            load_from(&[path], KeychainAccess::Deferred, now())
                .await
                .unwrap_err(),
            UsageError::SessionExpired
        );
    }

    #[tokio::test]
    async fn a_hex_encoded_auth_file_still_decodes() {
        let dir = tempfile::tempdir().unwrap();
        let json = oauth_file(&live_token(), "acct-hex");
        let hex = json.as_bytes().iter().fold(String::new(), |mut acc, byte| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{byte:02x}");
            acc
        });
        let path = write(dir.path(), "auth.json", &hex);
        let credential = load_from(&[path], KeychainAccess::Deferred, now())
            .await
            .unwrap();
        assert_eq!(credential.account_id.as_deref(), Some("acct-hex"));
    }

    #[tokio::test]
    async fn a_malformed_file_does_not_hide_a_good_one_behind_it() {
        let dir = tempfile::tempdir().unwrap();
        let broken = write(dir.path(), "broken.json", "{{{");
        let good = write(
            dir.path(),
            "good.json",
            &oauth_file(&live_token(), "acct-good"),
        );
        let credential = load_from(&[broken, good], KeychainAccess::Deferred, now())
            .await
            .unwrap();
        assert_eq!(credential.account_id.as_deref(), Some("acct-good"));
    }

    #[test]
    fn a_credential_file_without_tokens_is_skipped_not_decisive() {
        let dir = tempfile::tempdir().unwrap();
        let empty = write(dir.path(), "empty.json", "{}");
        let good = write(
            dir.path(),
            "good.json",
            &oauth_file(&live_token(), "acct-2"),
        );
        let scan = scan_files(&[empty, good]);
        assert!(matches!(scan.candidate, Candidate::Token(_)));
        assert_eq!(scan.pending, None);
    }

    #[test]
    fn a_malformed_file_is_remembered_as_a_shape_change_not_a_logout() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "auth.json", "not json at all");
        let scan = scan_files(&[path]);
        assert!(matches!(scan.candidate, Candidate::Absent));
        assert_eq!(scan.pending, Some(UsageError::UnsupportedPayload));
        assert_eq!(
            decide(scan, KeychainRead::skipped(), now()).unwrap_err(),
            UsageError::UnsupportedPayload
        );
    }

    #[test]
    fn no_candidate_anywhere_is_a_plain_logout() {
        assert_eq!(
            decide(FileScan::empty(), KeychainRead::skipped(), now()).unwrap_err(),
            UsageError::NotLoggedIn
        );
    }

    #[test]
    fn a_keychain_this_pass_could_not_read_is_not_a_logout() {
        // Deferred (background pass) and Failed (locked, denied, cancelled) both
        // mean "we did not look", which must never read as "you are signed out".
        for outcome in [KeychainOutcome::Deferred, KeychainOutcome::Failed] {
            let read = KeychainRead {
                outcome,
                source: CredentialSource::KeychainLegacy,
            };
            assert_eq!(
                decide(FileScan::empty(), read, now()).unwrap_err(),
                UsageError::CredentialStoreUnavailable
            );
        }
    }

    #[test]
    fn a_platform_without_a_keychain_reports_a_logout_not_a_broken_store() {
        // Windows and Linux have no keychain path at all; with no file either,
        // "not logged in" is the accurate fact.
        let read = KeychainRead {
            outcome: KeychainOutcome::Unsupported,
            source: CredentialSource::KeychainLegacy,
        };
        assert_eq!(
            decide(FileScan::empty(), read, now()).unwrap_err(),
            UsageError::NotLoggedIn
        );
    }

    #[test]
    fn a_keychain_credential_is_used_when_no_file_carries_one() {
        let read = KeychainRead {
            outcome: KeychainOutcome::Found(oauth_file(&live_token(), "acct-kc")),
            source: CredentialSource::KeychainCurrentUser,
        };
        let credential = decide(FileScan::empty(), read, now()).unwrap();
        assert_eq!(credential.account_id.as_deref(), Some("acct-kc"));
        assert_eq!(credential.source, CredentialSource::KeychainCurrentUser);
    }

    #[test]
    fn a_keychain_item_that_is_only_an_api_key_reports_the_cli_login_reason() {
        let read = KeychainRead {
            outcome: KeychainOutcome::Found(r#"{"OPENAI_API_KEY":"sk-test"}"#.to_string()),
            source: CredentialSource::KeychainLegacy,
        };
        assert_eq!(
            decide(FileScan::empty(), read, now()).unwrap_err(),
            UsageError::UsageRequiresCliLogin
        );
    }

    #[test]
    fn an_unreadable_keychain_payload_degrades_instead_of_guessing() {
        let read = KeychainRead {
            outcome: KeychainOutcome::Found("not json".to_string()),
            source: CredentialSource::KeychainLegacy,
        };
        assert_eq!(
            decide(FileScan::empty(), read, now()).unwrap_err(),
            UsageError::UnsupportedPayload
        );
    }

    #[test]
    fn a_file_credential_outranks_the_keychain() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "auth.json",
            &oauth_file(&live_token(), "acct-file"),
        );
        let read = KeychainRead {
            outcome: KeychainOutcome::Found(oauth_file(&live_token(), "acct-kc")),
            source: CredentialSource::KeychainCurrentUser,
        };
        let credential = decide(scan_files(&[path]), read, now()).unwrap();
        assert_eq!(credential.account_id.as_deref(), Some("acct-file"));
    }

    #[test]
    fn an_api_key_only_file_still_counts_as_a_local_credential() {
        // Otherwise the registry short-circuits to `not_logged_in` and the typed
        // `usage_requires_cli_login` reason never reaches the UI.
        let dir = tempfile::tempdir().unwrap();
        let api_key = write(dir.path(), "api.json", r#"{"OPENAI_API_KEY":"sk-test"}"#);
        let empty = write(dir.path(), "empty.json", "{}");
        let missing = dir.path().join("missing.json");
        assert!(any_token_like_file(&[api_key]));
        assert!(!any_token_like_file(&[empty]));
        assert!(!any_token_like_file(&[missing]));
    }

    #[test]
    fn blank_credential_fields_count_as_absent() {
        let auth: CodexAuth =
            serde_json::from_str(r#"{"tokens":{"access_token":"   "},"OPENAI_API_KEY":""}"#)
                .unwrap();
        assert!(!auth.is_token_like());
    }

    #[test]
    fn no_debug_line_can_print_a_token() {
        let tokens: CodexTokens =
            serde_json::from_str(r#"{"access_token":"super-secret-token","account_id":"acct"}"#)
                .unwrap();
        let auth = CodexAuth {
            tokens: Some(tokens),
            api_key: Some("sk-super-secret".to_string()),
        };
        let rendered = format!("{auth:?}");
        assert!(!rendered.contains("super-secret-token"), "{rendered}");
        assert!(!rendered.contains("sk-super-secret"), "{rendered}");

        let credential = CodexCredential {
            access_token: "super-secret-token".to_string(),
            account_id: Some("acct-1".to_string()),
            source: CredentialSource::File,
        };
        let rendered = format!("{credential:?}");
        assert!(!rendered.contains("super-secret-token"), "{rendered}");
        assert!(!rendered.contains("acct-1"), "{rendered}");
        // The source *kind* stays visible: it names no path and no service.
        assert!(rendered.contains("File"), "{rendered}");
    }
}
