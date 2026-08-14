//! Grok usage probe.
//!
//! Reads the credential `grok login` already wrote to `~/.grok/auth.json` and
//! asks the same billing endpoint the Grok CLI asks — `GET /v1/billing?format=credits`
//! on `cli-chat-proxy.grok.com` — for the weekly shared-pool meter.
//!
//! Three pieces, per the module contract:
//!
//! - [`auth`] — read-only credential discovery, multi-account aware.
//! - [`client`] — the requests, returning status + headers + body untouched.
//! - [`mapper`] — pure: `(response, now) -> ProviderSnapshot`.
//!
//! **Out of scope**, deliberately: the pay-as-you-go badge and the local spend
//! tiles `OpenUsage` estimates from the Grok CLI log. Both are dollar-denominated
//! and need a maintained model-pricing table, which `docs/specs/provider-usage.md`
//! tracks separately.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::Utc;

use super::{ProbeContext, ProviderSnapshot, UsageProbe};

/// Stable provider id, matching `LlmAdapter::id()`.
pub const PROVIDER_ID: &str = "grok";

/// Reads Grok's weekly shared-pool quota.
#[derive(Debug, Clone, Copy, Default)]
pub struct GrokProbe;

impl GrokProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for GrokProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_credentials()
    }

    /// Grok's credential is a file, never a keychain item, so `ctx` carries
    /// nothing this probe acts on: there is no prompt to defer.
    async fn probe(&self, _ctx: &ProbeContext) -> ProviderSnapshot {
        // The I/O boundary owns the clock; everything downstream takes it as a
        // parameter so the mapper stays pure.
        let now = Utc::now();

        let token = match auth::load_token(now) {
            Ok(token) => token,
            Err(error) => return ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        };

        let credits = match client::fetch_credits(&token).await {
            Ok(response) => response,
            Err(error) => return ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        };

        // Best-effort: the plan label is worth one extra request and no risk.
        let settings = client::fetch_settings(&token).await.ok();
        mapper::map(&credits, settings.as_ref(), now)
    }
}

#[cfg(test)]
mod tests {
    use super::{GrokProbe, PROVIDER_ID};
    use crate::usage::{ProbeContext, UsageProbe};

    #[test]
    fn the_id_matches_the_llm_adapter() {
        assert_eq!(GrokProbe::new().id(), "grok");
        assert_eq!(PROVIDER_ID, "grok");
    }

    #[tokio::test]
    async fn a_machine_without_the_grok_cli_reports_no_credentials() {
        // `has_local_credentials` must never touch the network, so this is safe
        // to assert unconditionally: it is a file-metadata check either way.
        let probe = GrokProbe::new();
        let found = probe.has_local_credentials().await;
        assert_eq!(found, super::auth::has_credentials());
    }

    #[test]
    fn the_probe_defers_nothing_to_the_keychain() {
        // Documented behaviour: Grok's credential is a file, so a background
        // pass is as capable as a manual one.
        assert!(!ProbeContext::background().is_manual);
    }
}
