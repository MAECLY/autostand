//! Devin usage probe: local credential, daily/weekly quota, extra-usage balance.
//!
//! Three pieces, as every provider here is:
//!
//! - [`auth`] — read-only credential discovery (CLI file, then the app's
//!   `state.vscdb`).
//! - [`client`] — one `GetUserStatus` request.
//! - [`mapper`] — pure `(payload, now) -> ProviderSnapshot`.
//!
//! Devin issues an API key rather than a refreshable OAuth token, so the
//! read-only decision costs nothing here: a rejected key is reported as
//! `session_expired` and the user re-runs `devin auth login`.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::model::{ProviderSnapshot, UsageError};
use super::{ProbeContext, UsageProbe};
use auth::DevinAuth;
use mapper::DevinProbeError;

/// Stable provider id, matching the IPC vocabulary.
pub const PROVIDER_ID: &str = "devin";

/// Reads Devin's seat quota from the session already on this machine.
#[derive(Debug, Default, Clone, Copy)]
pub struct DevinProbe;

impl DevinProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for DevinProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        // Metadata only: no file is parsed and no database is opened, so this
        // stays safe to run on every listing.
        auth::any_credential_file_exists()
    }

    async fn probe(&self, _ctx: &ProbeContext) -> ProviderSnapshot {
        // The registry is the I/O boundary, so the clock is read once here and
        // injected downwards; the mapper never reads it.
        let now = Utc::now();
        let logins = match load_logins().await {
            Ok(logins) => logins,
            Err(error) => return ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        };
        if logins.is_empty() {
            return ProviderSnapshot::from_failure(
                PROVIDER_ID,
                &DevinProbeError::Usage(UsageError::NotLoggedIn),
                now,
            );
        }

        let mut saw_rejected_key = false;
        let mut last_error = None;
        for login in &logins {
            match attempt(login, now).await {
                Ok(snapshot) => return snapshot,
                Err(error) => {
                    saw_rejected_key |=
                        matches!(error, DevinProbeError::Usage(UsageError::SessionExpired));
                    last_error = Some(error);
                }
            }
        }

        // A rejected key is the actionable answer, so it outranks whatever the
        // last attempt happened to fail with.
        let error = if saw_rejected_key {
            DevinProbeError::Usage(UsageError::SessionExpired)
        } else {
            last_error.unwrap_or(DevinProbeError::Usage(UsageError::NotLoggedIn))
        };
        ProviderSnapshot::from_failure(PROVIDER_ID, &error, now)
    }
}

/// Load every local login off the runtime thread — both sources block.
async fn load_logins() -> Result<Vec<DevinAuth>, DevinProbeError> {
    match tokio::task::spawn_blocking(auth::load_all).await {
        Ok(result) => result.map_err(DevinProbeError::from),
        // A panicked blocking task is a broken credential read, not a logout.
        Err(_) => Err(DevinProbeError::Usage(
            UsageError::CredentialStoreUnavailable,
        )),
    }
}

async fn attempt(
    auth: &DevinAuth,
    now: DateTime<Utc>,
) -> Result<ProviderSnapshot, DevinProbeError> {
    let response = client::fetch_user_status(auth.api_key.as_str(), &auth.api_server_url).await?;
    response.error_for_status(now)?;
    mapper::map(&response.json_value()?, now)
}

#[cfg(test)]
mod tests {
    use super::{mapper::DevinProbeError, DevinProbe, PROVIDER_ID};
    use crate::usage::model::{
        Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError,
    };
    use crate::usage::{UsageProbe, UsageRegistry};
    use chrono::{TimeZone, Utc};

    #[test]
    fn the_probe_reports_the_registry_id() {
        assert_eq!(DevinProbe::new().id(), PROVIDER_ID);
        assert_eq!(PROVIDER_ID, "devin");
    }

    #[test]
    fn the_builtin_registry_ships_the_probe() {
        assert!(UsageRegistry::with_builtin_probes().contains(PROVIDER_ID));
    }

    #[test]
    fn a_machine_with_no_credential_file_reports_sign_in_rather_than_a_blank_row() {
        let now = Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap();
        let snapshot = ProviderSnapshot::from_failure(
            PROVIDER_ID,
            &DevinProbeError::Usage(UsageError::NotLoggedIn),
            now,
        );
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::NotLoggedIn));
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn a_rejected_key_is_reported_and_never_refreshed() {
        // The read-only decision in one assertion: 401/403 becomes
        // `session_expired`, which the UI turns into "sign in again".
        assert_eq!(
            DevinProbeError::Usage(UsageError::SessionExpired).classify(),
            (Availability::AuthRequired, ReasonCode::SessionExpired)
        );
    }

    #[test]
    fn a_broken_credential_store_is_not_reported_as_a_logout() {
        assert_eq!(
            DevinProbeError::Usage(UsageError::CredentialStoreUnavailable).classify(),
            (
                Availability::Unknown,
                ReasonCode::CredentialStoreUnavailable
            )
        );
    }
}
