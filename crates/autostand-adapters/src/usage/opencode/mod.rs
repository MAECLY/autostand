//! `OpenCode` usage probe.
//!
//! Reads the `opencode-go` key `OpenCode` already stored in its own `auth.json`
//! and asks `OpenCode`'s official Go usage API for the session, weekly and
//! monthly windows.
//!
//! **Out of scope**, deliberately: the local spend tiles and usage trend
//! `OpenUsage` derives by scanning `OpenCode`'s `SQLite` logs. Those are
//! dollar-denominated and `docs/specs/provider-usage.md` tracks spend
//! separately. One consequence is visible here — [`UsageProbe::has_local_credentials`]
//! checks the credential file only, so a user who has run `OpenCode` locally but
//! never signed into Go reports "not logged in" rather than being listed with
//! nothing to show.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::Utc;

use super::{ProbeContext, ProviderSnapshot, UsageProbe};

/// Stable provider id.
pub const PROVIDER_ID: &str = "opencode";

/// Reads the `OpenCode` Go plan's usage windows.
#[derive(Debug, Clone, Copy, Default)]
pub struct OpenCodeProbe;

impl OpenCodeProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for OpenCodeProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_credentials()
    }

    /// `OpenCode`'s credential is a file, so `ctx` carries nothing to defer.
    async fn probe(&self, _ctx: &ProbeContext) -> ProviderSnapshot {
        let now = Utc::now();

        let key = match auth::go_api_key() {
            Ok(Some(key)) => key,
            // A readable file with no Go entry is a plain "not signed in".
            Ok(None) => {
                return ProviderSnapshot::from_failure(
                    PROVIDER_ID,
                    &crate::usage::model::UsageError::NotLoggedIn,
                    now,
                )
            }
            Err(error) => return ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        };

        match client::fetch_usage(&key).await {
            Ok(response) => mapper::map(&response, now),
            Err(error) => ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{OpenCodeProbe, PROVIDER_ID};
    use crate::usage::UsageProbe;

    #[test]
    fn the_id_is_stable() {
        assert_eq!(OpenCodeProbe::new().id(), "opencode");
        assert_eq!(PROVIDER_ID, "opencode");
    }

    #[tokio::test]
    async fn credential_detection_is_a_file_check_and_nothing_else() {
        let probe = OpenCodeProbe::new();
        assert_eq!(
            probe.has_local_credentials().await,
            super::auth::has_credentials()
        );
    }
}
