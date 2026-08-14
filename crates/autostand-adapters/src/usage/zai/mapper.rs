//! Pure mapping of Z.ai's quota payload into a [`ProviderSnapshot`].
//!
//! Z.ai returns a flat array of limit entries, each carrying its own window as a
//! `(unit, number)` pair rather than a name, so the meters are classified by
//! **window length**:
//!
//! ```json
//! { "code":200, "data": { "limits": [
//!     { "type":"TOKENS_LIMIT", "unit":3, "number":5, "percentage":17, "nextResetTime":1782724971179 },
//!     { "type":"TOKENS_LIMIT", "unit":6, "number":1, "percentage":3,  "nextResetTime":1783305486997 },
//!     { "type":"TIME_LIMIT",   "unit":5, "number":1, "usage":1000, "currentValue":0, "nextResetTime":1785292686976 }
//! ] }, "success":true }
//! ```
//!
//! `unit` is an enum: 3 hours, 4 days, 5 months, 6 weeks. A sub-daily window is
//! the session meter, a multi-day window the weekly one, and each meter carries
//! the *payload's* own period so its cadence tracks the plan instead of a
//! hardcoded assumption. An unrecognised unit is skipped rather than fatal, so a
//! future Z.ai window cannot hide the meters this build still understands.
//!
//! `TIME_LIMIT` is the monthly web-search/reader allowance, mapped as a raw
//! count. It deliberately carries **no percentage**: exhausting web searches
//! does not exhaust the plan, and a derived 0% here would make the fallback
//! chain skip a provider that can still render.

use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::http::HttpResponse;
use crate::usage::model::{
    Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError, UsageResource,
    UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// Token-quota entries, split into session and weekly by window length.
pub const TOKENS_LIMIT: &str = "TOKENS_LIMIT";
/// The monthly web-search / reader allowance.
pub const TIME_LIMIT: &str = "TIME_LIMIT";

/// Sub-daily token window.
pub const SESSION_RESOURCE_ID: &str = "session";
/// Multi-day token window.
pub const WEEKLY_RESOURCE_ID: &str = "weekly";
/// Monthly web-search / reader count.
pub const WEB_SEARCHES_RESOURCE_ID: &str = "web_searches";

/// One monthly web-search cycle. Only a fallback cadence for the count meter —
/// the token meters use the payload's own window.
pub const MONTHLY_PERIOD: Duration = Duration::from_secs(30 * 24 * 60 * 60);

/// Shown beside the provider name when the key works but nothing is metered.
pub const NO_CODING_PLAN_NOTICE: &str = "No active GLM Coding Plan";

/// Z.ai's failure modes: the shared ones, plus one of its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ZaiUsageError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// The key is valid but the account has no GLM Coding Plan: the quota
    /// endpoint answers 2xx with `success:false`. Nothing to meter — distinct
    /// from a malformed or rejected request.
    #[error("no coding plan")]
    NoCodingPlan,
}

impl ProbeFailure for ZaiUsageError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm.
        match self {
            Self::Usage(inner) => inner.classify(),
            Self::NoCodingPlan => (Availability::Unknown, ReasonCode::UsageNotSupported),
        }
    }
}

/// Build the snapshot from the quota outcome and the optional subscription body.
///
/// `quota` is `Err` only for a transport failure; any completed exchange arrives
/// as `Ok` whatever its status. `subscription` is best-effort — a failure there
/// costs the plan label and nothing else.
#[must_use]
pub fn map(
    quota: Result<&HttpResponse, UsageError>,
    subscription: Option<&HttpResponse>,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    match resources(quota, now) {
        Ok(resources) => ProviderSnapshot::ok(
            PROVIDER_ID,
            UsageSourceKind::ProviderReported,
            resources,
            now,
        )
        .with_plan(subscription.and_then(plan_name)),
        Err(error @ ZaiUsageError::NoCodingPlan) => {
            ProviderSnapshot::from_failure(PROVIDER_ID, &error, now)
                .with_notice(Some(NO_CODING_PLAN_NOTICE.to_string()))
        }
        Err(error) => ProviderSnapshot::from_failure(PROVIDER_ID, &error, now),
    }
}

/// The session, weekly and web-search meters.
pub fn resources(
    quota: Result<&HttpResponse, UsageError>,
    now: DateTime<Utc>,
) -> Result<Vec<UsageResource>, ZaiUsageError> {
    let response = quota.map_err(ZaiUsageError::Usage)?;
    response.error_for_status(now)?;
    let body = response.json_value()?;
    if is_no_coding_plan(&body) {
        return Err(ZaiUsageError::NoCodingPlan);
    }
    map_quota(&body)
}

/// True when a 2xx quota body is the "valid key, no GLM Coding Plan" signal.
///
/// Matched on the structured `success:false` *plus* the phrase the message
/// carries, so an unrelated business failure does not trip it.
#[must_use]
pub fn is_no_coding_plan(body: &Value) -> bool {
    if body.get("success").and_then(parse::boolean) != Some(false) {
        return false;
    }
    body.get("msg")
        .and_then(parse::text)
        .is_some_and(|message| message.to_lowercase().contains("coding plan"))
}

/// Map a parsed quota body. Pure over the payload, so the shape is testable.
pub fn map_quota(body: &Value) -> Result<Vec<UsageResource>, ZaiUsageError> {
    if !body.is_object() {
        return Err(UsageError::UnsupportedPayload.into());
    }
    // The array lives under `data.limits`; the legacy plugin also tolerated the
    // root being the container directly, so honour both.
    let container = match body.get("data") {
        Some(data) if data.is_object() => data,
        Some(_) => return Err(UsageError::UnsupportedPayload.into()),
        None => body,
    };
    let limits = container
        .get("limits")
        .and_then(Value::as_array)
        .ok_or(UsageError::UnsupportedPayload)?;
    if limits.is_empty() {
        // An explicit empty array is a valid "nothing metered" state.
        return Ok(Vec::new());
    }

    let mut resources = Vec::new();
    let mut saw_recognised = false;

    for entry in limits.iter().filter(|entry| is_type(entry, TOKENS_LIMIT)) {
        let Some(window) = token_window(entry)? else {
            continue;
        };
        saw_recognised = true;
        resources.push(percent_resource(entry, window)?);
    }
    if let Some(entry) = limits.iter().find(|entry| is_type(entry, TIME_LIMIT)) {
        saw_recognised = true;
        resources.push(web_search_resource(entry)?);
    }

    if resources.is_empty() && saw_recognised {
        return Err(UsageError::UnsupportedPayload.into());
    }
    Ok(resources)
}

/// `productName` from the first subscription entry, e.g. `"GLM Coding Max"`.
#[must_use]
pub fn plan_name(response: &HttpResponse) -> Option<String> {
    if !response.is_success() {
        return None;
    }
    let body: Value = response.json_value().ok()?;
    let raw = body.get("data")?.as_array()?.first()?.get("productName")?;
    parse::text(raw).map(str::to_string)
}

/// How a token window maps to a meter, carrying the payload's own period.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TokenWindow {
    Session { period_ms: i64 },
    Weekly { period_ms: i64 },
}

impl TokenWindow {
    fn id(self) -> &'static str {
        match self {
            Self::Session { .. } => SESSION_RESOURCE_ID,
            Self::Weekly { .. } => WEEKLY_RESOURCE_ID,
        }
    }

    fn period_ms(self) -> i64 {
        match self {
            Self::Session { period_ms } | Self::Weekly { period_ms } => period_ms,
        }
    }
}

/// A limit entry matches by `type` or `name`: Z.ai's payload has used either
/// field across revisions.
fn is_type(entry: &Value, wanted: &str) -> bool {
    ["type", "name"]
        .iter()
        .filter_map(|key| entry.get(key).and_then(parse::text))
        .any(|found| found == wanted)
}

/// One hour, in milliseconds.
const HOUR_MS: f64 = 60.0 * 60.0 * 1000.0;
/// One day, in milliseconds — also the session/weekly boundary.
const DAY_MS: f64 = 24.0 * HOUR_MS;
/// Longest window this build will represent: comfortably inside `i64`
/// milliseconds, and about 285 000 years, so no real billing period reaches it.
const MAX_PERIOD_MS: f64 = 9.0e18;

/// Classify a token entry's window.
///
/// `Ok(None)` for a unit this build does not model — skipping it keeps the
/// meters we *do* understand visible. A missing or non-positive `number` is a
/// payload change, because the window length is not optional.
fn token_window(entry: &Value) -> Result<Option<TokenWindow>, ZaiUsageError> {
    let unit = entry
        .get("unit")
        .and_then(parse::number)
        .ok_or(UsageError::UnsupportedPayload)?;
    let count = entry
        .get("number")
        .and_then(parse::number)
        .filter(|value| *value > 0.0)
        .ok_or(UsageError::UnsupportedPayload)?;

    // `unit` is an enum, so it is compared as a whole number; the truncation is
    // the point, and any value outside the modelled set falls through to `None`.
    #[allow(clippy::cast_possible_truncation)]
    let unit_ms = match unit.trunc() as i64 {
        3 => HOUR_MS,
        4 => DAY_MS,
        5 => 30.0 * DAY_MS,
        6 => 7.0 * DAY_MS,
        _ => return Ok(None),
    };

    let duration_ms = unit_ms * count;
    if !duration_ms.is_finite() || !(1.0..MAX_PERIOD_MS).contains(&duration_ms) {
        return Err(UsageError::UnsupportedPayload.into());
    }
    // Sub-daily is the session meter, multi-day the weekly one — the comparison
    // stays in `f64` so no cast can change which side of the boundary it lands.
    let session = duration_ms < DAY_MS;
    #[allow(clippy::cast_possible_truncation)]
    let period_ms = duration_ms.trunc() as i64;

    Ok(Some(if session {
        TokenWindow::Session { period_ms }
    } else {
        TokenWindow::Weekly { period_ms }
    }))
}

/// A percentage meter from a token entry.
fn percent_resource(entry: &Value, window: TokenWindow) -> Result<UsageResource, ZaiUsageError> {
    let percentage = entry
        .get("percentage")
        .and_then(parse::number)
        .ok_or(UsageError::UnsupportedPayload)?;
    Ok(
        UsageResource::percent(window.id(), parse::clamp_percent(percentage))
            .with_resets_at(next_reset(entry))
            .with_period_ms(Some(window.period_ms())),
    )
}

/// The monthly web-search / reader count meter.
fn web_search_resource(entry: &Value) -> Result<UsageResource, ZaiUsageError> {
    let used = entry
        .get("currentValue")
        .and_then(parse::number)
        .filter(|value| *value >= 0.0)
        .ok_or(UsageError::UnsupportedPayload)?;
    let limit = entry
        .get("usage")
        .and_then(parse::number)
        .filter(|value| *value >= 0.0)
        .ok_or(UsageError::UnsupportedPayload)?;
    Ok(
        UsageResource::consumption(WEB_SEARCHES_RESOURCE_ID, UsageUnit::Count)
            .with_used(Some(used))
            .with_limit(Some(limit))
            .with_resets_at(next_reset(entry))
            .with_period(Some(MONTHLY_PERIOD)),
    )
}

/// `nextResetTime` arrives as epoch milliseconds.
fn next_reset(entry: &Value) -> Option<DateTime<Utc>> {
    entry
        .get("nextResetTime")
        .and_then(parse::number)
        .and_then(parse::parse_epoch)
}

#[cfg(test)]
mod tests {
    use super::{is_no_coding_plan, map, map_quota, plan_name, resources, ZaiUsageError};
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{Availability, ReasonCode, UsageError, UsageUnit};
    use chrono::{DateTime, TimeZone, Utc};

    /// Captured from a GLM Coding Pro plan, anonymised: the key, customer id and
    /// agreement number are gone; only the fields the mapper reads remain.
    const LIVE_QUOTA: &str = r#"{"code":200,"msg":"Operation successful","data":{"limits":[
      {"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":17,"nextResetTime":1782724971179},
      {"type":"TOKENS_LIMIT","unit":6,"number":1,"percentage":3,"nextResetTime":1783305486997},
      {"type":"TIME_LIMIT","unit":5,"number":1,"usage":1000,"currentValue":0,"remaining":1000,"percentage":0,"nextResetTime":1785292686976,"usageDetails":[{"modelCode":"search-prime","usage":0}]}
    ],"level":"pro"},"success":true}"#;

    const LIVE_SUBSCRIPTION: &str = r#"{"code":200,"msg":"Operation successful","data":[{"productName":"GLM Coding Pro","status":"VALID","nextRenewTime":"2026-07-29","billingCycle":"monthly","inCurrentPeriod":true}],"success":true}"#;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    fn ok(body: &str) -> HttpResponse {
        HttpResponse::from_parts(200, &[], body.as_bytes())
    }

    fn quota(body: &str) -> Vec<crate::usage::model::UsageResource> {
        map_quota(&serde_json::from_str(body).unwrap()).unwrap()
    }

    #[test]
    fn the_live_response_maps_to_session_weekly_and_web_searches() {
        let subscription = ok(LIVE_SUBSCRIPTION);
        let snapshot = map(Ok(&ok(LIVE_QUOTA)), Some(&subscription), now());

        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.plan.as_deref(), Some("GLM Coding Pro"));

        let ids: Vec<_> = snapshot.resources.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["session", "weekly", "web_searches"]);

        let session = &snapshot.resources[0];
        assert_eq!(session.used_percent, Some(17.0));
        // Five hours, taken from the entry's own (unit, number) pair.
        assert_eq!(session.period_duration_ms, Some(5 * 60 * 60 * 1000));
        assert_eq!(
            session.resets_at.map(|at| at.timestamp_millis()),
            Some(1_782_724_971_179)
        );

        let weekly = &snapshot.resources[1];
        assert_eq!(weekly.used_percent, Some(3.0));
        assert_eq!(weekly.period_duration_ms, Some(7 * 24 * 60 * 60 * 1000));

        let web = &snapshot.resources[2];
        assert_eq!(web.unit, UsageUnit::Count);
        assert_eq!(web.used, Some(0.0));
        assert_eq!(web.limit, Some(1000.0));
        // No percentage: spent web searches must not read as an exhausted plan.
        assert_eq!(web.used_percent, None);
        assert_eq!(web.remaining_percent, None);
    }

    #[test]
    fn spent_web_searches_never_grade_the_provider_as_exhausted() {
        let body = r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":10},{"type":"TIME_LIMIT","usage":1000,"currentValue":1000}]}}"#;
        let snapshot = map(Ok(&ok(body)), None, now()).graded(20.0);
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.tightest_remaining_percent(), Some(90.0));
    }

    #[test]
    fn a_missing_required_value_never_becomes_zero_usage() {
        let malformed = [
            r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5}]}}"#,
            r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":true}]}}"#,
            r#"{"data":{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":0,"percentage":5}]}}"#,
            r#"{"data":{"limits":[{"type":"TIME_LIMIT","usage":1000}]}}"#,
            r#"{"data":{"limits":[{"type":"TIME_LIMIT","currentValue":10}]}}"#,
            r#"{"data":{"limits":[{"type":"TIME_LIMIT","currentValue":-1,"usage":1000}]}}"#,
        ];
        for body in malformed {
            let error = map_quota(&serde_json::from_str(body).unwrap()).unwrap_err();
            assert_eq!(
                error,
                ZaiUsageError::Usage(UsageError::UnsupportedPayload),
                "{body}"
            );
        }
    }

    #[test]
    fn a_malformed_envelope_is_rejected_but_an_empty_array_stays_valid() {
        for body in [
            r#"{"data":[]}"#,
            r#"{"data":{}}"#,
            r#"{"data":{"limits":{}}}"#,
        ] {
            let error = map_quota(&serde_json::from_str(body).unwrap()).unwrap_err();
            assert_eq!(
                error,
                ZaiUsageError::Usage(UsageError::UnsupportedPayload),
                "{body}"
            );
        }
        // An explicit empty array is "nothing metered", not a failure.
        assert!(quota(r#"{"data":{"limits":[]}}"#).is_empty());
    }

    #[test]
    fn the_root_may_be_the_container_directly() {
        let resources =
            quota(r#"{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":25}]}"#);
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "session");
    }

    #[test]
    fn an_entry_may_name_its_type_under_either_key() {
        let resources =
            quota(r#"{"limits":[{"name":"TOKENS_LIMIT","unit":6,"number":1,"percentage":40}]}"#);
        assert_eq!(resources[0].id, "weekly");
    }

    #[test]
    fn an_unknown_window_is_skipped_rather_than_hiding_the_meters_we_know() {
        let resources = quota(
            r#"{"data":{"limits":[{"type":"FUTURE_LIMIT"},{"type":"TOKENS_LIMIT","unit":99,"number":1,"percentage":70},{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":25}]}}"#,
        );
        let ids: Vec<_> = resources.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["session"]);
    }

    #[test]
    fn only_unknown_windows_stays_forward_compatible_no_data() {
        let resources = quota(
            r#"{"data":{"limits":[{"type":"FUTURE_LIMIT"},{"type":"TOKENS_LIMIT","unit":99,"number":1,"percentage":70}]}}"#,
        );
        assert!(resources.is_empty());
    }

    #[test]
    fn a_daily_token_window_is_weekly_and_an_hourly_one_is_a_session() {
        let session =
            quota(r#"{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":1,"percentage":5}]}"#);
        assert_eq!(session[0].id, "session");
        assert_eq!(session[0].period_duration_ms, Some(60 * 60 * 1000));

        let daily =
            quota(r#"{"limits":[{"type":"TOKENS_LIMIT","unit":4,"number":1,"percentage":5}]}"#);
        assert_eq!(daily[0].id, "weekly");
        assert_eq!(daily[0].period_duration_ms, Some(24 * 60 * 60 * 1000));
    }

    #[test]
    fn percentages_are_clamped_into_range() {
        let resources =
            quota(r#"{"limits":[{"type":"TOKENS_LIMIT","unit":3,"number":5,"percentage":150}]}"#);
        assert_eq!(resources[0].used_percent, Some(100.0));
    }

    #[test]
    fn a_valid_key_without_a_coding_plan_says_so() {
        let body =
            r#"{"success":false,"code":500,"msg":"You have not subscribed to a coding plan"}"#;
        assert!(is_no_coding_plan(&serde_json::from_str(body).unwrap()));

        let snapshot = map(Ok(&ok(body)), None, now());
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UsageNotSupported));
        assert_eq!(
            snapshot.notice.as_deref(),
            Some(super::NO_CODING_PLAN_NOTICE)
        );
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn an_unrelated_business_failure_is_not_read_as_a_missing_plan() {
        let body = r#"{"success":false,"code":500,"msg":"internal error"}"#;
        assert!(!is_no_coding_plan(&serde_json::from_str(body).unwrap()));
        assert!(!is_no_coding_plan(
            &serde_json::from_str(LIVE_QUOTA).unwrap()
        ));
    }

    #[test]
    fn a_rejected_key_reports_an_expiry_and_is_never_refreshed() {
        for status in [401, 403] {
            let snapshot = map(
                Ok(&HttpResponse::from_parts(status, &[], b"{}")),
                None,
                now(),
            );
            assert_eq!(snapshot.availability, Availability::AuthRequired);
            assert_eq!(snapshot.reason, Some(ReasonCode::SessionExpired));
        }
    }

    #[test]
    fn being_offline_is_never_reported_as_exhaustion() {
        let snapshot = map(Err(UsageError::Network), None, now());
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::Network));
    }

    #[test]
    fn a_failed_subscription_call_costs_the_label_and_nothing_else() {
        let failed = HttpResponse::from_parts(500, &[], b"");
        assert_eq!(plan_name(&failed), None);
        let snapshot = map(Ok(&ok(LIVE_QUOTA)), Some(&failed), now());
        assert_eq!(snapshot.plan, None);
        assert_eq!(snapshot.resources.len(), 3);
    }

    #[test]
    fn a_subscription_body_without_a_product_name_yields_no_label() {
        assert_eq!(plan_name(&ok(r#"{"data":[]}"#)), None);
        assert_eq!(plan_name(&ok(r#"{"data":[{"productName":"  "}]}"#)), None);
        assert_eq!(plan_name(&ok("{}")), None);
    }

    #[test]
    fn nothing_from_the_response_leaks_into_the_snapshot() {
        let response =
            HttpResponse::from_parts(500, &[("x-trace", "secret-trace")], b"secret-body");
        let snapshot = map(Ok(&response), None, now());
        let rendered = format!("{snapshot:?}");
        assert!(!rendered.contains("secret-body"), "{rendered}");
        assert!(!rendered.contains("secret-trace"), "{rendered}");
    }

    #[test]
    fn a_transport_failure_short_circuits_before_any_parsing() {
        assert_eq!(
            resources(Err(UsageError::Timeout), now()).unwrap_err(),
            ZaiUsageError::Usage(UsageError::Timeout)
        );
    }
}
