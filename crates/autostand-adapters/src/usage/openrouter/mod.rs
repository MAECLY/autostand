//! `OpenRouter` usage probe.
//!
//! Unlike the CLI-backed providers, `OpenRouter` leaves no credential on the
//! machine — the key is one the user supplied, read from autostand's own
//! keychain item (see [`auth`]). Both usage endpoints are fetched and mapped
//! independently, so a key that is gated on one still shows what the other
//! returned.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::Utc;

use super::model::UsageError;
use super::{ProbeContext, ProviderSnapshot, UsageProbe};

/// Stable provider id.
pub const PROVIDER_ID: &str = "openrouter";

/// Reads `OpenRouter` credit balance and per-key cap.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenRouterProbe;

impl OpenRouterProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for OpenRouterProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_credentials().await
    }

    /// The keychain item read here is autostand's own, so there is no
    /// third-party prompt for `ctx` to defer.
    async fn probe(&self, _ctx: &ProbeContext) -> ProviderSnapshot {
        let now = Utc::now();

        let key = match auth::api_key().await {
            Ok(Some(key)) => key,
            Ok(None) => {
                return ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::NotLoggedIn, now)
            }
            Err(error) => return ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        };

        // Independent on purpose: OpenRouter gates either endpoint for
        // particular key types, so one failure must not blank the other.
        let credits = client::fetch_credits(&key).await;
        let metadata = client::fetch_key(&key).await;

        mapper::map(
            credits.as_ref().map_err(|error| *error),
            metadata.as_ref().map_err(|error| *error),
            now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenRouterProbe, PROVIDER_ID};
    use crate::usage::UsageProbe;

    #[test]
    fn the_id_is_stable() {
        assert_eq!(OpenRouterProbe::new().id(), "openrouter");
        assert_eq!(PROVIDER_ID, "openrouter");
    }
}
