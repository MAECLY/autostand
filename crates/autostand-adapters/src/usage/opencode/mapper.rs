//! Pure mapping of `OpenCode`'s Go usage response into a [`ProviderSnapshot`].
//!
//! The endpoint reports *percent used* per window — the same numbers the
//! `OpenCode` dashboard shows — plus an ISO reset, so all three rows are
//! percentage meters:
//!
//! ```json
//! { "usage": {
//!     "rolling":  { "status": "ok", "percent": 12,  "resetsAt": "2026-07-12T13:30:00.662Z" },
//!     "weekly":   { "status": "ok", "percent": 8,   "resetsAt": "2026-07-13T00:00:00.662Z" },
//!     "monthly":  { "status": "rate-limited", "percent": 100, "resetsAt": "2026-08-04T11:18:32.662Z" } } }
//! ```
//!
//! A window whose `percent` is missing or unreadable fails the whole mapping:
//! three authoritative-looking meters built from two real values and one guess
//! would be worse than an honest "no data".

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::http::HttpResponse;
use crate::usage::model::{
    Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError, UsageResource,
    UsageSourceKind,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// Rolling session window.
pub const SESSION_PERIOD: Duration = Duration::from_secs(5 * 60 * 60);
/// Weekly window.
pub const WEEKLY_PERIOD: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// Monthly window, as `OpenCode` counts it.
pub const MONTHLY_PERIOD: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Plan label for an account with an active Go subscription.
pub const GO_PLAN: &str = "Go";

/// Shown beside the provider name when the key is valid but unmetered.
pub const NO_SUBSCRIPTION_NOTICE: &str = "No OpenCode Go subscription on this key";

/// The upstream error discriminator for "valid key, no Go entitlement".
pub const ENTITLEMENT_ERROR: &str = "EntitlementError";

/// `OpenCode`'s failure modes: the shared ones, plus one of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum OpenCodeUsageError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// The key authenticates, but this account has no Go subscription — there is
    /// simply nothing to meter. Distinct from a rejected key.
    #[error("no go subscription")]
    NoGoSubscription,
}

impl ProbeFailure for OpenCodeUsageError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm.
        match self {
            Self::Usage(inner) => inner.classify(),
            // Not a failure of ours and not exhaustion: this account has no
            // usage contract at all, so the row says so instead of reading empty.
            Self::NoGoSubscription => (Availability::Unknown, ReasonCode::UsageNotSupported),
        }
    }
}

/// Build the snapshot from the captured usage response.
#[must_use]
pub fn map(response: &HttpResponse, now: DateTime<Utc>) -> ProviderSnapshot {
    match resources(response, now) {
        Ok(resources) => ProviderSnapshot::ok(
            PROVIDER_ID,
            UsageSourceKind::ProviderReported,
            resources,
            now,
        )
        .with_plan(Some(GO_PLAN.to_string())),
        Err(error @ OpenCodeUsageError::NoGoSubscription) => {
            ProviderSnapshot::from_failure(PROVIDER_ID, &error, now)
                .with_notice(Some(NO_SUBSCRIPTION_NOTICE.to_string()))
        }
        Err(error) => ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
    }
}

/// The three Go plan meters.
pub fn resources(
    response: &HttpResponse,
    now: DateTime<Utc>,
) -> Result<Vec<UsageResource>, OpenCodeUsageError> {
    // A 403 whose body names the entitlement error is "no subscription", not a
    // rejected key — the difference is the whole message the user needs.
    if response.status() == 403 && error_type(response).as_deref() == Some(ENTITLEMENT_ERROR) {
        return Err(OpenCodeUsageError::NoGoSubscription);
    }
    response.error_for_status(now)?;

    let body = response.json_value()?;
    let usage = body.get("usage").ok_or(UsageError::UnsupportedPayload)?;

    Ok(vec![
        window(usage.get("rolling"), "session", SESSION_PERIOD, now)?,
        window(usage.get("weekly"), "weekly", WEEKLY_PERIOD, now)?,
        window(usage.get("monthly"), "monthly", MONTHLY_PERIOD, now)?,
    ])
}

/// The documented error discriminator (`AuthError`, `EntitlementError`, …).
///
/// `None` for an HTML, Cloudflare or empty body — those carry no discriminator
/// and must not be guessed at.
#[must_use]
pub fn error_type(response: &HttpResponse) -> Option<String> {
    let body: Value = response.json_value().ok()?;
    let raw = body.get("error")?.get("type")?;
    parse::text(raw).map(str::to_string)
}

/// One window. A missing or unreadable `percent` is a payload change, never 0%.
fn window(
    raw: Option<&Value>,
    id: &str,
    period: Duration,
    now: DateTime<Utc>,
) -> Result<UsageResource, OpenCodeUsageError> {
    let raw = raw.ok_or(UsageError::UnsupportedPayload)?;
    let percent = raw
        .get("percent")
        .and_then(parse::number)
        .ok_or(UsageError::UnsupportedPayload)?;
    let resets_at = raw
        .get("resetsAt")
        .and_then(parse::text)
        .and_then(parse::parse_rfc3339);
    Ok(UsageResource::percent(id, parse::clamp_percent(percent))
        .with_resets_at(resets_at)
        .with_period(Some(period))
        .derive_projection(now))
}

#[cfg(test)]
mod tests {
    use super::{error_type, map, resources, OpenCodeUsageError, GO_PLAN};
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{Availability, ReasonCode, UsageError};
    use crate::usage::parse;
    use chrono::{DateTime, TimeZone, Utc};

    /// The documented shape, as `OpenUsage` records it from the live endpoint.
    const SAMPLE: &[u8] = br#"{"usage":{"rolling":{"status":"ok","percent":12,"resetsAt":"2026-07-12T13:30:00.662Z"},"weekly":{"status":"ok","percent":8,"resetsAt":"2026-07-13T00:00:00.662Z"},"monthly":{"status":"rate-limited","percent":100,"resetsAt":"2026-08-04T11:18:32.662Z"}}}"#;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 12, 12, 0, 0).unwrap()
    }

    fn ok(body: &[u8]) -> HttpResponse {
        HttpResponse::from_parts(200, &[], body)
    }

    #[test]
    fn the_three_go_windows_map_in_order_with_their_periods() {
        let snapshot = map(&ok(SAMPLE), now());
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.plan.as_deref(), Some(GO_PLAN));

        let ids: Vec<_> = snapshot.resources.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["session", "weekly", "monthly"]);

        assert_eq!(snapshot.resources[0].used_percent, Some(12.0));
        assert_eq!(
            snapshot.resources[0].period_duration_ms,
            Some(5 * 60 * 60 * 1000)
        );
        assert_eq!(
            snapshot.resources[0].resets_at,
            parse::parse_rfc3339("2026-07-12T13:30:00.662Z")
        );

        assert_eq!(snapshot.resources[1].used_percent, Some(8.0));
        assert_eq!(
            snapshot.resources[1].period_duration_ms,
            Some(7 * 24 * 60 * 60 * 1000)
        );

        assert_eq!(snapshot.resources[2].used_percent, Some(100.0));
        assert_eq!(snapshot.resources[2].remaining_percent, Some(0.0));
        assert_eq!(
            snapshot.resources[2].period_duration_ms,
            Some(30 * 24 * 60 * 60 * 1000)
        );
    }

    #[test]
    fn a_measured_zero_is_a_meter_not_missing_data() {
        let body = br#"{"usage":{"rolling":{"percent":0},"weekly":{"percent":0},"monthly":{"percent":0}}}"#;
        let snapshot = map(&ok(body), now());
        assert_eq!(snapshot.resources[0].used_percent, Some(0.0));
        assert_eq!(snapshot.resources[0].limit, Some(100.0));
        assert_eq!(snapshot.resources[0].remaining_percent, Some(100.0));
    }

    #[test]
    fn percentages_are_clamped_into_range() {
        let body = br#"{"usage":{"rolling":{"percent":150},"weekly":{"percent":-4},"monthly":{"percent":35}}}"#;
        let snapshot = map(&ok(body), now());
        assert_eq!(snapshot.resources[0].used_percent, Some(100.0));
        assert_eq!(snapshot.resources[1].used_percent, Some(0.0));
    }

    #[test]
    fn a_window_without_a_percent_fails_instead_of_guessing() {
        let cases: Vec<&[u8]> = vec![
            b"{}",
            br#"{"usage":{}}"#,
            br#"{"usage":{"weekly":{"percent":1}}}"#,
            br#"{"usage":{"rolling":{},"weekly":{"percent":1},"monthly":{"percent":1}}}"#,
            br#"{"usage":{"rolling":{"percent":true},"weekly":{"percent":1},"monthly":{"percent":1}}}"#,
        ];
        for body in cases {
            let error = resources(&ok(body), now()).unwrap_err();
            assert_eq!(
                error,
                OpenCodeUsageError::Usage(UsageError::UnsupportedPayload)
            );
        }
    }

    #[test]
    fn a_missing_reset_leaves_the_countdown_blank_rather_than_inventing_one() {
        let body = br#"{"usage":{"rolling":{"percent":5},"weekly":{"percent":5},"monthly":{"percent":5}}}"#;
        let snapshot = map(&ok(body), now());
        assert_eq!(snapshot.resources[0].resets_at, None);
        assert_eq!(snapshot.resources[0].pace, None);
    }

    #[test]
    fn a_rejected_key_reports_an_expiry_and_is_never_refreshed() {
        let body = br#"{"type":"error","error":{"type":"AuthError","message":"Unauthorized"}}"#;
        let snapshot = map(&HttpResponse::from_parts(401, &[], body), now());
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::SessionExpired));
    }

    #[test]
    fn a_valid_key_without_a_go_plan_says_so_instead_of_reading_as_a_bad_key() {
        let body = br#"{"type":"error","error":{"type":"EntitlementError","message":"OpenCode Go subscription required."}}"#;
        let response = HttpResponse::from_parts(403, &[], body);
        assert_eq!(error_type(&response).as_deref(), Some("EntitlementError"));

        let snapshot = map(&response, now());
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UsageNotSupported));
        assert_eq!(
            snapshot.notice.as_deref(),
            Some(super::NO_SUBSCRIPTION_NOTICE)
        );
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn a_403_without_the_entitlement_discriminator_stays_an_auth_failure() {
        // A Cloudflare or HTML 403 says nothing about entitlement.
        let snapshot = map(
            &HttpResponse::from_parts(403, &[], b"<html>forbidden</html>"),
            now(),
        );
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::SessionExpired));
        assert_eq!(
            error_type(&HttpResponse::from_parts(403, &[], b"<html>")),
            None
        );
    }

    #[test]
    fn a_server_error_keeps_only_its_status() {
        let snapshot = map(
            &HttpResponse::from_parts(503, &[], b"upstream said no"),
            now(),
        );
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UnexpectedStatus));
        let rendered = format!("{snapshot:?}");
        assert!(!rendered.contains("upstream"), "{rendered}");
    }

    #[test]
    fn every_failure_variant_classifies_without_a_wildcard() {
        assert_eq!(
            OpenCodeUsageError::NoGoSubscription.classify_pair(),
            (Availability::Unknown, ReasonCode::UsageNotSupported)
        );
        assert_eq!(
            OpenCodeUsageError::Usage(UsageError::NotLoggedIn).classify_pair(),
            (Availability::AuthRequired, ReasonCode::NotLoggedIn)
        );
    }

    /// Small shim so the test reads as a pair rather than a trait call.
    trait ClassifyPair {
        fn classify_pair(&self) -> (Availability, ReasonCode);
    }
    impl ClassifyPair for OpenCodeUsageError {
        fn classify_pair(&self) -> (Availability, ReasonCode) {
            crate::usage::model::ProbeFailure::classify(self)
        }
    }
}
