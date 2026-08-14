//! Codex (`OpenAI`) usage probe.
//!
//! Three pieces, as every provider has:
//!
//! - [`auth`] — read-only credential discovery (`auth.json`, then the keychain).
//! - [`client`] — one `GET /backend-api/wham/usage`.
//! - [`mapper`] — pure `(payload, headers, now) -> ProviderSnapshot`.
//!
//! It replaces the `codex app-server --stdio` spawn the app used to run: no
//! child process, no eight-second protocol handshake, and it reports quota
//! whether or not the `codex` CLI is installed.
//!
//! Every failure mode this provider has is already in [`UsageError`] —
//! `not_logged_in`, `session_expired`, `usage_requires_cli_login`,
//! `credential_store_unavailable`, plus the transport classes — so there is no
//! Codex-specific error enum to keep in sync. If one is ever needed it must
//! implement `ProbeFailure` with an exhaustive match, never a wildcard arm.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::creds::keychain::KeychainAccess;
use super::model::{ProviderSnapshot, UsageError};
use super::{ProbeContext, UsageProbe};

/// autostand's id for this provider.
///
/// `openai`, not `codex`: it must match `LlmAdapter::id()` for the OpenAI/Codex
/// adapter, which is how the registry is consulted and how a snapshot reaches
/// the right Settings row.
pub const PROVIDER_ID: &str = "openai";

/// Reads the subscription quota of the account the `codex` CLI is signed in to.
#[derive(Debug, Default, Clone, Copy)]
pub struct CodexProbe;

impl CodexProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for CodexProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_local_credentials().await
    }

    async fn probe(&self, ctx: &ProbeContext) -> ProviderSnapshot {
        // The one clock read in this provider. Everything below takes `now` as an
        // argument so the mapper stays pure and fixture-testable.
        let now = Utc::now();
        match usage(ctx.keychain_access(), now).await {
            Ok(snapshot) => snapshot,
            Err(error) => ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        }
    }
}

async fn usage(access: KeychainAccess, now: DateTime<Utc>) -> Result<ProviderSnapshot, UsageError> {
    let credential = auth::load(access, now).await?;
    // The source *kind* carries no service name, no path and no secret, so it is
    // safe to trace; nothing else about the credential ever is.
    tracing::debug!(
        provider = PROVIDER_ID,
        source = %credential.source,
        "codex usage credential resolved"
    );
    let response = client::fetch_usage(&credential).await?;
    response.error_for_status(now)?;
    let payload = response.json_value()?;
    Ok(mapper::map(&payload, &response, now))
}

#[cfg(test)]
mod tests {
    use super::{CodexProbe, PROVIDER_ID};
    use crate::usage::creds::keychain::KeychainAccess;
    use crate::usage::model::{Availability, ProviderSnapshot, ReasonCode, UsageError};
    use crate::usage::{ProbeContext, UsageProbe, UsageRegistry};
    use chrono::Utc;

    #[test]
    fn the_probe_answers_to_the_adapter_id_not_the_vendor_name() {
        // Settings keys its rows by `LlmAdapter::id()`; a `codex` id would leave
        // the OpenAI row unprobed and register a provider nothing asks for.
        assert_eq!(CodexProbe::new().id(), "openai");
        assert_eq!(PROVIDER_ID, "openai");
    }

    #[test]
    fn the_builtin_registry_ships_the_codex_probe() {
        assert!(UsageRegistry::with_builtin_probes().contains(PROVIDER_ID));
    }

    #[test]
    fn a_background_pass_defers_the_keychain_and_a_manual_one_does_not() {
        // The gate the whole read-only credential path hangs on: a scheduled
        // refresh must never raise the macOS "allow access" dialog.
        assert_eq!(
            ProbeContext::background().keychain_access(),
            KeychainAccess::Deferred
        );
        assert_eq!(
            ProbeContext::manual().keychain_access(),
            KeychainAccess::Allowed
        );
    }

    #[test]
    fn every_failure_this_probe_can_hit_classifies_without_a_reading() {
        // A failed probe must never leave a number behind, and never claim
        // exhaustion — absence of data is not evidence of a spent quota.
        let now = Utc::now();
        for error in [
            UsageError::NotLoggedIn,
            UsageError::SessionExpired,
            UsageError::UsageRequiresCliLogin,
            UsageError::CredentialStoreUnavailable,
            UsageError::UnsupportedPayload,
            UsageError::Network,
            UsageError::Timeout,
            UsageError::RateLimited {
                retry_after_secs: Some(60),
            },
            UsageError::UnexpectedStatus { status: 503 },
        ] {
            let snapshot = ProviderSnapshot::from_failure(PROVIDER_ID, &error, now);
            assert_eq!(snapshot.provider, PROVIDER_ID);
            assert!(snapshot.resources.is_empty(), "{error:?}");
            assert!(snapshot.reason.is_some(), "{error:?}");
            assert_ne!(snapshot.availability, Availability::Exhausted, "{error:?}");
            assert_ne!(
                snapshot.reason,
                Some(ReasonCode::UsageNotSupported),
                "{error:?}"
            );
        }
        assert_eq!(
            ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::UsageRequiresCliLogin, now)
                .reason,
            Some(ReasonCode::UsageRequiresCliLogin)
        );
    }
}
