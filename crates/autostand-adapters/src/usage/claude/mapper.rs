//! Pure normalisation of Anthropic's `/api/oauth/usage` payload.
//!
//! Nothing here performs I/O or reads a clock: `now` is injected, which is what
//! lets the whole mapping be table-tested against a captured fixture. A payload
//! shape change therefore degrades Claude to "no data" rather than producing a
//! wrong number.
//!
//! # What the payload carries
//!
//! | Payload | Resource |
//! | --- | --- |
//! | `five_hour.{utilization, resets_at}` | `session`, percent, period 5h |
//! | `seven_day.{…}` | `weekly`, percent, period 7d |
//! | `seven_day_sonnet.{…}` | `sonnet`, percent, period 7d |
//! | `limits[]` with `kind == "weekly_scoped"` | one row each, labelled from `scope.model.display_name` |
//! | `extra_usage.{is_enabled, used_credits, monthly_limit}` (cents) | `extra_usage`, USD |
//!
//! The plan string (`"Max 20x"`) comes from the *credential*, not the payload:
//! `subscriptionType` title-cased, plus the `\d+x` multiplier found in
//! `rateLimitTier`.

use std::sync::OnceLock;
use std::time::Duration;

use chrono::{DateTime, Utc};
use regex::Regex;
use serde_json::Value;

use crate::usage::http::HttpResponse;
use crate::usage::model::{
    ProviderSnapshot, UsageError, UsageResource, UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// The rolling five-hour window Anthropic reports as `five_hour`.
pub const SESSION_PERIOD: Duration = Duration::from_secs(5 * 60 * 60);

/// The rolling seven-day window, shared by every `seven_day*` key and by the
/// `weekly` group in `limits[]`.
pub const WEEKLY_PERIOD: Duration = Duration::from_secs(7 * 24 * 60 * 60);

/// Shown when the stored login cannot read usage because it lacks
/// `user:profile`. Not an error: inference still works, only the meters are
/// missing, and one `claude` re-login restores them.
pub const MISSING_SCOPE_NOTICE: &str = "Re-login for live usage";

/// The credential facts the mapper needs, and nothing else.
///
/// Deliberately excludes the token: a mapper that cannot see a secret cannot
/// leak one into a snapshot.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlanFacts<'a> {
    pub subscription_type: Option<&'a str>,
    pub rate_limit_tier: Option<&'a str>,
}

/// Normalise a successful `/api/oauth/usage` body.
///
/// `response` is part of the fixed mapper shape; Claude reports everything in
/// the body, so no value is currently recovered from a header. A body that
/// parses but yields no recognisable window is
/// [`UsageError::UnsupportedPayload`] — reporting an empty meter as a healthy
/// provider would be a fabricated reading.
#[must_use]
pub fn map(
    payload: &Value,
    _response: &HttpResponse,
    plan: PlanFacts<'_>,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    let mut resources = Vec::new();
    push_window(
        &mut resources,
        payload.get("five_hour"),
        "session",
        SESSION_PERIOD,
        now,
    );
    push_window(
        &mut resources,
        payload.get("seven_day"),
        "weekly",
        WEEKLY_PERIOD,
        now,
    );
    push_window(
        &mut resources,
        payload.get("seven_day_sonnet"),
        "sonnet",
        WEEKLY_PERIOD,
        now,
    );
    push_scoped_weekly_limits(&mut resources, payload.get("limits"), now);
    push_extra_usage(&mut resources, payload.get("extra_usage"));

    if resources.is_empty() {
        return ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::UnsupportedPayload, now)
            .with_plan(format_plan(plan));
    }

    ProviderSnapshot::ok(
        PROVIDER_ID,
        UsageSourceKind::ProviderReported,
        resources,
        now,
    )
    .with_plan(format_plan(plan))
}

/// The snapshot for a login that authenticates but cannot read usage.
///
/// Availability stays as the shared classifier decides (`available` — inference
/// is unaffected); the notice is what tells the user the one-step fix.
#[must_use]
pub fn missing_scope_snapshot(plan: PlanFacts<'_>, now: DateTime<Utc>) -> ProviderSnapshot {
    ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::MissingProfileScope, now)
        .with_plan(format_plan(plan))
        .with_notice(Some(MISSING_SCOPE_NOTICE.to_string()))
}

/// The snapshot for an inference-only `CLAUDE_CODE_OAUTH_TOKEN`.
#[must_use]
pub fn inference_only_snapshot(plan: PlanFacts<'_>, now: DateTime<Utc>) -> ProviderSnapshot {
    ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::UsageRequiresCliLogin, now)
        .with_plan(format_plan(plan))
}

/// The snapshot served while Anthropic's 429 cooldown is in force.
///
/// With a last-good reading the bars stay on screen, flagged `stale` so the UI
/// can say so; without one there is nothing to show but the reason. Either way
/// the endpoint is not called — during a cooldown a user hammering refresh is
/// exactly what makes it worse.
#[must_use]
pub fn rate_limited_snapshot(
    last_good: Option<&ProviderSnapshot>,
    retry_after_secs: Option<u64>,
    plan: PlanFacts<'_>,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    let notice = Some(rate_limited_notice(retry_after_secs));
    match last_good {
        Some(cached) => {
            let mut snapshot = cached.clone();
            snapshot.checked_at = now;
            snapshot.stale = true;
            snapshot.notice = notice;
            snapshot
        }
        None => ProviderSnapshot::from_failure(
            PROVIDER_ID,
            &UsageError::RateLimited { retry_after_secs },
            now,
        )
        .with_plan(format_plan(plan))
        .with_notice(notice),
    }
}

/// Human copy for the cooldown. App-authored: no vendor text, no header value.
///
/// The wait rounds **up**, so the copy never promises a retry that is still a
/// few seconds away.
#[must_use]
pub fn rate_limited_notice(retry_after_secs: Option<u64>) -> String {
    match retry_after_secs {
        Some(0) => "Live usage rate limited — retrying now".to_string(),
        Some(seconds) => format!(
            "Live usage rate limited — retry in ~{}m",
            seconds.div_ceil(60)
        ),
        None => "Live usage rate limited — data may be stale".to_string(),
    }
}

/// `"max"` + `"default_claude_max_20x"` → `"Max 20x"`.
///
/// The multiplier is read from the tier by pattern rather than by a lookup
/// table, because Anthropic spells the tier differently across plans and only
/// the `<n>x` part is meaningful to the user.
#[must_use]
pub fn format_plan(plan: PlanFacts<'_>) -> Option<String> {
    let subscription = plan.subscription_type?.trim();
    if subscription.is_empty() {
        return None;
    }
    let base = parse::title_case(subscription);
    if base.is_empty() {
        return None;
    }
    match plan.rate_limit_tier.and_then(multiplier) {
        Some(multiplier) => Some(format!("{base} {multiplier}")),
        None => Some(base),
    }
}

fn multiplier(tier: &str) -> Option<&str> {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    let pattern = PATTERN.get_or_init(|| {
        Regex::new(r"\d+x").expect("the multiplier pattern is a literal and always compiles")
    });
    pattern.find(tier).map(|found| found.as_str())
}

/// One `{ utilization, resets_at }` window.
///
/// A window without `utilization` is skipped rather than emitted as an empty
/// meter: the provider did not report it, and a row of dashes is noise.
fn push_window(
    resources: &mut Vec<UsageResource>,
    window: Option<&Value>,
    id: &str,
    period: Duration,
    now: DateTime<Utc>,
) {
    let Some(window) = window.filter(|value| value.is_object()) else {
        return;
    };
    let Some(used_percent) = window
        .get("utilization")
        .and_then(parse::number)
        .and_then(parse::clamp_percent)
    else {
        return;
    };
    resources.push(
        UsageResource::percent(id, Some(used_percent))
            .with_period(Some(period))
            .with_resets_at(parse::parse_reset_at(window, now))
            .derive_pace(now),
    );
}

/// Model-scoped weekly limits, one row per `kind == "weekly_scoped"` entry.
///
/// Anthropic moved per-model weekly windows out of the top-level
/// `seven_day_<model>` keys (which now come back `null`) and into this array,
/// naming each one through `scope.model.display_name`.
fn push_scoped_weekly_limits(
    resources: &mut Vec<UsageResource>,
    limits: Option<&Value>,
    now: DateTime<Utc>,
) {
    let Some(entries) = limits.and_then(Value::as_array) else {
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        if entry.get("kind").and_then(parse::text) != Some("weekly_scoped") {
            continue;
        }
        let Some(used_percent) = entry
            .get("percent")
            .and_then(parse::number)
            .and_then(parse::clamp_percent)
        else {
            continue;
        };
        let display_name = entry
            .get("scope")
            .and_then(|scope| scope.get("model"))
            .and_then(|model| model.get("display_name"))
            .and_then(parse::text);
        resources.push(
            UsageResource::percent(scoped_id(display_name, index), Some(used_percent))
                .with_period(Some(WEEKLY_PERIOD))
                .with_resets_at(parse::parse_reset_at(entry, now))
                .with_label(display_name.map(str::to_string))
                .derive_pace(now),
        );
    }
}

/// A stable id per scoped row.
///
/// Derived from the model name where there is one, so the id survives Anthropic
/// reordering the array; the positional form is the fallback for an unnamed
/// scope, which would otherwise collide with a sibling.
fn scoped_id(display_name: Option<&str>, index: usize) -> String {
    match display_name.map(slug).filter(|slug| !slug.is_empty()) {
        Some(slug) => format!("weekly_scoped_{slug}"),
        None => format!("weekly_scoped_{index}"),
    }
}

fn slug(raw: &str) -> String {
    raw.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|part| !part.is_empty())
        .map(str::to_ascii_lowercase)
        .collect::<Vec<_>>()
        .join("_")
}

/// Pay-as-you-go spend beyond the subscription, reported in **cents**.
///
/// A monthly cap makes it a meter (`Consumption`); without one it is an
/// open-ended running total, which is a `Balance` in this vocabulary — there is
/// no limit to fill, so there is no percentage to invent.
fn push_extra_usage(resources: &mut Vec<UsageResource>, extra: Option<&Value>) {
    let Some(extra) = extra.filter(|value| value.is_object()) else {
        return;
    };
    if extra.get("is_enabled").and_then(parse::boolean) != Some(true) {
        return;
    }
    let Some(used) = extra
        .get("used_credits")
        .and_then(parse::number)
        .map(parse::cents_to_dollars)
    else {
        return;
    };

    let limit = extra
        .get("monthly_limit")
        .and_then(parse::number)
        .map(parse::cents_to_dollars)
        .filter(|limit| *limit > 0.0);

    match limit {
        Some(limit) => resources.push(
            UsageResource::consumption("extra_usage", UsageUnit::Usd)
                .with_used(Some(used))
                .with_limit(Some(limit)),
        ),
        // Nothing spent and no cap is not a resource, it is silence.
        None if used > 0.0 => resources.push(
            UsageResource::balance("extra_usage", UsageUnit::Usd, Some(used)).with_used(Some(used)),
        ),
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::{
        format_plan, inference_only_snapshot, map, missing_scope_snapshot, rate_limited_notice,
        rate_limited_snapshot, PlanFacts, MISSING_SCOPE_NOTICE, SESSION_PERIOD, WEEKLY_PERIOD,
    };
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{
        Availability, ProviderSnapshot, ReasonCode, ResourceKind, UsageResource, UsageSourceKind,
        UsageUnit,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{json, Value};

    /// The real payload captured from `/api/oauth/usage` on a Max 20x account.
    const FIXTURE: &str = include_str!("../../../tests/fixtures/usage/claude-usage.json");

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    fn ok_response() -> HttpResponse {
        HttpResponse::from_parts(200, &[("content-type", "application/json")], b"{}")
    }

    fn max_20x() -> PlanFacts<'static> {
        PlanFacts {
            subscription_type: Some("max"),
            rate_limit_tier: Some("default_claude_max_20x"),
        }
    }

    fn snapshot(payload: &Value) -> ProviderSnapshot {
        map(payload, &ok_response(), max_20x(), now())
    }

    fn resource<'a>(snapshot: &'a ProviderSnapshot, id: &str) -> Option<&'a UsageResource> {
        snapshot.resources.iter().find(|r| r.id == id)
    }

    // ---- the captured payload -------------------------------------------------

    #[test]
    fn the_captured_payload_maps_to_the_windows_it_reports() {
        let payload: Value = serde_json::from_str(FIXTURE).expect("the fixture is valid JSON");
        let snapshot = snapshot(&payload);

        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.source, UsageSourceKind::ProviderReported);
        assert_eq!(snapshot.plan.as_deref(), Some("Max 20x"));
        assert_eq!(snapshot.reason, None);
        assert!(!snapshot.stale);

        let session = resource(&snapshot, "session").expect("five_hour maps to session");
        assert_eq!(session.used_percent, Some(16.0));
        assert_eq!(session.remaining_percent, Some(84.0));
        assert_eq!(session.period(), Some(SESSION_PERIOD));
        assert_eq!(
            session.resets_at,
            Some(
                Utc.with_ymd_and_hms(2026, 8, 14, 4, 30, 0).unwrap()
                    + chrono::Duration::milliseconds(56)
            )
        );

        let weekly = resource(&snapshot, "weekly").expect("seven_day maps to weekly");
        assert_eq!(weekly.used_percent, Some(44.0));
        assert_eq!(weekly.period(), Some(WEEKLY_PERIOD));

        // Every `seven_day_*` model key is null in this capture, so no row is
        // invented for them.
        assert!(resource(&snapshot, "sonnet").is_none());

        // `limits[]` carries session + weekly_all + weekly_scoped; only the
        // scoped one becomes a row, the other two duplicate the top-level keys.
        let fable = resource(&snapshot, "weekly_scoped_fable").expect("the scoped model row");
        assert_eq!(fable.used_percent, Some(0.0));
        assert_eq!(fable.label.as_deref(), Some("Fable"));
        assert_eq!(fable.resets_at, None);
        assert_eq!(snapshot.resources.len(), 3);

        // `extra_usage.is_enabled` is false in this capture.
        assert!(resource(&snapshot, "extra_usage").is_none());
        assert_eq!(snapshot.tightest_remaining_percent(), Some(56.0));
    }

    #[test]
    fn the_captured_payload_never_leaks_into_the_snapshot_debug() {
        let payload: Value = serde_json::from_str(FIXTURE).unwrap();
        let rendered = format!("{:?}", snapshot(&payload));
        assert!(!rendered.contains("disclaimer"), "{rendered}");
        assert!(!rendered.contains("support.claude.com"), "{rendered}");
    }

    // ---- table: window mapping ------------------------------------------------

    #[test]
    fn optional_fields_may_all_be_absent() {
        // Only `utilization` is load-bearing. No reset, no dollars, no limits.
        let snapshot = snapshot(&json!({ "five_hour": { "utilization": 10 } }));
        let session = resource(&snapshot, "session").unwrap();
        assert_eq!(session.used_percent, Some(10.0));
        assert_eq!(session.resets_at, None);
        // No reset means no elapsed time, so no projection is invented.
        assert_eq!(session.pace, None);
    }

    #[test]
    fn a_window_without_utilization_is_skipped_not_zeroed() {
        // The golden rule: a field the provider did not send is never 0.
        for payload in [
            json!({ "five_hour": { "resets_at": "2099-01-01T00:00:00Z" }, "seven_day": { "utilization": 5 } }),
            json!({ "five_hour": null, "seven_day": { "utilization": 5 } }),
            json!({ "five_hour": { "utilization": null }, "seven_day": { "utilization": 5 } }),
            json!({ "five_hour": "unexpected", "seven_day": { "utilization": 5 } }),
        ] {
            let snapshot = snapshot(&payload);
            assert!(resource(&snapshot, "session").is_none(), "{payload}");
            assert!(resource(&snapshot, "weekly").is_some(), "{payload}");
        }
    }

    #[test]
    fn percentages_outside_the_range_are_clamped_never_dropped() {
        let snapshot = snapshot(&json!({
            "five_hour": { "utilization": 137.5 },
            "seven_day": { "utilization": -20 },
        }));
        assert_eq!(
            resource(&snapshot, "session").unwrap().used_percent,
            Some(100.0)
        );
        assert_eq!(
            resource(&snapshot, "session").unwrap().remaining_percent,
            Some(0.0)
        );
        assert_eq!(
            resource(&snapshot, "weekly").unwrap().used_percent,
            Some(0.0)
        );
    }

    #[test]
    fn a_non_finite_percentage_becomes_no_data_rather_than_zero() {
        // A string that is not a number must not read as an empty quota.
        let snapshot = snapshot(&json!({
            "five_hour": { "utilization": "not-a-number" },
            "seven_day": { "utilization": "44" },
        }));
        assert!(resource(&snapshot, "session").is_none());
        // A numeric string is still a number.
        assert_eq!(
            resource(&snapshot, "weekly").unwrap().used_percent,
            Some(44.0)
        );
    }

    #[test]
    fn a_reset_reads_from_epoch_seconds_or_milliseconds() {
        let expected = Utc.with_ymd_and_hms(2036, 7, 18, 1, 15, 0).unwrap();
        let seconds = snapshot(&json!({
            "five_hour": { "utilization": 10, "resets_at": expected.timestamp() },
        }));
        assert_eq!(
            resource(&seconds, "session").unwrap().resets_at,
            Some(expected)
        );

        let milliseconds = snapshot(&json!({
            "five_hour": { "utilization": 10, "resets_at": expected.timestamp_millis() },
        }));
        assert_eq!(
            resource(&milliseconds, "session").unwrap().resets_at,
            Some(expected)
        );
    }

    #[test]
    fn a_reset_reads_from_rfc3339_with_any_fraction_and_no_zone() {
        // Anthropic sends microsecond precision; a bare timestamp means UTC.
        let with_micros = snapshot(&json!({
            "five_hour": { "utilization": 10, "resets_at": "2099-06-01T12:00:00.123456" },
        }));
        assert_eq!(
            resource(&with_micros, "session").unwrap().resets_at,
            Some(
                Utc.with_ymd_and_hms(2099, 6, 1, 12, 0, 0).unwrap()
                    + chrono::Duration::milliseconds(123)
            )
        );

        let with_offset = snapshot(&json!({
            "five_hour": { "utilization": 10, "resets_at": "2099-06-01T12:00:00.056078+00:00" },
        }));
        assert!(resource(&with_offset, "session")
            .unwrap()
            .resets_at
            .is_some());
    }

    #[test]
    fn pace_projects_from_the_window_length_and_the_reset() {
        // Half of the five-hour window gone with 10% spent projects to 20%.
        let snapshot = snapshot(&json!({
            "five_hour": {
                "utilization": 10,
                "resets_at": (now() + chrono::Duration::seconds(9_000)).to_rfc3339(),
            },
        }));
        assert_eq!(
            resource(&snapshot, "session").unwrap().pace,
            Some(autostand_core::pace::Pace::Ahead)
        );
    }

    // ---- table: scoped weekly limits -----------------------------------------

    #[test]
    fn every_weekly_scoped_entry_becomes_its_own_labelled_row() {
        let snapshot = snapshot(&json!({
            "limits": [
                { "kind": "session", "percent": 10 },
                { "kind": "weekly_all", "percent": 20 },
                { "kind": "weekly_scoped", "percent": 7, "resets_at": "2099-01-08T00:00:00Z",
                  "scope": { "model": { "display_name": "Fable" } } },
                { "kind": "weekly_scoped", "percent": 3,
                  "scope": { "model": { "display_name": "Claude Opus 4.5" } } },
            ],
        }));

        // Session and weekly_all duplicate the top-level keys; only the scoped
        // rows are new information.
        assert_eq!(snapshot.resources.len(), 2);

        let fable = resource(&snapshot, "weekly_scoped_fable").unwrap();
        assert_eq!(fable.used_percent, Some(7.0));
        assert_eq!(fable.label.as_deref(), Some("Fable"));
        assert_eq!(fable.period(), Some(WEEKLY_PERIOD));

        let opus = resource(&snapshot, "weekly_scoped_claude_opus_4_5").unwrap();
        assert_eq!(opus.used_percent, Some(3.0));
        assert_eq!(opus.label.as_deref(), Some("Claude Opus 4.5"));
    }

    #[test]
    fn an_unnamed_scope_still_gets_a_unique_id() {
        let snapshot = snapshot(&json!({
            "limits": [
                { "kind": "weekly_scoped", "percent": 1 },
                { "kind": "weekly_scoped", "percent": 2, "scope": { "model": {} } },
            ],
        }));
        assert_eq!(snapshot.resources.len(), 2);
        assert_eq!(
            resource(&snapshot, "weekly_scoped_0").unwrap().used_percent,
            Some(1.0)
        );
        assert_eq!(
            resource(&snapshot, "weekly_scoped_1").unwrap().used_percent,
            Some(2.0)
        );
        assert_eq!(resource(&snapshot, "weekly_scoped_0").unwrap().label, None);
    }

    #[test]
    fn limits_that_is_not_an_array_is_ignored() {
        let snapshot = snapshot(&json!({ "five_hour": { "utilization": 1 }, "limits": {} }));
        assert_eq!(snapshot.resources.len(), 1);
    }

    // ---- table: extra usage ---------------------------------------------------

    #[test]
    fn capped_extra_usage_is_a_dollar_meter() {
        let snapshot = snapshot(&json!({
            "extra_usage": { "is_enabled": true, "used_credits": 500, "monthly_limit": 1000 },
        }));
        let extra = resource(&snapshot, "extra_usage").unwrap();
        assert_eq!(extra.kind, ResourceKind::Consumption);
        assert_eq!(extra.unit, UsageUnit::Usd);
        assert_eq!(extra.used, Some(5.0));
        assert_eq!(extra.limit, Some(10.0));
        // Dollars are not percentages: nothing is invented for the meter fill.
        assert_eq!(extra.used_percent, None);
    }

    #[test]
    fn uncapped_extra_usage_is_an_open_ended_balance() {
        let snapshot = snapshot(&json!({
            "extra_usage": { "is_enabled": true, "used_credits": 123_456 },
        }));
        let extra = resource(&snapshot, "extra_usage").unwrap();
        assert_eq!(extra.kind, ResourceKind::Balance);
        assert_eq!(extra.available, Some(1234.56));
        assert_eq!(extra.limit, None);
    }

    #[test]
    fn extra_usage_is_skipped_when_disabled_unspent_or_uncredited() {
        for payload in [
            json!({ "five_hour": { "utilization": 1 },
                    "extra_usage": { "is_enabled": false, "used_credits": 500, "monthly_limit": 1000 } }),
            json!({ "five_hour": { "utilization": 1 },
                    "extra_usage": { "is_enabled": true } }),
            // Enabled, nothing spent, no cap: silence, not a $0.00 row.
            json!({ "five_hour": { "utilization": 1 },
                    "extra_usage": { "is_enabled": true, "used_credits": 0 } }),
            // A zero cap is not a cap.
            json!({ "five_hour": { "utilization": 1 },
                    "extra_usage": { "is_enabled": true, "used_credits": 0, "monthly_limit": 0 } }),
        ] {
            let snapshot = snapshot(&payload);
            assert!(resource(&snapshot, "extra_usage").is_none(), "{payload}");
        }
    }

    // ---- table: plan ----------------------------------------------------------

    #[test]
    fn the_plan_joins_the_subscription_with_the_tier_multiplier() {
        let cases = [
            (Some("max"), Some("default_claude_max_20x"), Some("Max 20x")),
            (
                Some("max"),
                Some("claude_max_subscription_5x"),
                Some("Max 5x"),
            ),
            (Some("pro"), None, Some("Pro")),
            // A tier with no multiplier contributes nothing rather than noise.
            (Some("pro"), Some("default"), Some("Pro")),
            (Some("PRO PLAN"), None, Some("Pro Plan")),
            (None, Some("default_claude_max_20x"), None),
            (Some("   "), Some("default_claude_max_20x"), None),
        ];
        for (subscription_type, rate_limit_tier, expected) in cases {
            let plan = format_plan(PlanFacts {
                subscription_type,
                rate_limit_tier,
            });
            assert_eq!(plan.as_deref(), expected, "{subscription_type:?}");
        }
    }

    // ---- table: degraded snapshots -------------------------------------------

    #[test]
    fn a_payload_with_no_recognisable_window_degrades_to_unknown() {
        // A shape change must never produce a wrong number.
        for payload in [json!({}), json!({ "something_new": { "utilization": 10 } })] {
            let snapshot = snapshot(&payload);
            assert_eq!(snapshot.availability, Availability::Unknown, "{payload}");
            assert_eq!(snapshot.reason, Some(ReasonCode::UnsupportedPayload));
            assert!(snapshot.resources.is_empty());
            // The plan is still known: it comes from the credential.
            assert_eq!(snapshot.plan.as_deref(), Some("Max 20x"));
        }
    }

    #[test]
    fn the_scope_gate_is_a_notice_not_a_failure() {
        // Inference still works; only the meters are missing.
        let snapshot = missing_scope_snapshot(max_20x(), now());
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.reason, Some(ReasonCode::MissingProfileScope));
        assert_eq!(snapshot.notice.as_deref(), Some(MISSING_SCOPE_NOTICE));
        assert_eq!(snapshot.plan.as_deref(), Some("Max 20x"));
        assert!(snapshot.resources.is_empty());
        // No percentage exists, so grading cannot downgrade it to exhausted.
        assert_eq!(snapshot.graded(20.0).availability, Availability::Available);
    }

    #[test]
    fn a_setup_token_says_so_instead_of_reading_as_unavailable() {
        let snapshot = inference_only_snapshot(PlanFacts::default(), now());
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UsageRequiresCliLogin));
        assert_eq!(snapshot.plan, None);
    }

    // ---- table: rate limiting -------------------------------------------------

    #[test]
    fn a_cooldown_keeps_the_last_good_bars_and_marks_them_stale() {
        let last_good = snapshot(&json!({ "five_hour": { "utilization": 25 } }));
        let later = now() + chrono::Duration::minutes(3);
        let served = rate_limited_snapshot(Some(&last_good), Some(600), max_20x(), later);

        assert!(served.stale);
        assert_eq!(served.checked_at, later);
        assert_eq!(served.availability, Availability::Available);
        assert_eq!(
            resource(&served, "session").unwrap().used_percent,
            Some(25.0)
        );
        assert_eq!(
            served.notice.as_deref(),
            Some("Live usage rate limited — retry in ~10m")
        );
    }

    #[test]
    fn a_cooldown_with_nothing_cached_states_the_reason() {
        let served = rate_limited_snapshot(None, Some(240), max_20x(), now());
        assert_eq!(served.availability, Availability::RateLimited);
        assert_eq!(served.reason, Some(ReasonCode::RateLimited));
        assert!(served.resources.is_empty());
        assert_eq!(served.plan.as_deref(), Some("Max 20x"));
        assert_eq!(
            served.notice.as_deref(),
            Some("Live usage rate limited — retry in ~4m")
        );
    }

    #[test]
    fn the_cooldown_notice_rounds_up_and_survives_a_missing_retry_after() {
        assert_eq!(
            rate_limited_notice(Some(1)),
            "Live usage rate limited — retry in ~1m"
        );
        assert_eq!(
            rate_limited_notice(Some(61)),
            "Live usage rate limited — retry in ~2m"
        );
        assert_eq!(
            rate_limited_notice(Some(0)),
            "Live usage rate limited — retrying now"
        );
        assert_eq!(
            rate_limited_notice(None),
            "Live usage rate limited — data may be stale"
        );
    }
}
