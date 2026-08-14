//! Read-only discovery of the Claude Code login that already exists on this
//! machine.
//!
//! There is no write path here and there must never be one. autostand reads
//! Anthropic's own credential to call Anthropic's own usage endpoint; it never
//! writes, refreshes, rotates or deletes it. An expired token is reported as
//! `auth_required`, not renewed.
//!
//! # Candidate order
//!
//! 1. **macOS keychain**, service `"Claude Code-credentials"` — and, when
//!    `CLAUDE_CONFIG_DIR` is set, `"Claude Code-credentials-<sha256[..8]>"`
//!    first. Recent Claude Code versions keep the live session here and can
//!    leave a stale `~/.claude/.credentials.json` behind, so the keychain wins.
//!    Per service: the current user's item (`-a $USER`) first, then the legacy
//!    service-only item.
//! 2. **`$CLAUDE_CONFIG_DIR/.credentials.json`**, else
//!    `~/.claude/.credentials.json`. Plain or hex-encoded JSON.
//! 3. **`CLAUDE_CODE_OAUTH_TOKEN`** — always last. A `claude setup-token` value
//!    can run inference but cannot read subscription limits, so it must never
//!    shadow a real login.
//!
//! The probe walks that order and advances to the next candidate only on an
//! expiry-class rejection, so a fresh `claude` re-login is picked up whichever
//! store it landed in without a stale entry in an earlier store outranking it.

use std::fmt;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::usage::creds::keychain::{self, KeychainAccess, KeychainOutcome};
use crate::usage::creds::{files, short_fingerprint, CredentialSource};
use crate::usage::parse;

/// Home override honoured by Claude Code.
pub const CONFIG_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";

/// An inference-only token, e.g. from `claude setup-token`.
pub const OAUTH_TOKEN_ENV: &str = "CLAUDE_CODE_OAUTH_TOKEN";

/// Scope the usage endpoint requires. A login without it authenticates for
/// inference but cannot read subscription windows.
pub const USAGE_SCOPE: &str = "user:profile";

/// Default config directory when `CLAUDE_CONFIG_DIR` is unset.
const DEFAULT_CONFIG_DIR: &str = "~/.claude";

const CREDENTIALS_FILE: &str = ".credentials.json";

/// Keychain service Claude Code stores the production login under.
///
/// Claude Code appends an environment suffix (`-staging-oauth`, `-local-oauth`,
/// `-custom-oauth`) for its non-production OAuth endpoints. autostand only ever
/// calls `api.anthropic.com`, so reading a staging credential would be a
/// mismatch — only the production service is a candidate.
const KEYCHAIN_SERVICE: &str = "Claude Code-credentials";

/// The Claude Code OAuth block, as stored in the keychain item or the
/// credentials file.
///
/// The refresh token is deliberately **not** deserialized: autostand never
/// refreshes, so reading it would be surface with no purpose.
#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaudeOAuth {
    pub access_token: Option<String>,
    /// Epoch **milliseconds**, as Claude Code writes it.
    pub expires_at: Option<f64>,
    pub subscription_type: Option<String>,
    pub rate_limit_tier: Option<String>,
    pub scopes: Option<Vec<String>>,
}

// Hand-written: a derived `Debug` would print the access token the moment
// someone adds `{:?}` to a trace line.
impl fmt::Debug for ClaudeOAuth {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ClaudeOAuth")
            .field(
                "access_token",
                &format_args!(
                    "<{}>",
                    if self.has_access_token() {
                        "redacted"
                    } else {
                        "absent"
                    }
                ),
            )
            .field("expires_at", &self.expires_at)
            .field("subscription_type", &self.subscription_type)
            .field("rate_limit_tier", &self.rate_limit_tier)
            .field("scopes", &self.scopes)
            .finish()
    }
}

impl ClaudeOAuth {
    /// Whether this block carries a non-blank access token — the single
    /// definition of "usable", shared by candidate loading and
    /// `has_local_credentials` so the two cannot drift.
    #[must_use]
    pub fn has_access_token(&self) -> bool {
        self.access_token
            .as_deref()
            .is_some_and(|token| !token.trim().is_empty())
    }

    /// The trimmed access token, when there is one.
    #[must_use]
    pub fn access_token(&self) -> Option<&str> {
        self.access_token
            .as_deref()
            .map(str::trim)
            .filter(|token| !token.is_empty())
    }

    /// Expiry as an instant, decoded from the epoch-milliseconds field.
    #[must_use]
    pub fn expiry(&self) -> Option<DateTime<Utc>> {
        self.expires_at.and_then(parse::parse_epoch)
    }

    /// Whether the stored expiry has already passed.
    ///
    /// Used only to *order* candidates — never to short-circuit a request. The
    /// endpoint stays the authority on whether a token still works, so a skewed
    /// clock can never manufacture a "session expired" the server would not
    /// have reported.
    #[must_use]
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.expiry().is_some_and(|expiry| expiry <= now)
    }
}

/// One credential candidate: the OAuth block plus where it came from.
#[derive(Debug, Clone)]
pub struct ClaudeCredential {
    pub oauth: ClaudeOAuth,
    /// Log-safe kind only — never the service name, never the path.
    pub source: CredentialSource,
    /// True for `CLAUDE_CODE_OAUTH_TOKEN`: it can run the model but 403s on the
    /// usage endpoint.
    pub inference_only: bool,
}

/// Why the usage endpoint can or cannot be called for a candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiveUsage {
    Available,
    /// An explicit `CLAUDE_CODE_OAUTH_TOKEN`: inference-only by design.
    InferenceOnlyToken,
    /// A stored login whose granted scopes lack [`USAGE_SCOPE`].
    MissingProfileScope,
}

/// Whether a candidate can read live usage.
///
/// Credentials written before Claude Code recorded `scopes` have an
/// absent/empty list; those are treated as capable rather than suppressed —
/// a token that genuinely lacks the scope 403s loudly and is classified then.
#[must_use]
pub fn live_usage(credential: &ClaudeCredential) -> LiveUsage {
    if credential.inference_only {
        return LiveUsage::InferenceOnlyToken;
    }
    match credential.oauth.scopes.as_deref() {
        Some(scopes) if !scopes.is_empty() => {
            if scopes.iter().any(|scope| scope.trim() == USAGE_SCOPE) {
                LiveUsage::Available
            } else {
                LiveUsage::MissingProfileScope
            }
        }
        _ => LiveUsage::Available,
    }
}

/// Everything one credential sweep learned.
#[derive(Debug, Default)]
pub struct CandidateLoad {
    /// Usable candidates in probe order: keychain, then file, then environment.
    pub candidates: Vec<ClaudeCredential>,
    /// A keychain item exists but this pass declined to read it, because a
    /// background refresh must never raise a macOS dialog. Distinct from "no
    /// credential": the user *is* signed in, we simply did not look.
    pub keychain_deferred: bool,
    /// A store existed but could not be consulted (locked keychain, denied
    /// file). Not proof of a logout.
    pub store_unavailable: bool,
}

/// Parse a credential blob, accepting plain or hex-encoded JSON.
///
/// A blob without a usable `claudeAiOauth.accessToken` is `None` — not an
/// error. `~/.claude/.credentials.json` legitimately exists carrying only
/// unrelated MCP OAuth state, and calling that a corrupt credential store would
/// be wrong.
#[must_use]
pub fn parse_credentials(raw: &str) -> Option<ClaudeOAuth> {
    let file: ClaudeCredentialsFile = parse::decode_json_with_hex_fallback(raw.as_bytes())?;
    file.claude_ai_oauth.filter(ClaudeOAuth::has_access_token)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ClaudeCredentialsFile {
    claude_ai_oauth: Option<ClaudeOAuth>,
}

/// Keychain services to try, in order.
#[must_use]
pub fn keychain_service_candidates() -> Vec<String> {
    match files::env_text(CONFIG_DIR_ENV) {
        Some(config_dir) => vec![
            format!("{KEYCHAIN_SERVICE}-{}", short_fingerprint(&config_dir)),
            KEYCHAIN_SERVICE.to_string(),
        ],
        None => vec![KEYCHAIN_SERVICE.to_string()],
    }
}

/// The credentials file this configuration points at.
///
/// `CLAUDE_CONFIG_DIR` *replaces* the default directory rather than adding to
/// it, so a configured install never silently falls back to `~/.claude`.
#[must_use]
pub fn credentials_path() -> Option<PathBuf> {
    let dir = files::env_text(CONFIG_DIR_ENV).unwrap_or_else(|| DEFAULT_CONFIG_DIR.to_string());
    files::expand_home(&dir).map(|dir| dir.join(CREDENTIALS_FILE))
}

/// Cheap, local-only "is there anything to probe?".
///
/// Files are checked by metadata and the keychain by an attribute lookup that
/// omits `-w`, so neither reads a secret and neither can raise a prompt.
pub async fn has_local_credentials() -> bool {
    if files::env_text(OAUTH_TOKEN_ENV).is_some() {
        return true;
    }
    if credentials_path().is_some_and(|path| files::any_exists(&[path])) {
        return true;
    }
    for service in keychain_service_candidates() {
        if keychain_item_exists(&service).await {
            return true;
        }
    }
    false
}

/// Every credential currently readable, in probe order.
///
/// Re-read on every refresh; nothing is cached, so an external `claude` login
/// is visible on the next pass.
pub async fn load_candidates(access: KeychainAccess) -> CandidateLoad {
    let mut load = CandidateLoad::default();

    for service in keychain_service_candidates() {
        match read_keychain_service(&service, access).await {
            ServiceRead::Found(credential) => {
                load.candidates.push(*credential);
                break;
            }
            ServiceRead::NotFound => {}
            ServiceRead::Deferred => {
                // Only "deferred" if there is genuinely something we skipped;
                // otherwise a machine that never logged in would report a
                // credential store it does not have.
                load.keychain_deferred |= keychain_item_exists(&service).await;
            }
            ServiceRead::Failed => load.store_unavailable = true,
        }
    }

    match load_file_credential() {
        Ok(Some(credential)) => load.candidates.push(credential),
        Ok(None) => {}
        Err(()) => load.store_unavailable = true,
    }

    if let Some(token) = files::env_text(OAUTH_TOKEN_ENV) {
        load.candidates
            .push(environment_credential(&token, load.candidates.first()));
    }

    load
}

/// The environment token, borrowing plan metadata from the login it trails.
///
/// The metadata is display-only (`Max 20x`); the scopes are deliberately *not*
/// borrowed, so this candidate is never mistaken for one that can read usage.
fn environment_credential(token: &str, preferred: Option<&ClaudeCredential>) -> ClaudeCredential {
    ClaudeCredential {
        oauth: ClaudeOAuth {
            access_token: Some(token.to_string()),
            expires_at: None,
            subscription_type: preferred.and_then(|c| c.oauth.subscription_type.clone()),
            rate_limit_tier: preferred.and_then(|c| c.oauth.rate_limit_tier.clone()),
            scopes: None,
        },
        source: CredentialSource::Environment,
        inference_only: true,
    }
}

/// `Err(())` means the file exists but could not be read — deliberately not a
/// [`crate::usage::model::UsageError`], so no path or `io::Error` can ride along.
fn load_file_credential() -> Result<Option<ClaudeCredential>, ()> {
    let Some(path) = credentials_path() else {
        return Ok(None);
    };
    let Ok(contents) = files::read_text_if_present(&path) else {
        return Err(());
    };
    Ok(contents
        .as_deref()
        .and_then(parse_credentials)
        .map(|oauth| ClaudeCredential {
            oauth,
            source: CredentialSource::File,
            inference_only: false,
        }))
}

enum ServiceRead {
    Found(Box<ClaudeCredential>),
    NotFound,
    Deferred,
    Failed,
}

impl ServiceRead {
    fn found(raw: &str, source: CredentialSource) -> Self {
        match parse_credentials(raw) {
            Some(oauth) => Self::Found(Box::new(ClaudeCredential {
                oauth,
                source,
                inference_only: false,
            })),
            // A malformed or tokenless item is "nothing here", the same as a
            // miss — the next candidate deserves a turn.
            None => Self::NotFound,
        }
    }
}

/// One service, current-user item first and legacy service-only item second.
async fn read_keychain_service(service: &str, access: KeychainAccess) -> ServiceRead {
    if let Some(account) = keychain::current_user_account() {
        match keychain::read_generic_password(service, Some(&account), access).await {
            KeychainOutcome::Found(raw) => {
                let read = ServiceRead::found(&raw, CredentialSource::KeychainCurrentUser);
                if !matches!(read, ServiceRead::NotFound) {
                    return read;
                }
            }
            KeychainOutcome::NotFound => {}
            KeychainOutcome::Deferred => return ServiceRead::Deferred,
            KeychainOutcome::Unsupported => return ServiceRead::NotFound,
            KeychainOutcome::Failed => return ServiceRead::Failed,
        }
    }

    match keychain::read_generic_password(service, None, access).await {
        KeychainOutcome::Found(raw) => ServiceRead::found(&raw, CredentialSource::KeychainLegacy),
        KeychainOutcome::NotFound | KeychainOutcome::Unsupported => ServiceRead::NotFound,
        KeychainOutcome::Deferred => ServiceRead::Deferred,
        KeychainOutcome::Failed => ServiceRead::Failed,
    }
}

/// Attribute-only existence probe: no secret is read, so this is safe on a
/// background pass and inside `has_local_credentials`.
async fn keychain_item_exists(service: &str) -> bool {
    if let Some(account) = keychain::current_user_account() {
        if keychain::generic_password_exists(service, Some(&account)).await == Some(true) {
            return true;
        }
    }
    // `None` means the probe itself failed. Keep the provider listed rather
    // than hide a login we simply could not confirm.
    keychain::generic_password_exists(service, None)
        .await
        .unwrap_or(true)
}

#[cfg(test)]
mod tests {
    use super::{
        environment_credential, keychain_service_candidates, live_usage, parse_credentials,
        ClaudeCredential, ClaudeOAuth, LiveUsage, CONFIG_DIR_ENV,
    };
    use crate::usage::creds::{short_fingerprint, CredentialSource};
    use chrono::{TimeZone, Utc};
    use std::sync::Mutex;

    /// `std::env::set_var` is process-global; serialise the tests that use it.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn stored(oauth: ClaudeOAuth) -> ClaudeCredential {
        ClaudeCredential {
            oauth,
            source: CredentialSource::File,
            inference_only: false,
        }
    }

    #[test]
    fn a_credential_blob_parses_from_plain_json() {
        let oauth = parse_credentials(
            r#"{"claudeAiOauth":{"accessToken":"token","expiresAt":1786687238979,
                "subscriptionType":"max","rateLimitTier":"default_claude_max_20x",
                "scopes":["user:inference","user:profile"]}}"#,
        )
        .expect("a well-formed blob parses");
        assert_eq!(oauth.subscription_type.as_deref(), Some("max"));
        assert_eq!(
            oauth.rate_limit_tier.as_deref(),
            Some("default_claude_max_20x")
        );
        assert_eq!(oauth.access_token(), Some("token"));
        assert_eq!(
            oauth.expiry(),
            Some(Utc.timestamp_millis_opt(1_786_687_238_979).unwrap())
        );
    }

    #[test]
    fn a_hex_encoded_credential_blob_parses_too() {
        let plain = r#"{"claudeAiOauth":{"accessToken":"token"}}"#;
        let hex = plain.bytes().fold(String::new(), |mut acc, byte| {
            use std::fmt::Write as _;
            let _ = write!(acc, "{byte:02x}");
            acc
        });
        assert!(parse_credentials(&hex).is_some());
    }

    #[test]
    fn a_blob_without_an_oauth_block_is_absence_not_corruption() {
        // `~/.claude/.credentials.json` legitimately exists holding only MCP
        // OAuth state; that is "no Claude login", not a broken store.
        assert!(parse_credentials(r#"{"mcpOAuth":{"some":"server"}}"#).is_none());
        assert!(parse_credentials(r#"{"claudeAiOauth":{"accessToken":"  "}}"#).is_none());
        assert!(parse_credentials("not json at all").is_none());
    }

    #[test]
    fn the_refresh_token_is_never_deserialized() {
        // Read-only: reading it would be surface with no purpose.
        let oauth = parse_credentials(
            r#"{"claudeAiOauth":{"accessToken":"token","refreshToken":"secret-refresh"}}"#,
        )
        .unwrap();
        assert!(!format!("{oauth:?}").contains("secret-refresh"));
    }

    #[test]
    fn debug_never_prints_the_access_token() {
        let oauth =
            parse_credentials(r#"{"claudeAiOauth":{"accessToken":"sk-ant-secret"}}"#).unwrap();
        let rendered = format!("{oauth:?}");
        assert!(!rendered.contains("sk-ant-secret"), "{rendered}");
        assert!(rendered.contains("<redacted>"), "{rendered}");

        let empty = ClaudeOAuth::default();
        assert!(format!("{empty:?}").contains("<absent>"));
    }

    #[test]
    fn expiry_is_read_from_epoch_milliseconds() {
        let now = Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap();
        let expired = ClaudeOAuth {
            expires_at: Some(1_000_000_000_000.0),
            ..ClaudeOAuth::default()
        };
        assert!(expired.is_expired(now));

        let live = ClaudeOAuth {
            expires_at: Some(4_102_444_800_000.0),
            ..ClaudeOAuth::default()
        };
        assert!(!live.is_expired(now));

        // No expiry recorded is not an expiry.
        assert!(!ClaudeOAuth::default().is_expired(now));
    }

    #[test]
    fn scopes_gate_live_usage_but_an_absent_list_does_not() {
        let with_scope = stored(ClaudeOAuth {
            scopes: Some(vec!["user:inference".into(), "user:profile".into()]),
            ..ClaudeOAuth::default()
        });
        assert_eq!(live_usage(&with_scope), LiveUsage::Available);

        let without_scope = stored(ClaudeOAuth {
            scopes: Some(vec!["user:inference".into()]),
            ..ClaudeOAuth::default()
        });
        assert_eq!(live_usage(&without_scope), LiveUsage::MissingProfileScope);

        // Older credentials predate the field; do not suppress them.
        assert_eq!(
            live_usage(&stored(ClaudeOAuth::default())),
            LiveUsage::Available
        );
        let empty = stored(ClaudeOAuth {
            scopes: Some(Vec::new()),
            ..ClaudeOAuth::default()
        });
        assert_eq!(live_usage(&empty), LiveUsage::Available);
    }

    #[test]
    fn an_environment_token_is_inference_only_whatever_scopes_it_borrows() {
        let preferred = stored(ClaudeOAuth {
            subscription_type: Some("max".into()),
            rate_limit_tier: Some("default_claude_max_20x".into()),
            scopes: Some(vec!["user:profile".into()]),
            ..ClaudeOAuth::default()
        });
        let env = environment_credential("env-token", Some(&preferred));

        assert_eq!(env.source, CredentialSource::Environment);
        assert_eq!(env.oauth.access_token(), Some("env-token"));
        // Plan metadata is borrowed for display…
        assert_eq!(env.oauth.subscription_type.as_deref(), Some("max"));
        // …but never the scopes, so it can never masquerade as usage-capable.
        assert_eq!(env.oauth.scopes, None);
        assert_eq!(live_usage(&env), LiveUsage::InferenceOnlyToken);
    }

    #[test]
    fn the_environment_token_carries_no_plan_when_it_stands_alone() {
        let env = environment_credential("env-token", None);
        assert_eq!(env.oauth.subscription_type, None);
        assert_eq!(live_usage(&env), LiveUsage::InferenceOnlyToken);
    }

    #[test]
    fn a_config_dir_adds_a_hashed_service_ahead_of_the_base_one() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let previous = std::env::var(CONFIG_DIR_ENV).ok();

        std::env::remove_var(CONFIG_DIR_ENV);
        assert_eq!(
            keychain_service_candidates(),
            vec!["Claude Code-credentials"]
        );

        std::env::set_var(CONFIG_DIR_ENV, "/Users/dev/.claude-work");
        assert_eq!(
            keychain_service_candidates(),
            vec![
                format!(
                    "Claude Code-credentials-{}",
                    short_fingerprint("/Users/dev/.claude-work")
                ),
                "Claude Code-credentials".to_string(),
            ]
        );

        match previous {
            Some(value) => std::env::set_var(CONFIG_DIR_ENV, value),
            None => std::env::remove_var(CONFIG_DIR_ENV),
        }
    }
}
