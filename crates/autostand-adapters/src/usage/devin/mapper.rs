//! Pure mapping of Devin's `GetUserStatus` body into a [`ProviderSnapshot`].
//!
//! No I/O and no clock: `now` is injected, which is what makes the whole thing
//! table-testable from a fixture.
//!
//! Devin reports quota as percent **remaining**; every meter here flips it to
//! percent used. A field Devin did not send stays `None` — including the daily
//! window on a plan that hides it, which is absent rather than zero.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::model::{
    Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError, UsageResource,
    UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// One day, the daily quota's window.
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// Seven days, the weekly quota's window.
const WEEK_MS: i64 = 7 * DAY_MS;

/// Devin's extra-usage balance is reported in millionths of a dollar.
const MICROS_PER_DOLLAR: f64 = 1_000_000.0;

/// Devin's own failure modes on top of the shared ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum DevinProbeError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// The body parsed but carried no quota and no balance — nothing to show.
    #[error("quota unavailable")]
    QuotaUnavailable,
}

impl ProbeFailure for DevinProbeError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm.
        match self {
            Self::Usage(error) => error.classify(),
            // A payload with no readable quota is "we could not measure", never
            // "you are out".
            Self::QuotaUnavailable => (Availability::Unknown, ReasonCode::UnsupportedPayload),
        }
    }
}

/// Map a `GetUserStatus` response body.
///
/// `body` is the whole response object; the quota lives under `userStatus`.
pub fn map(body: &Value, now: DateTime<Utc>) -> Result<ProviderSnapshot, DevinProbeError> {
    let user_status = body
        .get("userStatus")
        .ok_or(DevinProbeError::Usage(UsageError::UnsupportedPayload))?;
    map_user_status(user_status, now)
}

/// Map the `userStatus` object itself.
pub fn map_user_status(
    user_status: &Value,
    now: DateTime<Utc>,
) -> Result<ProviderSnapshot, DevinProbeError> {
    let plan_status = user_status.get("planStatus").unwrap_or(&Value::Null);
    let plan_info = plan_status.get("planInfo").unwrap_or(&Value::Null);
    let hide_daily = plan_info
        .get("hideDailyQuota")
        .and_then(parse::boolean)
        .unwrap_or(false);

    let daily_remaining = plan_status
        .get("dailyQuotaRemainingPercent")
        .and_then(parse::number);
    let weekly_remaining = plan_status
        .get("weeklyQuotaRemainingPercent")
        .and_then(parse::number);

    let mut resources = Vec::new();
    if !hide_daily {
        if let Some(remaining) = daily_remaining {
            resources.push(quota(
                "daily",
                remaining,
                plan_status.get("dailyQuotaResetAtUnix"),
                DAY_MS,
                now,
            ));
        }
    }

    // A plan that hides the daily quota still has one; with no weekly figure to
    // show, that hidden daily reading fills the weekly row rather than leaving
    // the account with no meter at all.
    let weekly_source = weekly_remaining.or(if hide_daily { daily_remaining } else { None });
    if let Some(remaining) = weekly_source {
        resources.push(quota(
            "weekly",
            remaining,
            plan_status.get("weeklyQuotaResetAtUnix"),
            WEEK_MS,
            now,
        ));
    }

    if let Some(balance) = extra_usage_dollars(plan_status.get("overageBalanceMicros")) {
        resources.push(UsageResource::balance(
            "extraUsageBalance",
            UsageUnit::Usd,
            Some(balance),
        ));
    }

    if resources.is_empty() {
        return Err(DevinProbeError::QuotaUnavailable);
    }

    Ok(ProviderSnapshot::ok(
        PROVIDER_ID,
        UsageSourceKind::ProviderReported,
        resources,
        now,
    )
    .with_plan(plan_name(plan_info)))
}

/// A quota row: percent remaining flipped to percent used, with its window.
fn quota(
    id: &str,
    remaining_percent: f64,
    reset: Option<&Value>,
    period_ms: i64,
    now: DateTime<Utc>,
) -> UsageResource {
    UsageResource::percent(id, parse::clamp_percent(100.0 - remaining_percent))
        .with_resets_at(reset.and_then(|value| reset_at(value, now)))
        .with_period_ms(Some(period_ms))
        .derive_projection(now)
}

/// Devin's `…ResetAtUnix` fields are epoch seconds, and it sends them as JSON
/// strings as often as numbers — a shape the shared string branch reads as
/// RFC 3339 and rejects. Numbers are tried first, then the shared parser, so a
/// future switch to a timestamp still works.
fn reset_at(value: &Value, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    parse::number(value)
        .and_then(parse::parse_epoch)
        .or_else(|| parse::parse_reset_at(value, now))
}

/// Dollars from a micro-dollar balance.
///
/// A present balance of zero is a measured zero and stays `Some(0.0)`; only an
/// absent or non-numeric field becomes `None` ("No data").
fn extra_usage_dollars(value: Option<&Value>) -> Option<f64> {
    let micros = value.and_then(parse::number)?;
    Some(micros.max(0.0) / MICROS_PER_DOLLAR)
}

fn plan_name(plan_info: &Value) -> Option<String> {
    plan_info
        .get("planName")
        .and_then(parse::text)
        .map(parse::title_case)
}

#[cfg(test)]
mod tests {
    use super::{map, map_user_status, DevinProbeError, DAY_MS, WEEK_MS};
    use crate::usage::model::{
        Availability, ProbeFailure, ReasonCode, ResourceKind, UsageResource, UsageSourceKind,
        UsageUnit,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{json, Value};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap()
    }

    fn user_status() -> Value {
        json!({
            "planStatus": {
                "planInfo": { "planName": "Max", "billingStrategy": "BILLING_STRATEGY_QUOTA" },
                "dailyQuotaRemainingPercent": 100,
                "weeklyQuotaRemainingPercent": 40,
                "overageBalanceMicros": "964220000",
                "dailyQuotaResetAtUnix": "1774080000",
                "weeklyQuotaResetAtUnix": "1774166400"
            }
        })
    }

    fn find<'a>(resources: &'a [UsageResource], id: &str) -> Option<&'a UsageResource> {
        resources.iter().find(|resource| resource.id == id)
    }

    #[test]
    fn maps_daily_weekly_and_the_extra_usage_balance() {
        let snapshot = map(&json!({ "userStatus": user_status() }), now()).unwrap();

        assert_eq!(snapshot.provider, "devin");
        assert_eq!(snapshot.plan.as_deref(), Some("Max"));
        assert_eq!(snapshot.source, UsageSourceKind::ProviderReported);
        assert_eq!(snapshot.availability, Availability::Available);

        let daily = find(&snapshot.resources, "daily").unwrap();
        assert_eq!(daily.used_percent, Some(0.0));
        assert_eq!(daily.period_duration_ms, Some(DAY_MS));
        assert!(daily.resets_at.is_some());

        let weekly = find(&snapshot.resources, "weekly").unwrap();
        assert_eq!(weekly.used_percent, Some(60.0));
        assert_eq!(weekly.period_duration_ms, Some(WEEK_MS));

        let balance = find(&snapshot.resources, "extraUsageBalance").unwrap();
        assert_eq!(balance.kind, ResourceKind::Balance);
        assert_eq!(balance.unit, UsageUnit::Usd);
        assert!((balance.available.unwrap() - 964.22).abs() < 1e-6);
    }

    #[test]
    fn a_present_zero_balance_stays_zero_and_never_becomes_no_data() {
        let mut status = user_status();
        status["planStatus"]["overageBalanceMicros"] = json!("0");

        let snapshot = map_user_status(&status, now()).unwrap();

        assert_eq!(
            find(&snapshot.resources, "extraUsageBalance")
                .unwrap()
                .available,
            Some(0.0)
        );
    }

    #[test]
    fn an_absent_balance_is_no_data_rather_than_zero() {
        let mut status = user_status();
        status["planStatus"]
            .as_object_mut()
            .unwrap()
            .remove("overageBalanceMicros");

        let snapshot = map_user_status(&status, now()).unwrap();

        assert!(find(&snapshot.resources, "extraUsageBalance").is_none());
    }

    #[test]
    fn a_hidden_daily_quota_fills_the_missing_weekly_row_still_flipped_to_used() {
        let mut status = user_status();
        status["planStatus"]["planInfo"]["hideDailyQuota"] = json!(true);
        status["planStatus"]["dailyQuotaRemainingPercent"] = json!(30);
        status["planStatus"]
            .as_object_mut()
            .unwrap()
            .remove("weeklyQuotaRemainingPercent");

        let snapshot = map_user_status(&status, now()).unwrap();

        assert!(find(&snapshot.resources, "daily").is_none());
        // 30% remaining is 70% used — not passed through raw.
        assert_eq!(
            find(&snapshot.resources, "weekly").unwrap().used_percent,
            Some(70.0)
        );
    }

    #[test]
    fn a_hidden_daily_quota_does_not_displace_a_real_weekly_reading() {
        let mut status = user_status();
        status["planStatus"]["planInfo"]["hideDailyQuota"] = json!(true);

        let snapshot = map_user_status(&status, now()).unwrap();

        assert!(find(&snapshot.resources, "daily").is_none());
        assert_eq!(
            find(&snapshot.resources, "weekly").unwrap().used_percent,
            Some(60.0)
        );
    }

    #[test]
    fn a_body_with_nothing_displayable_degrades_instead_of_showing_zeros() {
        let status = json!({ "planStatus": { "planInfo": { "planName": "Max" } } });

        assert_eq!(
            map_user_status(&status, now()),
            Err(DevinProbeError::QuotaUnavailable)
        );
        assert_eq!(
            DevinProbeError::QuotaUnavailable.classify(),
            (Availability::Unknown, ReasonCode::UnsupportedPayload)
        );
    }

    #[test]
    fn a_body_without_user_status_is_an_unsupported_payload() {
        let error = map(&json!({ "unexpected": true }), now()).unwrap_err();
        assert_eq!(error.classify().1, ReasonCode::UnsupportedPayload);
    }

    #[test]
    fn a_missing_plan_name_stays_none_rather_than_being_invented() {
        let mut status = user_status();
        status["planStatus"]["planInfo"]
            .as_object_mut()
            .unwrap()
            .remove("planName");

        assert_eq!(map_user_status(&status, now()).unwrap().plan, None);
    }

    #[test]
    fn an_out_of_range_remaining_percentage_is_clamped_not_wrapped() {
        let mut status = user_status();
        status["planStatus"]["weeklyQuotaRemainingPercent"] = json!(-25);

        let snapshot = map_user_status(&status, now()).unwrap();

        assert_eq!(
            find(&snapshot.resources, "weekly").unwrap().used_percent,
            Some(100.0)
        );
    }

    #[test]
    fn a_failure_never_carries_free_text_from_the_payload() {
        let rendered = DevinProbeError::QuotaUnavailable.to_string();
        assert_eq!(rendered, "quota unavailable");
    }
}
