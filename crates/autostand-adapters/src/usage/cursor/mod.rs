//! Cursor usage probe: editor session, dashboard usage, and the request-metered
//! fallback for team and enterprise plans.
//!
//! Three pieces, as every provider here is:
//!
//! - [`auth`] — read-only session discovery (the editor's `state.vscdb`, then
//!   the keychain).
//! - [`client`] — one required request plus three optional enrichments.
//! - [`mapper`] — pure `(payloads, now) -> ProviderSnapshot`.
//!
//! **No token is refreshed.** `OpenUsage` rotates Cursor's access token and
//! writes it back into Cursor's own database; autostand reports an expired token
//! as `session_expired` instead, and the user signs in through Cursor.
//!
//! # Known limitation, inherited from the vendor
//!
//! Cursor exposes per-model consumption only as a **row-aggregated CSV export**
//! from the dashboard. There is no structured endpoint for it, so nothing here
//! can express a per-model or long-context threshold: the meters below are
//! plan-wide. That is a limit of what Cursor publishes, not a simplification
//! made here, and it will not change until Cursor ships a structured endpoint.

pub mod auth;
pub mod client;
pub mod mapper;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;

use super::http::HttpResponse;
use super::model::{ProviderSnapshot, UsageError};
use super::{ProbeContext, UsageProbe};
use auth::CursorSession;
use mapper::{CursorPayloads, CursorProbeError, PlanUsageFacts};

/// Stable provider id, matching the IPC vocabulary.
pub const PROVIDER_ID: &str = "cursor";

/// Reads Cursor's quota from the session the editor already holds.
#[derive(Debug, Default, Clone, Copy)]
pub struct CursorProbe;

impl CursorProbe {
    #[must_use]
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl UsageProbe for CursorProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_local_credentials().await
    }

    async fn probe(&self, ctx: &ProbeContext) -> ProviderSnapshot {
        // The clock is read once at this I/O boundary and injected downwards;
        // the mapper never reads it.
        let now = Utc::now();
        // A keychain secret is read only when the user asked for this refresh.
        let credential = match auth::load(ctx.keychain_access()).await {
            Ok(Some(credential)) => credential,
            Ok(None) => return failure(&UsageError::NotLoggedIn, now),
            Err(error) => return failure(&error, now),
        };
        let token = credential.access_token.as_str();
        // Read-only: an expired token is stated, never exchanged for a new one.
        if auth::is_expired(token, now) {
            return failure(&UsageError::SessionExpired, now);
        }

        match gather(token, now).await {
            Ok(snapshot) => snapshot,
            Err(error) => ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
        }
    }
}

fn failure(error: &UsageError, now: DateTime<Utc>) -> ProviderSnapshot {
    ProviderSnapshot::from_failure(PROVIDER_ID, &CursorProbeError::Usage(*error), now)
}

/// The one required request, then whichever enrichments this account shape needs.
async fn gather(token: &str, now: DateTime<Utc>) -> Result<ProviderSnapshot, CursorProbeError> {
    let response = client::fetch_usage(token).await?;
    response.error_for_status(now)?;
    let usage = response.json_value()?;

    let plan = optional_json(client::fetch_plan(token).await)
        .as_ref()
        .and_then(|body| mapper::plan_name(body).map(str::to_string));
    // A plan lookup that failed is itself a signal: an enterprise account often
    // answers nothing here, and that is one of the request-fallback conditions.
    let plan_unavailable = plan.is_none();
    let session = auth::session(token);

    if mapper::should_use_request_fallback(&usage, plan.as_deref(), plan_unavailable) {
        return request_metered(session.as_ref(), plan.as_deref(), now)
            .await
            .ok_or(CursorProbeError::UsageLimitMissing);
    }
    // An enabled plan with neither an allowance nor a percentage may still be
    // metered in requests. Try, but never fail the refresh over it: the spend
    // mapping below is still the better answer when it works.
    if PlanUsageFacts::read(&usage).should_try_request_endpoint() {
        if let Some(snapshot) = request_metered(session.as_ref(), plan.as_deref(), now).await {
            return Ok(snapshot);
        }
    }

    let grants = optional_json(client::fetch_credit_grants(token).await);
    let stripe_balance_cents = prepaid_balance_cents(session.as_ref()).await;
    mapper::map(
        &CursorPayloads {
            usage: &usage,
            plan_name: plan.as_deref(),
            credit_grants: grants.as_ref(),
            stripe_balance_cents,
        },
        now,
    )
}

async fn request_metered(
    session: Option<&CursorSession>,
    plan: Option<&str>,
    now: DateTime<Utc>,
) -> Option<ProviderSnapshot> {
    let body = optional_json(client::fetch_request_based_usage(session?).await)?;
    mapper::map_request_based(&body, plan, now).ok()
}

/// The prepaid balance, or `0` when it cannot be read.
///
/// Zero is the right default here and is not a fabricated meter: it feeds the
/// credit *sum*, and a pool that adds up to nothing produces no resource at all
/// rather than a `$0` row.
async fn prepaid_balance_cents(session: Option<&CursorSession>) -> f64 {
    let Some(session) = session else {
        return 0.0;
    };
    optional_json(client::fetch_stripe_balance(session).await)
        .as_ref()
        .map_or(0.0, mapper::stripe_balance_cents)
}

/// An optional endpoint's body, or `None` for any failure.
///
/// Optional endpoints enrich a usable snapshot; none of them may fail the
/// refresh. Nothing is logged here — a status or a body would breach the rule
/// that no response detail leaves this module.
fn optional_json(result: Result<HttpResponse, UsageError>) -> Option<Value> {
    let response = result.ok()?;
    if !response.is_success() {
        return None;
    }
    response.json_value().ok()
}

#[cfg(test)]
mod tests {
    use super::{failure, optional_json, CursorProbe, PROVIDER_ID};
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{Availability, ReasonCode, UsageError};
    use crate::usage::{UsageProbe, UsageRegistry};
    use chrono::{TimeZone, Utc};

    #[test]
    fn the_probe_reports_the_registry_id() {
        assert_eq!(CursorProbe::new().id(), PROVIDER_ID);
        assert_eq!(PROVIDER_ID, "cursor");
    }

    #[test]
    fn the_builtin_registry_ships_the_probe() {
        assert!(UsageRegistry::with_builtin_probes().contains(PROVIDER_ID));
    }

    #[test]
    fn an_expired_session_is_reported_rather_than_refreshed() {
        let now = Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap();
        let snapshot = failure(&UsageError::SessionExpired, now);
        assert_eq!(snapshot.provider, PROVIDER_ID);
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::SessionExpired));
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn an_optional_endpoint_contributes_nothing_when_it_fails() {
        let ok = HttpResponse::from_parts(200, &[], br#"{"planInfo":{"planName":"pro"}}"#);
        assert!(optional_json(Ok(ok)).is_some());

        let forbidden = HttpResponse::from_parts(403, &[], b"{}");
        assert!(optional_json(Ok(forbidden)).is_none());

        let garbled = HttpResponse::from_parts(200, &[], b"<html>");
        assert!(optional_json(Ok(garbled)).is_none());

        assert!(optional_json(Err(UsageError::Network)).is_none());
    }
}
