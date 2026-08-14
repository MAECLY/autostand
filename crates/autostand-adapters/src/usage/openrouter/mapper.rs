//! Pure mapping of `OpenRouter`'s `/credits` and `/key` payloads into a
//! [`ProviderSnapshot`].
//!
//! Both endpoints wrap their payload in `{ "data": { … } }` and both are mapped
//! independently, because `OpenRouter` gates either one for particular key types:
//!
//! ```json
//! /credits → { "data": { "total_credits": 277.47, "total_usage": 178.20 } }
//! /key     → { "data": { "is_free_tier": false, "usage": 2, "limit": 5 } }
//! ```
//!
//! Dollar spend *tiles* (today / this week / this month) are out of scope here —
//! `docs/specs/provider-usage.md` tracks spend separately — so only the three
//! bounded resources are produced: `credits`, `balance` and `key_limit`.
//!
//! Percentages on the two dollar meters are **derived from two reported
//! numbers**, never invented: a key with no cap has no percentage at all, and a
//! payload with no `total_usage` produces no rows.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::http::HttpResponse;
use crate::usage::model::{
    ProviderSnapshot, ResourceKind, UsageError, UsageResource, UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// Spend against credits purchased.
pub const CREDITS_RESOURCE_ID: &str = "credits";
/// Prepaid credits remaining.
pub const BALANCE_RESOURCE_ID: &str = "balance";
/// This key's own spend cap, when one is configured.
pub const KEY_LIMIT_RESOURCE_ID: &str = "key_limit";

/// What one endpoint produced.
///
/// Three cases rather than `Result`, because "the key was rejected" has to stay
/// distinguishable from "the request failed" — a single 403 on a gated endpoint
/// is not evidence of a bad key.
enum Endpoint {
    Data(Value),
    AuthFailure,
    Failed(UsageError),
}

/// Build the snapshot from both captured responses.
///
/// Each argument is the outcome of one request: `Ok` for a completed exchange
/// (whatever its status), `Err` for a transport failure. Keeping the error in
/// the signature is what lets this stay pure while still classifying "offline"
/// differently from "rejected".
#[must_use]
pub fn map(
    credits: Result<&HttpResponse, UsageError>,
    key: Result<&HttpResponse, UsageError>,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    let credits = classify(credits, now);
    let key = classify(key, now);

    let mut resources = Vec::new();
    let mut plan = None;

    if let Endpoint::Data(data) = &credits {
        resources.extend(credit_resources(data));
    }
    if let Endpoint::Data(data) = &key {
        plan = plan_name(data);
        resources.extend(key_resources(data));
    }

    if !resources.is_empty() {
        return ProviderSnapshot::ok(
            PROVIDER_ID,
            UsageSourceKind::ProviderReported,
            resources,
            now,
        )
        .with_plan(plan);
    }

    // Nothing usable came back. Only call the key rejected when *both* endpoints
    // rejected it: `OpenRouter` gates some endpoints per key type, so one 403
    // beside one success means gated, not invalid.
    if matches!(credits, Endpoint::AuthFailure) && matches!(key, Endpoint::AuthFailure) {
        return ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::SessionExpired, now);
    }
    let error = failure(&credits)
        .or_else(|| failure(&key))
        .unwrap_or(UsageError::UnsupportedPayload);
    ProviderSnapshot::from_failure(PROVIDER_ID, &error, now)
}

/// The credits meter and the remaining balance.
///
/// Empty when the payload carries no `total_usage`: without it there is nothing
/// measured, and a balance computed from one number would be a guess.
#[must_use]
pub fn credit_resources(data: &Value) -> Vec<UsageResource> {
    let Some(total_usage) = data.get("total_usage").and_then(parse::number) else {
        return Vec::new();
    };
    let used = total_usage.max(0.0);
    let purchased = data
        .get("total_credits")
        .and_then(parse::number)
        .unwrap_or(0.0)
        .max(0.0);

    let mut resources = Vec::new();
    // A meter needs a ceiling. A free or never-topped-up account reports 0 here,
    // and those accounts still get the balance row below.
    if purchased > 0.0 {
        resources.push(bounded_usd(CREDITS_RESOURCE_ID, used, purchased));
    }
    // A real zero is shown as "$0.00 left", never as "No data".
    resources.push(UsageResource::balance(
        BALANCE_RESOURCE_ID,
        UsageUnit::Usd,
        Some((purchased - used).max(0.0)),
    ));
    resources
}

/// This key's spend cap, when it has one.
#[must_use]
pub fn key_resources(data: &Value) -> Vec<UsageResource> {
    let Some(limit) = data
        .get("limit")
        .and_then(parse::number)
        .filter(|v| *v > 0.0)
    else {
        return Vec::new();
    };
    let used = data
        .get("usage")
        .and_then(parse::number)
        .unwrap_or(0.0)
        .max(0.0);
    vec![bounded_usd(KEY_LIMIT_RESOURCE_ID, used, limit)]
}

/// The tier, as the account's own flag names it.
#[must_use]
pub fn plan_name(data: &Value) -> Option<String> {
    data.get("is_free_tier")
        .and_then(parse::boolean)
        .map(|free| if free { "Free tier" } else { "Pay as you go" }.to_string())
}

/// A dollar meter with a percentage derived from its own two numbers.
///
/// The percentage is what lets an exhausted key be skipped by the fallback
/// chain; it is arithmetic over reported values, not an invented reading.
fn bounded_usd(id: &str, used: f64, limit: f64) -> UsageResource {
    let used_percent = parse::clamp_percent(used / limit * 100.0);
    let mut resource = UsageResource::consumption(id, UsageUnit::Usd)
        .with_used(Some(used))
        .with_limit(Some(limit));
    resource.used_percent = used_percent;
    resource.remaining_percent = used_percent.map(|percent| (100.0 - percent).clamp(0.0, 100.0));
    debug_assert_eq!(resource.kind, ResourceKind::Consumption);
    resource
}

/// Classify one endpoint outcome.
fn classify(outcome: Result<&HttpResponse, UsageError>, now: DateTime<Utc>) -> Endpoint {
    let response = match outcome {
        Ok(response) => response,
        Err(error) => return Endpoint::Failed(error),
    };
    if matches!(response.status(), 401 | 403) {
        return Endpoint::AuthFailure;
    }
    if let Err(error) = response.error_for_status(now) {
        return Endpoint::Failed(error);
    }
    match response.json_value() {
        Ok(body) => body
            .get("data")
            .filter(|data| data.is_object())
            .map_or(Endpoint::Failed(UsageError::UnsupportedPayload), |data| {
                Endpoint::Data(data.clone())
            }),
        Err(error) => Endpoint::Failed(error),
    }
}

fn failure(endpoint: &Endpoint) -> Option<UsageError> {
    match endpoint {
        Endpoint::Failed(error) => Some(*error),
        Endpoint::Data(_) | Endpoint::AuthFailure => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{credit_resources, key_resources, map, plan_name};
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{Availability, ReasonCode, ResourceKind, UsageError, UsageUnit};
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::json;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    fn ok(body: &str) -> HttpResponse {
        HttpResponse::from_parts(200, &[], body.as_bytes())
    }

    fn close(actual: Option<f64>, expected: f64) -> bool {
        actual.is_some_and(|value| (value - expected).abs() < 0.001)
    }

    #[test]
    fn credits_give_a_meter_and_a_balance() {
        let resources =
            credit_resources(&json!({ "total_credits": 277.47, "total_usage": 178.20 }));
        assert_eq!(resources.len(), 2);

        let credits = &resources[0];
        assert_eq!(credits.id, "credits");
        assert_eq!(credits.kind, ResourceKind::Consumption);
        assert_eq!(credits.unit, UsageUnit::Usd);
        assert!(close(credits.used, 178.20));
        assert!(close(credits.limit, 277.47));
        // Percentage derived from the two reported numbers, so an exhausted key
        // can be skipped by the fallback chain.
        assert!(close(credits.used_percent, 64.223));

        let balance = &resources[1];
        assert_eq!(balance.id, "balance");
        assert_eq!(balance.kind, ResourceKind::Balance);
        assert!(close(balance.available, 99.27));
    }

    #[test]
    fn an_account_that_never_topped_up_still_gets_a_measured_zero_balance() {
        let resources = credit_resources(&json!({ "total_credits": 0, "total_usage": 0 }));
        assert_eq!(resources.len(), 1);
        assert_eq!(resources[0].id, "balance");
        assert_eq!(resources[0].available, Some(0.0));
    }

    #[test]
    fn a_payload_without_total_usage_produces_nothing_rather_than_a_guess() {
        assert!(credit_resources(&json!({ "foo": "bar" })).is_empty());
        assert!(credit_resources(&json!({ "total_credits": 100 })).is_empty());
    }

    #[test]
    fn the_key_cap_maps_only_when_one_is_configured() {
        let capped = key_resources(&json!({ "usage": 2, "limit": 5 }));
        assert_eq!(capped.len(), 1);
        assert_eq!(capped[0].id, "key_limit");
        assert!(close(capped[0].used, 2.0));
        assert!(close(capped[0].limit, 5.0));
        assert!(close(capped[0].used_percent, 40.0));
        assert!(close(capped[0].remaining_percent, 60.0));

        assert!(key_resources(&json!({ "is_free_tier": true, "limit": null })).is_empty());
        assert!(key_resources(&json!({ "limit": 0 })).is_empty());
    }

    #[test]
    fn the_plan_label_comes_from_the_tier_flag() {
        assert_eq!(
            plan_name(&json!({ "is_free_tier": true })).as_deref(),
            Some("Free tier")
        );
        assert_eq!(
            plan_name(&json!({ "is_free_tier": false })).as_deref(),
            Some("Pay as you go")
        );
        assert_eq!(plan_name(&json!({})), None);
    }

    #[test]
    fn both_endpoints_contribute_to_one_snapshot() {
        let credits = ok(r#"{"data":{"total_credits":100,"total_usage":40}}"#);
        let key = ok(r#"{"data":{"is_free_tier":false,"usage":2,"limit":5}}"#);
        let snapshot = map(Ok(&credits), Ok(&key), now());

        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.plan.as_deref(), Some("Pay as you go"));
        let ids: Vec<_> = snapshot.resources.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["credits", "balance", "key_limit"]);
    }

    #[test]
    fn one_endpoint_failing_never_blanks_the_other() {
        let credits = ok(r#"{"data":{"total_credits":100,"total_usage":40}}"#);
        let snapshot = map(Ok(&credits), Err(UsageError::Network), now());
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.resources.len(), 2);
        assert_eq!(snapshot.reason, None);
    }

    #[test]
    fn a_gated_credits_endpoint_still_shows_the_key_rows() {
        let credits = HttpResponse::from_parts(403, &[], b"{}");
        let key = ok(r#"{"data":{"is_free_tier":false,"usage":1,"limit":5}}"#);
        let snapshot = map(Ok(&credits), Ok(&key), now());
        assert_eq!(snapshot.availability, Availability::Available);
        let ids: Vec<_> = snapshot.resources.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, vec!["key_limit"]);
    }

    #[test]
    fn only_both_endpoints_rejecting_counts_as_a_rejected_key() {
        let rejected = HttpResponse::from_parts(401, &[], b"{}");
        let snapshot = map(Ok(&rejected), Ok(&rejected), now());
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::SessionExpired));
    }

    #[test]
    fn a_gated_endpoint_beside_an_empty_one_is_not_blamed_on_the_key() {
        // `/credits` 403 (gated) and `/key` 200 with nothing usable: the key is
        // valid, so telling the user to replace it would be wrong.
        let credits = HttpResponse::from_parts(403, &[], b"{}");
        let key = ok(r#"{"data":{}}"#);
        let snapshot = map(Ok(&credits), Ok(&key), now());
        assert_ne!(snapshot.reason, Some(ReasonCode::SessionExpired));
        assert_eq!(snapshot.availability, Availability::Unknown);
    }

    #[test]
    fn being_offline_is_never_reported_as_exhaustion() {
        let snapshot = map(Err(UsageError::Network), Err(UsageError::Timeout), now());
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::Network));
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn an_unwrapped_or_broken_body_degrades_without_leaking() {
        let broken = ok("<html>nope</html>");
        let unwrapped = ok(r#"{"total_usage":10}"#);
        for response in [&broken, &unwrapped] {
            let snapshot = map(Ok(response), Ok(response), now());
            assert_eq!(snapshot.availability, Availability::Unknown);
            assert_eq!(snapshot.reason, Some(ReasonCode::UnsupportedPayload));
            let rendered = format!("{snapshot:?}");
            assert!(!rendered.contains("nope"), "{rendered}");
        }
    }
}
