//! GitHub Copilot usage probe: per-seat quota, with organization billing as the
//! fallback for a seat an organization manages.
//!
//! Three pieces, as every provider here is:
//!
//! - [`auth`] — read-only token discovery across the editor config, the GitHub
//!   CLI config and the CLI keychain item.
//! - [`client`] — the seat request and the two org-billing requests.
//! - [`mapper`] — pure `(payload, now) -> resources`.
//!
//! **There is no refresh path.** GitHub tokens on this machine belong to other
//! tools, and rotating one would log the user out of them. A rejected token
//! moves the probe to the next source; when none is left the provider reports
//! `session_expired` and the user runs `gh auth login` (or signs in again in the
//! editor). That is the read-only decision, applied literally.

pub mod auth;
pub mod client;
pub mod mapper;

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Utc};

use super::model::{ProviderSnapshot, UsageError, UsageResource, UsageSourceKind};
use super::{ProbeContext, UsageProbe};
use auth::CopilotToken;
use mapper::{CopilotProbeError, CopilotSeat};

/// Stable provider id, matching the IPC vocabulary.
pub const PROVIDER_ID: &str = "copilot";

/// Ceiling on how many organizations one refresh will ask about.
///
/// Discovery only runs for an org-managed seat, and a user in dozens of
/// organizations should not turn one refresh into dozens of billing requests.
const MAX_ORGS_PROBED: usize = 10;

/// Reads Copilot quota from a GitHub token this machine already holds.
#[derive(Debug, Default, Clone, Copy)]
pub struct CopilotProbe;

impl CopilotProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for CopilotProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_local_credentials().await
    }

    async fn probe(&self, ctx: &ProbeContext) -> ProviderSnapshot {
        // The clock is read once at this I/O boundary and injected downwards;
        // the mappers never read it.
        let now = Utc::now();
        // A keychain secret is read only when the user asked for this refresh.
        let tokens = match auth::load_tokens(ctx.keychain_access()).await {
            Ok(tokens) => tokens,
            Err(error) => {
                return ProviderSnapshot::from_failure(
                    PROVIDER_ID,
                    &CopilotProbeError::Usage(error),
                    now,
                )
            }
        };
        if tokens.is_empty() {
            return failure(&UsageError::NotLoggedIn, now);
        }

        let mut saw_rejected_token = false;
        let mut last_error = None;
        for token in &tokens {
            match seat(token, now).await {
                Ok(seat) => return finish(seat, token, now).await,
                Err(CopilotProbeError::Usage(UsageError::SessionExpired)) => {
                    // This source's token is not the one Copilot accepts; the
                    // next source may hold one that is. Nothing is rotated.
                    saw_rejected_token = true;
                }
                // A payload-level failure is the same for every token, so trying
                // another would only spend the user's rate limit.
                Err(error @ CopilotProbeError::QuotaUnavailable) => {
                    return ProviderSnapshot::from_failure(PROVIDER_ID, &error, now)
                }
                Err(error) => last_error = Some(error),
            }
        }

        let error = if saw_rejected_token {
            CopilotProbeError::Usage(UsageError::SessionExpired)
        } else {
            last_error.unwrap_or(CopilotProbeError::Usage(UsageError::NotLoggedIn))
        };
        ProviderSnapshot::from_failure(PROVIDER_ID, &error, now)
    }
}

fn failure(error: &UsageError, now: DateTime<Utc>) -> ProviderSnapshot {
    ProviderSnapshot::from_failure(PROVIDER_ID, &CopilotProbeError::Usage(*error), now)
}

/// One seat request, mapped.
async fn seat(token: &CopilotToken, now: DateTime<Utc>) -> Result<CopilotSeat, CopilotProbeError> {
    let response = client::fetch_usage(token.value.as_str()).await?;
    response.error_for_status(now)?;
    mapper::map(&response.json_value()?, now)
}

/// Complete a mapped seat, looking at organization billing when the seat itself
/// carries no meters.
async fn finish(seat: CopilotSeat, token: &CopilotToken, now: DateTime<Utc>) -> ProviderSnapshot {
    if !seat.org_managed {
        return seat.snapshot;
    }
    // Best effort: an org owner or billing manager sees org meters, everyone
    // else keeps the plan-only card. A 403 here is the expected answer for a
    // plain member, not a failure worth reddening the row for.
    match org_resources(token.value.as_str()).await {
        Some(resources) => {
            ProviderSnapshot::ok(PROVIDER_ID, UsageSourceKind::ManagementApi, resources, now)
                .with_plan(seat.snapshot.plan.clone())
        }
        None => seat.snapshot,
    }
}

/// What one organization's billing summary told us.
enum OrgOutcome {
    /// Copilot credit usage, ready to show.
    Found(Vec<UsageResource>),
    /// This organization definitively has nothing to show: no access, or no
    /// Copilot credits in the summary. Keep looking.
    Empty,
    /// A brief outage (429/5xx/transport). Not evidence the organization is
    /// wrong, so a remembered one survives it.
    Transient,
}

/// Organization-level meters for an org-managed seat.
///
/// The organization that answered is remembered for this process run, so a
/// steady-state refresh makes one billing call instead of re-probing every
/// organization. Only a slug is remembered — never a token.
async fn org_resources(token: &str) -> Option<Vec<UsageResource>> {
    if let Some(remembered) = remembered_org() {
        match org_summary(&remembered, token).await {
            OrgOutcome::Found(resources) => return Some(resources),
            // It answered, but no longer shows Copilot usage: the user left the
            // org or lost the billing role. Forget it and re-probe.
            OrgOutcome::Empty => forget_org(),
            OrgOutcome::Transient => return None,
        }
    }

    let response = client::fetch_user_orgs(token).await.ok()?;
    if !response.is_success() {
        // 403 means the token lacks `read:org`, which editor plugin tokens
        // routinely do. Expected, not an error.
        return None;
    }
    let logins = mapper::org_logins(&response.json_value().ok()?);
    for login in logins.iter().take(MAX_ORGS_PROBED) {
        // One organization's outage must not hide another's usage, so a
        // transient failure keeps the search going.
        if let OrgOutcome::Found(resources) = org_summary(login, token).await {
            remember_org(login);
            return Some(resources);
        }
    }
    None
}

async fn org_summary(org: &str, token: &str) -> OrgOutcome {
    let Ok(response) = client::fetch_org_usage_summary(org, token).await else {
        return OrgOutcome::Transient;
    };
    if !response.is_success() {
        return if response.status() == 429 || response.status() >= 500 {
            OrgOutcome::Transient
        } else {
            OrgOutcome::Empty
        };
    }
    let Ok(body) = response.json_value() else {
        return OrgOutcome::Empty;
    };
    match mapper::org_billing_resources(&body) {
        Some(resources) => OrgOutcome::Found(resources),
        None => OrgOutcome::Empty,
    }
}

/// The organization whose billing answered last, for this process run only.
///
/// Deliberately in memory rather than on disk: it is a cache-avoidance hint, and
/// a fresh launch re-discovering one organization costs a single request.
fn org_memo() -> &'static Mutex<Option<String>> {
    static MEMO: Mutex<Option<String>> = Mutex::new(None);
    &MEMO
}

fn remembered_org() -> Option<String> {
    org_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
}

fn remember_org(org: &str) {
    *org_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(org.to_string());
}

fn forget_org() {
    *org_memo()
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner) = None;
}

#[cfg(test)]
mod tests {
    use super::{failure, mapper::CopilotProbeError, CopilotProbe, PROVIDER_ID};
    use crate::usage::model::{Availability, ProbeFailure, ReasonCode, UsageError};
    use crate::usage::{UsageProbe, UsageRegistry};
    use chrono::{TimeZone, Utc};

    #[test]
    fn the_probe_reports_the_registry_id() {
        assert_eq!(CopilotProbe::new().id(), PROVIDER_ID);
        assert_eq!(PROVIDER_ID, "copilot");
    }

    #[test]
    fn the_builtin_registry_ships_the_probe() {
        assert!(UsageRegistry::with_builtin_probes().contains(PROVIDER_ID));
    }

    #[test]
    fn a_machine_with_no_github_token_reports_sign_in() {
        let now = Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap();
        let snapshot = failure(&UsageError::NotLoggedIn, now);
        assert_eq!(snapshot.provider, PROVIDER_ID);
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::NotLoggedIn));
    }

    #[test]
    fn a_rejected_token_is_reported_and_never_rotated() {
        // Copilot has no refresh path: the tokens belong to other tools, and
        // rotating one would sign the user out of them.
        assert_eq!(
            CopilotProbeError::Usage(UsageError::SessionExpired).classify(),
            (Availability::AuthRequired, ReasonCode::SessionExpired)
        );
    }

    #[test]
    fn org_memoisation_round_trips_and_can_be_forgotten() {
        super::remember_org("acme");
        assert_eq!(super::remembered_org().as_deref(), Some("acme"));
        super::forget_org();
        assert_eq!(super::remembered_org(), None);
    }
}
