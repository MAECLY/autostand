//! Pure mapping of Grok's credits config into a [`ProviderSnapshot`].
//!
//! The response is a proto3 message serialised as JSON, which has one property
//! that dominates this file: **zero-valued fields are omitted entirely**. An
//! absent `creditUsagePercent` therefore means a genuine 0%, not a schema
//! change — but a field that *is* present and non-numeric is drift, and must
//! degrade the provider rather than clamp to zero.
//!
//! Observed shape (captured 2026-07-06, matching what the Grok CLI logs as
//! "billing: fetched credits config"):
//!
//! ```json
//! { "config": {
//!     "creditUsagePercent": 99.0,
//!     "currentPeriod": { "type": "USAGE_PERIOD_TYPE_WEEKLY",
//!                        "start": "2026-07-03T04:01:09.238389+00:00",
//!                        "end":   "2026-07-10T04:01:09.238389+00:00" },
//!     "onDemandCap": { "val": 2500 },
//!     "isUnifiedBillingUser": true } }
//! ```
//!
//! No I/O and no clock: `now` is injected so a fixture test is exact.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::http::HttpResponse;
use crate::usage::model::{ProviderSnapshot, UsageError, UsageResource, UsageSourceKind};
use crate::usage::parse;

use super::PROVIDER_ID;

/// The shared weekly pool Grok migrated unified-billing users to.
pub const WEEKLY_PERIOD_TYPE: &str = "USAGE_PERIOD_TYPE_WEEKLY";

/// Resource id for the weekly shared pool.
pub const WEEKLY_RESOURCE_ID: &str = "weekly";

/// Build the snapshot from the two captured responses.
///
/// `settings` is optional and never fatal: the plan label is a nicety, the meter
/// is the point.
#[must_use]
pub fn map(
    credits: &HttpResponse,
    settings: Option<&HttpResponse>,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    match resources(credits, now) {
        Ok(resources) => ProviderSnapshot::ok(
            PROVIDER_ID,
            UsageSourceKind::ProviderReported,
            resources,
            now,
        )
        .with_plan(settings.and_then(plan_name)),
        Err(error) => ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
    }
}

/// The weekly meter, or an empty list when this account has no weekly pool.
///
/// An account still on the older monthly-only billing genuinely has no weekly
/// window; labelling its monthly percent "Weekly" would be worse than an honest
/// blank, so the row simply does not appear.
pub fn resources(
    response: &HttpResponse,
    now: DateTime<Utc>,
) -> Result<Vec<UsageResource>, UsageError> {
    response.error_for_status(now)?;
    let body = response.json_value()?;
    let config = body.get("config").ok_or(UsageError::UnsupportedPayload)?;
    let period = config
        .get("currentPeriod")
        .ok_or(UsageError::UnsupportedPayload)?;

    let period_type = period
        .get("type")
        .and_then(parse::text)
        .ok_or(UsageError::UnsupportedPayload)?;
    let start = period
        .get("start")
        .and_then(parse::text)
        .and_then(parse::parse_rfc3339)
        .ok_or(UsageError::UnsupportedPayload)?;
    let end = period
        .get("end")
        .and_then(parse::text)
        .and_then(parse::parse_rfc3339)
        .ok_or(UsageError::UnsupportedPayload)?;
    if end <= start {
        return Err(UsageError::UnsupportedPayload);
    }

    // proto-JSON drops zero values, so an *absent* percent is a real 0%. A
    // present-but-unreadable percent is drift and must not become one.
    let used_percent = match config.get("creditUsagePercent") {
        None => 0.0,
        Some(raw) => parse::number(raw).ok_or(UsageError::UnsupportedPayload)?,
    };

    if period_type != WEEKLY_PERIOD_TYPE {
        return Ok(Vec::new());
    }

    Ok(vec![UsageResource::percent(
        WEEKLY_RESOURCE_ID,
        parse::clamp_percent(used_percent),
    )
    .with_resets_at(Some(end))
    .with_period_ms(Some((end - start).num_milliseconds()))
    .derive_projection(now)])
}

/// The subscription tier's display name, e.g. `"SuperGrok Heavy"`.
///
/// `None` on any failure: an unreadable settings response costs a label, and a
/// label is never worth failing a refresh over.
#[must_use]
pub fn plan_name(response: &HttpResponse) -> Option<String> {
    if !response.is_success() {
        return None;
    }
    let body: Value = response.json_value().ok()?;
    let raw = body.get("subscription_tier_display")?;
    parse::text(raw).map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::{map, plan_name, resources, WEEKLY_RESOURCE_ID};
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{Availability, ReasonCode, UsageError, UsageSourceKind};
    use crate::usage::parse;
    use chrono::{DateTime, TimeZone, Utc};

    /// Captured live from `cli-chat-proxy.grok.com` (percent edited to a nonzero
    /// value; proto-JSON omits it at 0). Includes fields the mapper does not
    /// read, which is the point: unknown-field tolerance on a real payload.
    const CAPTURED: &[u8] = br#"{"config":{"creditUsagePercent":99.0,"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-06-30T21:36:52.140114+00:00","end":"2026-07-07T21:36:52.140114+00:00"},"onDemandCap":{"val":0},"onDemandUsed":{"val":0},"isUnifiedBillingUser":true,"prepaidBalance":{"val":0},"topUpMethod":"TOP_UP_METHOD_SAVED_PAYMENT_METHOD","billingPeriodStart":"2026-06-30T21:36:52.140114+00:00","billingPeriodEnd":"2026-07-07T21:36:52.140114+00:00"}}"#;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 3, 12, 0, 0).unwrap()
    }

    fn ok(body: &[u8]) -> HttpResponse {
        HttpResponse::from_parts(200, &[], body)
    }

    fn weekly_body(period_type: &str, percent: Option<&str>) -> Vec<u8> {
        let percent =
            percent.map_or_else(String::new, |raw| format!(r#""creditUsagePercent":{raw},"#));
        format!(
            r#"{{"config":{{{percent}"currentPeriod":{{"type":"{period_type}","start":"2026-06-30T21:36:52.140114+00:00","end":"2026-07-07T21:36:52.140114+00:00"}}}}}}"#
        )
        .into_bytes()
    }

    #[test]
    fn the_captured_response_maps_to_one_weekly_meter() {
        let snapshot = map(&ok(CAPTURED), None, now());
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.source, UsageSourceKind::ProviderReported);
        assert_eq!(snapshot.resources.len(), 1);

        let weekly = &snapshot.resources[0];
        assert_eq!(weekly.id, WEEKLY_RESOURCE_ID);
        assert_eq!(weekly.used_percent, Some(99.0));
        assert_eq!(weekly.remaining_percent, Some(1.0));
        assert_eq!(weekly.limit, Some(100.0));
        assert_eq!(
            weekly.resets_at,
            parse::parse_rfc3339("2026-07-07T21:36:52.140114+00:00")
        );
        // Seven days, taken from the payload's own period rather than assumed.
        assert_eq!(weekly.period_duration_ms, Some(7 * 24 * 60 * 60 * 1000));
    }

    #[test]
    fn an_omitted_percent_is_a_real_zero_not_missing_data() {
        // proto-JSON drops zero-valued fields; the meter must read 0%, not blank.
        let snapshot = map(
            &ok(&weekly_body("USAGE_PERIOD_TYPE_WEEKLY", None)),
            None,
            now(),
        );
        assert_eq!(snapshot.resources[0].used_percent, Some(0.0));
        assert_eq!(snapshot.resources[0].remaining_percent, Some(100.0));
    }

    #[test]
    fn a_present_but_unreadable_percent_degrades_instead_of_becoming_zero() {
        let snapshot = map(
            &ok(&weekly_body(
                "USAGE_PERIOD_TYPE_WEEKLY",
                Some(r#""not-a-number""#),
            )),
            None,
            now(),
        );
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UnsupportedPayload));
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn an_out_of_range_percent_is_clamped() {
        let snapshot = map(
            &ok(&weekly_body("USAGE_PERIOD_TYPE_WEEKLY", Some("140"))),
            None,
            now(),
        );
        assert_eq!(snapshot.resources[0].used_percent, Some(100.0));
    }

    #[test]
    fn a_monthly_only_account_gets_no_weekly_row_rather_than_a_mislabelled_one() {
        let snapshot = map(
            &ok(&weekly_body("USAGE_PERIOD_TYPE_MONTHLY", Some("40"))),
            None,
            now(),
        );
        assert_eq!(snapshot.availability, Availability::Available);
        assert!(snapshot.resources.is_empty());
        assert_eq!(snapshot.reason, None);
    }

    #[test]
    fn a_missing_or_broken_period_is_a_payload_change() {
        let cases: Vec<Vec<u8>> = vec![
            b"{}".to_vec(),
            br#"{"config":{}}"#.to_vec(),
            br#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY"}}}"#.to_vec(),
            // end before start: the window does not move forward.
            br#"{"config":{"currentPeriod":{"type":"USAGE_PERIOD_TYPE_WEEKLY","start":"2026-07-07T00:00:00Z","end":"2026-06-30T00:00:00Z"}}}"#.to_vec(),
            b"<html>nope</html>".to_vec(),
        ];
        for body in cases {
            let error = resources(&ok(&body), now()).unwrap_err();
            assert_eq!(error, UsageError::UnsupportedPayload);
        }
    }

    #[test]
    fn an_auth_failure_is_reported_never_refreshed() {
        for status in [401, 403] {
            let snapshot = map(&HttpResponse::from_parts(status, &[], b"{}"), None, now());
            assert_eq!(snapshot.availability, Availability::AuthRequired);
            assert_eq!(snapshot.reason, Some(ReasonCode::SessionExpired));
        }
    }

    #[test]
    fn a_429_carries_the_vendor_cooldown_through_to_the_snapshot() {
        let response = HttpResponse::from_parts(429, &[("Retry-After", "120")], b"{}");
        let snapshot = map(&response, None, now());
        assert_eq!(snapshot.availability, Availability::RateLimited);
        assert_eq!(snapshot.reason, Some(ReasonCode::RateLimited));
    }

    #[test]
    fn the_plan_label_comes_from_the_settings_response() {
        let settings = ok(br#"{"subscription_tier_display":"SuperGrok Heavy"}"#);
        assert_eq!(plan_name(&settings).as_deref(), Some("SuperGrok Heavy"));
        let snapshot = map(&ok(CAPTURED), Some(&settings), now());
        assert_eq!(snapshot.plan.as_deref(), Some("SuperGrok Heavy"));
    }

    #[test]
    fn a_failed_settings_call_costs_the_label_and_nothing_else() {
        let settings = HttpResponse::from_parts(500, &[], b"");
        assert_eq!(plan_name(&settings), None);
        let snapshot = map(&ok(CAPTURED), Some(&settings), now());
        assert_eq!(snapshot.plan, None);
        assert_eq!(snapshot.resources.len(), 1);
    }

    #[test]
    fn a_blank_plan_label_is_missing_not_empty() {
        assert_eq!(
            plan_name(&ok(br#"{"subscription_tier_display":"   "}"#)),
            None
        );
        assert_eq!(plan_name(&ok(b"{}")), None);
    }

    #[test]
    fn nothing_from_the_response_leaks_into_the_snapshot() {
        let response =
            HttpResponse::from_parts(500, &[("x-trace", "secret-trace")], b"secret-body");
        let snapshot = map(&response, None, now());
        let rendered = format!("{snapshot:?}");
        assert!(!rendered.contains("secret-body"), "{rendered}");
        assert!(!rendered.contains("secret-trace"), "{rendered}");
    }
}
