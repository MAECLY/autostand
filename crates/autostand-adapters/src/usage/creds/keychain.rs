//! Read-only macOS keychain lookups for credentials another app stored.
//!
//! Three constraints shape this module:
//!
//! 1. **Read only.** There is no write function. autostand reads a vendor's
//!    keychain item to query that vendor's own usage endpoint and does nothing
//!    else with it.
//! 2. **Never on a background refresh.** macOS may prompt before a non-owning
//!    app reads another app's item, so a secret read happens only when the user
//!    asked for it — [`KeychainAccess::Allowed`], which comes from
//!    `ProbeContext { is_manual: true }`. A background pass gets
//!    [`KeychainOutcome::Deferred`] and falls back to the credentials file.
//! 3. **macOS only, degrading cleanly.** `tauri.conf.json` targets all
//!    platforms; on Windows and Linux every call returns
//!    [`KeychainOutcome::Unsupported`] and providers fall back to file paths.
//!
//! `/usr/bin/security` is used rather than the `keyring` crate because these
//! items are looked up by *service* (sometimes with no account at all), which
//! `keyring::Entry` cannot express — it requires both service and user.

use std::time::Duration;

use autostand_runlog::proc::{run_process, ProcSpec, StreamPolicy};

/// The system tool that answers a keychain query.
const SECURITY_BINARY: &str = "/usr/bin/security";

/// Step this lookup would be filed under — it never is; see [`run_security`].
const KEYCHAIN_STEP: &str = "provider_health";

/// Whether this pass is allowed to read a secret from the keychain.
///
/// Derived from `ProbeContext::is_manual`, never chosen ad hoc by a provider.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeychainAccess {
    /// The user asked for this refresh; a system prompt is acceptable.
    Allowed,
    /// A background pass. Never read a secret — a dialog the user did not ask
    /// for is worse than a missing meter.
    Deferred,
}

/// Result of a keychain lookup.
///
/// Five outcomes rather than `Option`, because "not found", "not allowed to
/// look", "wrong platform" and "the lookup itself failed" lead to different
/// reason codes and must not collapse into one another.
#[derive(Clone, PartialEq, Eq)]
pub enum KeychainOutcome {
    /// The secret. Never logged, never placed in an error or a DTO.
    Found(String),
    /// No item matches this service (and account, when given).
    NotFound,
    /// Skipped: this is a background refresh.
    Deferred,
    /// Not macOS.
    Unsupported,
    /// The lookup failed — locked keychain, denied access, cancelled prompt.
    /// Not proof of a logout.
    Failed,
}

// Hand-written so `Found` can never print its payload.
impl std::fmt::Debug for KeychainOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Found(_) => f.write_str("Found(<redacted>)"),
            Self::NotFound => f.write_str("NotFound"),
            Self::Deferred => f.write_str("Deferred"),
            Self::Unsupported => f.write_str("Unsupported"),
            Self::Failed => f.write_str("Failed"),
        }
    }
}

impl KeychainOutcome {
    /// The secret when one was found, dropping every other outcome.
    ///
    /// Use only where the distinction genuinely does not matter; the reason
    /// codes depend on it in most call sites.
    #[must_use]
    pub fn found(self) -> Option<String> {
        match self {
            Self::Found(secret) => Some(secret),
            _ => None,
        }
    }
}

/// `security` exits 44 (`errSecItemNotFound`) when nothing matches — the
/// legitimate "no credential stored" case. Any *other* non-zero exit is a real
/// failure (locked, denied, cancelled) that must not read as "not signed in".
const ITEM_NOT_FOUND_EXIT: i32 = 44;

/// A hung `security` call must not hold up a refresh.
const LOOKUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Read a generic password by service, optionally scoped to an account.
///
/// `account: None` is a service-only lookup (the legacy shape several CLIs
/// still use); `Some(name)` passes `-a <name>`.
pub async fn read_generic_password(
    service: &str,
    account: Option<&str>,
    access: KeychainAccess,
) -> KeychainOutcome {
    if access == KeychainAccess::Deferred {
        return KeychainOutcome::Deferred;
    }
    if !is_supported() {
        return KeychainOutcome::Unsupported;
    }
    let mut args = vec!["find-generic-password".to_string()];
    if let Some(account) = account {
        args.push("-a".to_string());
        args.push(account.to_string());
    }
    args.push("-s".to_string());
    args.push(service.to_string());
    // `-w` prints only the password, so nothing else from the item is ever read.
    args.push("-w".to_string());

    match run_security(&args).await {
        SecurityResult::Ok(stdout) => {
            let secret = stdout.trim();
            if secret.is_empty() {
                KeychainOutcome::NotFound
            } else {
                KeychainOutcome::Found(secret.to_string())
            }
        }
        SecurityResult::NotFound => KeychainOutcome::NotFound,
        SecurityResult::Failed => KeychainOutcome::Failed,
    }
}

/// Read the current user's own item for `service` (`-a $USER`), the shape recent
/// vendor CLIs use, falling back to the legacy service-only item.
pub async fn read_generic_password_for_current_user(
    service: &str,
    access: KeychainAccess,
) -> KeychainOutcome {
    let account = current_user_account();
    if let Some(account) = account {
        let outcome = read_generic_password(service, Some(&account), access).await;
        if !matches!(outcome, KeychainOutcome::NotFound) {
            return outcome;
        }
    }
    read_generic_password(service, None, access).await
}

/// Whether an item exists, without reading its secret.
///
/// Safe on the background path and inside `has_local_credentials`: omitting
/// `-w` asks only for attributes, which cannot raise an unlock prompt. `None`
/// means the probe itself failed, so a caller can pick its own safe side rather
/// than being handed a confident wrong answer.
pub async fn generic_password_exists(service: &str, account: Option<&str>) -> Option<bool> {
    if !is_supported() {
        return Some(false);
    }
    let mut args = vec!["find-generic-password".to_string()];
    if let Some(account) = account {
        args.push("-a".to_string());
        args.push(account.to_string());
    }
    args.push("-s".to_string());
    args.push(service.to_string());

    match run_security(&args).await {
        SecurityResult::Ok(_) => Some(true),
        SecurityResult::NotFound => Some(false),
        SecurityResult::Failed => None,
    }
}

/// True on macOS, where the keychain path exists at all.
#[must_use]
pub fn is_supported() -> bool {
    cfg!(target_os = "macos")
}

/// The account name recent vendor CLIs store their item under.
#[must_use]
pub fn current_user_account() -> Option<String> {
    super::files::env_text("USER").or_else(|| super::files::env_text("LOGNAME"))
}

enum SecurityResult {
    Ok(String),
    NotFound,
    Failed,
}

/// Run `/usr/bin/security` and classify its exit status.
///
/// [`StreamPolicy::Silent`] is not a preference here, it is the invariant: this
/// child's **stdout is the credential**. It still goes through the workspace's
/// single spawner so the timeout/kill policy stays in one place — the spawner
/// simply logs nothing at all for it. The keychain read is also an internal step
/// of a provider-health probe, which reports its own outcome.
async fn run_security(args: &[String]) -> SecurityResult {
    let output = run_process(
        ProcSpec::new(SECURITY_BINARY, KEYCHAIN_STEP)
            .args(args)
            .timeout(LOOKUP_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await;

    match output {
        Ok(output) if output.success => SecurityResult::Ok(output.stdout),
        Ok(output) if output.code == Some(ITEM_NOT_FOUND_EXIT) => SecurityResult::NotFound,
        Ok(_) | Err(_) => SecurityResult::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        generic_password_exists, is_supported, read_generic_password, KeychainAccess,
        KeychainOutcome,
    };

    #[tokio::test]
    async fn a_background_pass_never_touches_the_keychain() {
        // The whole point of `is_manual`: a scheduled refresh must not raise a
        // system dialog. Deferred is returned before any platform check runs.
        let outcome =
            read_generic_password("Claude Code-credentials", None, KeychainAccess::Deferred).await;
        assert_eq!(outcome, KeychainOutcome::Deferred);
    }

    #[tokio::test]
    async fn a_manual_pass_reports_a_definite_outcome_on_every_platform() {
        let outcome = read_generic_password(
            "autostand-test-service-that-does-not-exist-12345",
            None,
            KeychainAccess::Allowed,
        )
        .await;
        if is_supported() {
            assert!(
                matches!(outcome, KeychainOutcome::NotFound | KeychainOutcome::Failed),
                "{outcome:?}"
            );
        } else {
            assert_eq!(outcome, KeychainOutcome::Unsupported);
        }
    }

    #[tokio::test]
    async fn the_existence_probe_needs_no_manual_gate() {
        // Attributes-only: no secret, no prompt, so it is safe on the launch path.
        let exists =
            generic_password_exists("autostand-test-service-that-does-not-exist-12345", None).await;
        assert!(matches!(exists, Some(false) | None), "{exists:?}");
    }

    /// This child's stdout is a credential, so it is the one spawn that must
    /// produce no Terminal line at all — not even "it ran".
    #[tokio::test]
    async fn a_keychain_lookup_writes_nothing_to_the_terminal() {
        let sink = std::sync::Arc::new(autostand_runlog::RecordingSink::new());
        let as_ref: autostand_runlog::SinkRef = sink.clone();
        autostand_runlog::scoped(as_ref, async {
            let _ = read_generic_password(
                "autostand-test-service-that-does-not-exist-12345",
                None,
                KeychainAccess::Allowed,
            )
            .await;
        })
        .await;
        assert!(sink.lines().is_empty(), "{:?}", sink.messages());
    }

    #[test]
    fn a_found_secret_never_reaches_a_debug_line() {
        let outcome = KeychainOutcome::Found("super-secret-token".to_string());
        let rendered = format!("{outcome:?}");
        assert!(!rendered.contains("super-secret-token"), "{rendered}");
        assert_eq!(rendered, "Found(<redacted>)");
    }

    #[test]
    fn the_other_outcomes_stay_distinguishable() {
        // "not found", "not allowed to look" and "the lookup broke" lead to
        // different reason codes, so they must never collapse.
        assert_eq!(format!("{:?}", KeychainOutcome::NotFound), "NotFound");
        assert_eq!(format!("{:?}", KeychainOutcome::Deferred), "Deferred");
        assert_eq!(format!("{:?}", KeychainOutcome::Unsupported), "Unsupported");
        assert_eq!(format!("{:?}", KeychainOutcome::Failed), "Failed");
        assert_eq!(KeychainOutcome::NotFound.found(), None);
        assert_eq!(KeychainOutcome::Failed.found(), None);
        assert_eq!(
            KeychainOutcome::Found("t".into()).found().as_deref(),
            Some("t")
        );
    }

    #[test]
    fn platform_support_matches_the_documented_degradation() {
        assert_eq!(is_supported(), cfg!(target_os = "macos"));
    }
}
