//! Z.ai (Zhipu AI) usage probe.
//!
//! Like `OpenRouter`, Z.ai leaves no credential on the machine — the key is one
//! the user supplied, read from autostand's own keychain item (see [`auth`]).
//! The quota endpoint is required; the subscription endpoint is best-effort and
//! only names the plan.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::Utc;

use super::model::UsageError;
use super::{ProbeContext, ProviderSnapshot, UsageProbe};

/// Stable provider id.
pub const PROVIDER_ID: &str = "zai";

/// Reads Z.ai GLM Coding Plan quota.
#[derive(Debug, Clone, Copy, Default)]
pub struct ZaiProbe;

impl ZaiProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for ZaiProbe {
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

        let quota = client::fetch_quota(&key).await;
        // Best-effort: the plan label is never worth failing a refresh over.
        let subscription = client::fetch_subscription(&key).await.ok();

        mapper::map(
            quota.as_ref().map_err(|error| *error),
            subscription.as_ref(),
            now,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{ZaiProbe, PROVIDER_ID};
    use crate::usage::UsageProbe;

    #[test]
    fn the_id_is_stable() {
        assert_eq!(ZaiProbe::new().id(), "zai");
        assert_eq!(PROVIDER_ID, "zai");
    }
}
