//! Pure `(payload, headers, now) -> ProviderSnapshot`.
//!
//! No I/O, no clock: `now` is injected, which is what lets a relative
//! `reset_after_seconds` and a derived pace be asserted against fixtures.
//!
//! # Windows are classified by duration, not by slot
//!
//! Codex normally returns the five-hour window as `primary_window` and the
//! weekly window as `secondary_window`, but it sometimes drops one limit and
//! promotes the weekly window into the primary slot. Reading the slot alone
//! would then label a weekly quota "Session" and mislead the user about when it
//! resets. So `limit_window_seconds` decides — `18000` is the session window,
//! `604800` the weekly one — and the historical slot order is used only for a
//! window whose duration is absent or unfamiliar.
//!
//! # Headers fill what the body omits
//!
//! `x-codex-primary-used-percent`, `x-codex-secondary-used-percent` and
//! `x-codex-credits-balance` restate values the body sometimes leaves out. A
//! reading recovered that way flips the snapshot's source to
//! [`UsageSourceKind::ResponseHeaders`], so the panel can say where the number
//! came from. The body always wins where both are present: a header can be a
//! stale echo of an earlier request.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::http::HttpResponse;
use crate::usage::model::{
    ProviderSnapshot, ReasonCode, UsageResource, UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// The session window's own duration, as Codex states it.
const SESSION_WINDOW_SECONDS: f64 = 18_000.0;
/// The weekly window's own duration.
const WEEKLY_WINDOW_SECONDS: f64 = 604_800.0;

const SESSION_PERIOD_MS: i64 = 18_000_000;
const WEEKLY_PERIOD_MS: i64 = 604_800_000;

const PRIMARY_PERCENT_HEADER: &str = "x-codex-primary-used-percent";
const SECONDARY_PERCENT_HEADER: &str = "x-codex-secondary-used-percent";
const CREDITS_BALANCE_HEADER: &str = "x-codex-credits-balance";

/// Resource ids, stable across refreshes because the UI keys rows by them.
pub const SESSION_ID: &str = "session";
pub const WEEKLY_ID: &str = "weekly";
pub const CREDITS_ID: &str = "credits";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WindowKind {
    Session,
    Weekly,
}

impl WindowKind {
    fn id(self) -> &'static str {
        match self {
            Self::Session => SESSION_ID,
            Self::Weekly => WEEKLY_ID,
        }
    }

    /// The window's length when the payload does not state one. This is the
    /// window's definition, not a measurement, so it is safe to supply — unlike
    /// a percentage, which is never invented.
    fn default_period_ms(self) -> i64 {
        match self {
            Self::Session => SESSION_PERIOD_MS,
            Self::Weekly => WEEKLY_PERIOD_MS,
        }
    }
}

/// One rate-limit window before it is assigned to a kind.
struct Candidate<'a> {
    /// The window object, when the body carried one.
    window: Option<&'a Value>,
    used_percent: Option<f64>,
    /// True when the percentage came from a response header.
    from_headers: bool,
    /// The kind this slot historically means, used only as a fallback.
    slot: WindowKind,
}

/// Normalise one usage response.
///
/// A 200 that carries none of the documented fields degrades to
/// [`ReasonCode::UnsupportedPayload`] — "no data" — rather than a fabricated
/// zero.
#[must_use]
pub fn map(payload: &Value, response: &HttpResponse, now: DateTime<Utc>) -> ProviderSnapshot {
    let rate_limit = payload.get("rate_limit");
    let candidates: Vec<Candidate<'_>> = [
        candidate(
            rate_limit.and_then(|limit| limit.get("primary_window")),
            response.header_f64(PRIMARY_PERCENT_HEADER),
            WindowKind::Session,
        ),
        candidate(
            rate_limit.and_then(|limit| limit.get("secondary_window")),
            response.header_f64(SECONDARY_PERCENT_HEADER),
            WindowKind::Weekly,
        ),
    ]
    .into_iter()
    .flatten()
    .collect();

    let mut resources = Vec::new();
    let mut from_headers = false;
    for kind in [WindowKind::Session, WindowKind::Weekly] {
        let Some(selected) = select(&candidates, kind) else {
            continue;
        };
        if let Some(resource) = window_resource(selected, kind, now) {
            from_headers |= selected.from_headers;
            resources.push(resource);
        }
    }
    if let Some((credits, credits_from_headers)) = credits_resource(payload, response) {
        from_headers |= credits_from_headers;
        resources.push(credits);
    }

    let plan = plan(payload);
    if resources.is_empty() {
        return ProviderSnapshot::unknown(PROVIDER_ID, ReasonCode::UnsupportedPayload, now)
            .with_plan(plan);
    }
    let source = if from_headers {
        UsageSourceKind::ResponseHeaders
    } else {
        UsageSourceKind::ProviderReported
    };
    ProviderSnapshot::ok(PROVIDER_ID, source, resources, now).with_plan(plan)
}

/// A window the body carried, a window only the headers know about, or nothing.
fn candidate(
    window: Option<&Value>,
    header_percent: Option<f64>,
    slot: WindowKind,
) -> Option<Candidate<'_>> {
    // `null` is how Codex says "this limit does not apply", so it is not an
    // object and not a window.
    let window = window.filter(|value| value.is_object());
    let body_percent = window
        .and_then(|value| value.get("used_percent"))
        .and_then(parse::number);
    if window.is_none() && header_percent.is_none() {
        return None;
    }
    Some(Candidate {
        window,
        used_percent: body_percent.or(header_percent),
        from_headers: body_percent.is_none() && header_percent.is_some(),
        slot,
    })
}

/// The candidate for `kind`: an exact duration match first, then the slot.
fn select<'a>(candidates: &'a [Candidate<'a>], kind: WindowKind) -> Option<&'a Candidate<'a>> {
    candidates
        .iter()
        .find(|candidate| exact_kind(candidate.window) == Some(kind))
        .or_else(|| {
            candidates
                .iter()
                .find(|candidate| exact_kind(candidate.window).is_none() && candidate.slot == kind)
        })
}

/// The kind a window's stated duration identifies, if it is one we know.
fn exact_kind(window: Option<&Value>) -> Option<WindowKind> {
    let seconds = window?
        .get("limit_window_seconds")
        .and_then(parse::number)?;
    if (seconds - SESSION_WINDOW_SECONDS).abs() < f64::EPSILON {
        Some(WindowKind::Session)
    } else if (seconds - WEEKLY_WINDOW_SECONDS).abs() < f64::EPSILON {
        Some(WindowKind::Weekly)
    } else {
        None
    }
}

fn window_resource(
    candidate: &Candidate<'_>,
    kind: WindowKind,
    now: DateTime<Utc>,
) -> Option<UsageResource> {
    // No percentage anywhere means no meter. A window with a reset but no usage
    // would otherwise render as a confident 0%.
    let used_percent = parse::clamp_percent(candidate.used_percent?)?;
    let period_ms = candidate
        .window
        .and_then(period_ms)
        .unwrap_or_else(|| kind.default_period_ms());
    Some(
        UsageResource::percent(kind.id(), Some(used_percent))
            .with_period_ms(Some(period_ms))
            .with_resets_at(
                candidate
                    .window
                    .and_then(|window| parse::parse_reset_at(window, now)),
            )
            .derive_pace(now),
    )
}

fn period_ms(window: &Value) -> Option<i64> {
    let seconds = window.get("limit_window_seconds").and_then(parse::number)?;
    if seconds <= 0.0 {
        return None;
    }
    #[allow(clippy::cast_possible_truncation)]
    Some((seconds * 1000.0).trunc() as i64)
}

/// The flex-credit balance, and whether it had to be read from a header.
fn credits_resource(payload: &Value, response: &HttpResponse) -> Option<(UsageResource, bool)> {
    let credits = payload.get("credits");
    let body_balance = credits
        .and_then(|value| value.get("balance"))
        .and_then(parse::number)
        .or_else(|| {
            // `has_credits: false` is a *measured* zero — the provider stated the
            // balance is empty — so it is reported rather than dropped as "no data".
            match credits
                .and_then(|value| value.get("has_credits"))
                .and_then(parse::boolean)
            {
                Some(false) => Some(0.0),
                Some(true) | None => None,
            }
        });
    let balance = body_balance.or_else(|| response.header_f64(CREDITS_BALANCE_HEADER))?;
    Some((
        UsageResource::balance(CREDITS_ID, UsageUnit::Credits, Some(balance)),
        body_balance.is_none(),
    ))
}

/// `plan_type` as the vendor's own naming, so a percentage can be interpreted.
fn plan(payload: &Value) -> Option<String> {
    let raw = payload.get("plan_type").and_then(parse::text)?;
    let formatted = match raw.to_ascii_lowercase().as_str() {
        "prolite" => "Pro 5x".to_string(),
        "pro" => "Pro 20x".to_string(),
        _ => parse::title_case(raw),
    };
    if formatted.is_empty() {
        None
    } else {
        Some(formatted)
    }
}

#[cfg(test)]
mod tests {
    //! Payload fixtures live in `tests/fixtures/usage/codex/`. They are modelled
    //! on `OpenUsage`'s own Codex tests and on the field list in
    //! `docs/specs/provider-usage.md` — evidence of field names, not a capture
    //! from a live account, so a real payload change still has to be re-verified
    //! against the endpoint.

    use super::{map, CREDITS_ID, SESSION_ID, WEEKLY_ID};
    use crate::usage::http::HttpResponse;
    use crate::usage::model::{
        Availability, ProviderSnapshot, ReasonCode, ResourceKind, UsageResource, UsageSourceKind,
        UsageUnit,
    };
    use autostand_core::pace::Pace;
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{json, Value};

    const USAGE_PRO: &str = include_str!("../../../tests/fixtures/usage/codex/usage-pro.json");
    const USAGE_WEEKLY_ONLY: &str =
        include_str!("../../../tests/fixtures/usage/codex/usage-weekly-only.json");
    const USAGE_HEADERS_ONLY: &str =
        include_str!("../../../tests/fixtures/usage/codex/usage-headers-only.json");

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    fn snapshot(payload: &Value, headers: &[(&str, &str)]) -> ProviderSnapshot {
        let response = HttpResponse::from_parts(200, headers, payload.to_string().as_bytes());
        map(payload, &response, now())
    }

    fn fixture(raw: &str, headers: &[(&str, &str)]) -> ProviderSnapshot {
        let payload: Value = serde_json::from_str(raw).expect("fixture is valid JSON");
        let response = HttpResponse::from_parts(200, headers, raw.as_bytes());
        map(&payload, &response, now())
    }

    fn resource<'a>(snapshot: &'a ProviderSnapshot, id: &str) -> Option<&'a UsageResource> {
        snapshot.resources.iter().find(|entry| entry.id == id)
    }

    #[test]
    fn a_full_payload_maps_windows_credits_and_plan() {
        let snapshot = fixture(USAGE_PRO, &[]);
        assert_eq!(snapshot.provider, "openai");
        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.source, UsageSourceKind::ProviderReported);
        assert_eq!(snapshot.plan.as_deref(), Some("Pro 20x"));
        assert_eq!(snapshot.checked_at, now());

        let session = resource(&snapshot, SESSION_ID).expect("session window");
        assert_eq!(session.used_percent, Some(42.0));
        assert_eq!(session.remaining_percent, Some(58.0));
        assert_eq!(session.period_duration_ms, Some(18_000_000));
        assert_eq!(session.unit, UsageUnit::Percent);
        assert_eq!(session.kind, ResourceKind::Consumption);

        let weekly = resource(&snapshot, WEEKLY_ID).expect("weekly window");
        assert_eq!(weekly.used_percent, Some(13.5));
        assert_eq!(weekly.period_duration_ms, Some(604_800_000));

        let credits = resource(&snapshot, CREDITS_ID).expect("credits");
        assert_eq!(credits.kind, ResourceKind::Balance);
        assert_eq!(credits.unit, UsageUnit::Credits);
        assert_eq!(credits.available, Some(821.0));
        assert_eq!(credits.used_percent, None);
    }

    #[test]
    fn a_weekly_window_promoted_into_the_primary_slot_is_still_weekly() {
        // The vendor drops the five-hour limit and moves the weekly one up.
        // Reading the slot would label a 7-day quota "Session".
        let snapshot = fixture(USAGE_WEEKLY_ONLY, &[]);
        assert!(resource(&snapshot, SESSION_ID).is_none());
        let weekly = resource(&snapshot, WEEKLY_ID).expect("weekly window");
        assert_eq!(weekly.used_percent, Some(5.0));
        assert_eq!(weekly.period_duration_ms, Some(604_800_000));
    }

    #[test]
    fn an_unrecognised_duration_falls_back_to_the_historical_slot_order() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 11, "limit_window_seconds": 86_400 },
                    "secondary_window": { "used_percent": 22, "limit_window_seconds": 2_592_000 }
                }
            }),
            &[],
        );
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().used_percent,
            Some(11.0)
        );
        assert_eq!(
            resource(&snapshot, WEEKLY_ID).unwrap().used_percent,
            Some(22.0)
        );
        // The stated duration is kept even when it does not name a known kind.
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().period_duration_ms,
            Some(86_400_000)
        );
    }

    #[test]
    fn a_window_without_a_stated_duration_keeps_its_kind_default() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 1, "reset_after_seconds": 18000 }
                }
            }),
            &[],
        );
        let session = resource(&snapshot, SESSION_ID).expect("session window");
        assert_eq!(session.used_percent, Some(1.0));
        assert_eq!(session.period_duration_ms, Some(18_000_000));
    }

    #[test]
    fn headers_fill_windows_the_body_omitted_and_say_so() {
        let snapshot = fixture(
            USAGE_HEADERS_ONLY,
            &[
                ("x-codex-primary-used-percent", "25"),
                ("x-codex-secondary-used-percent", "50"),
            ],
        );
        assert_eq!(snapshot.source, UsageSourceKind::ResponseHeaders);
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().used_percent,
            Some(25.0)
        );
        assert_eq!(
            resource(&snapshot, WEEKLY_ID).unwrap().used_percent,
            Some(50.0)
        );
        // No window object, so nothing to derive a reset or a pace from — and
        // neither is invented.
        assert_eq!(resource(&snapshot, SESSION_ID).unwrap().resets_at, None);
        assert_eq!(resource(&snapshot, SESSION_ID).unwrap().pace, None);
    }

    #[test]
    fn the_body_outranks_a_stale_header() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 0, "reset_after_seconds": 60 },
                    "secondary_window": { "used_percent": 7, "reset_after_seconds": 120 }
                }
            }),
            &[
                ("x-codex-primary-used-percent", "99"),
                ("x-codex-secondary-used-percent", "99"),
            ],
        );
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().used_percent,
            Some(0.0)
        );
        assert_eq!(
            resource(&snapshot, WEEKLY_ID).unwrap().used_percent,
            Some(7.0)
        );
        assert_eq!(snapshot.source, UsageSourceKind::ProviderReported);
    }

    #[test]
    fn a_credits_balance_recovered_from_a_header_marks_the_source() {
        let snapshot = snapshot(
            &json!({ "rate_limit": { "primary_window": { "used_percent": 5 } } }),
            &[("x-codex-credits-balance", "128")],
        );
        assert_eq!(
            resource(&snapshot, CREDITS_ID).unwrap().available,
            Some(128.0)
        );
        assert_eq!(snapshot.source, UsageSourceKind::ResponseHeaders);
    }

    #[test]
    fn a_credit_balance_the_body_states_as_empty_is_a_measured_zero() {
        // `has_credits: false` is a fact the provider sent, unlike a missing
        // field — so it reads "0", not "No data".
        let snapshot = snapshot(
            &json!({ "credits": { "has_credits": false } }),
            &[("x-codex-credits-balance", "99")],
        );
        assert_eq!(
            resource(&snapshot, CREDITS_ID).unwrap().available,
            Some(0.0)
        );
    }

    #[test]
    fn a_numeric_string_balance_still_parses() {
        let snapshot = snapshot(&json!({ "credits": { "balance": "100" } }), &[]);
        assert_eq!(
            resource(&snapshot, CREDITS_ID).unwrap().available,
            Some(100.0)
        );
        assert_eq!(snapshot.source, UsageSourceKind::ProviderReported);
    }

    #[test]
    fn an_absent_credits_object_produces_no_row_rather_than_zero() {
        let snapshot = snapshot(
            &json!({ "rate_limit": { "primary_window": { "used_percent": 5 } } }),
            &[],
        );
        assert!(resource(&snapshot, CREDITS_ID).is_none());
    }

    #[test]
    fn an_absolute_reset_is_read_from_the_epoch_the_window_states() {
        let resets_at = now() + chrono::Duration::seconds(3_600);
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 30,
                        "limit_window_seconds": 18000,
                        "reset_at": resets_at.timestamp(),
                        "reset_after_seconds": 9
                    }
                }
            }),
            &[],
        );
        // Absolute wins over relative: it survives clock skew.
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().resets_at,
            Some(resets_at)
        );
    }

    #[test]
    fn a_relative_reset_is_resolved_against_the_injected_now() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 30, "reset_after_seconds": 600 }
                }
            }),
            &[],
        );
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().resets_at,
            Some(now() + chrono::Duration::seconds(600))
        );
    }

    #[test]
    fn pace_is_projected_once_the_window_has_run_long_enough() {
        // Half of the five-hour window gone with 10% used projects to 20%.
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": {
                        "used_percent": 10,
                        "limit_window_seconds": 18000,
                        "reset_after_seconds": 9000
                    }
                }
            }),
            &[],
        );
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().pace,
            Some(Pace::Ahead)
        );
    }

    #[test]
    fn out_of_range_percentages_are_clamped_never_dropped() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 140, "limit_window_seconds": 18_000 },
                    "secondary_window": { "used_percent": -5, "limit_window_seconds": 604_800 }
                }
            }),
            &[],
        );
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().used_percent,
            Some(100.0)
        );
        assert_eq!(
            resource(&snapshot, WEEKLY_ID).unwrap().used_percent,
            Some(0.0)
        );
    }

    #[test]
    fn a_window_with_a_reset_but_no_percentage_is_no_data_not_zero() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "limit_window_seconds": 18000, "reset_after_seconds": 60 }
                }
            }),
            &[],
        );
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UnsupportedPayload));
        assert!(snapshot.resources.is_empty());
    }

    #[test]
    fn a_null_window_is_absent_rather_than_empty() {
        let snapshot = snapshot(
            &json!({
                "rate_limit": {
                    "primary_window": { "used_percent": 12, "limit_window_seconds": 18000 },
                    "secondary_window": null
                }
            }),
            &[],
        );
        assert!(resource(&snapshot, WEEKLY_ID).is_none());
        assert_eq!(
            resource(&snapshot, SESSION_ID).unwrap().used_percent,
            Some(12.0)
        );
    }

    #[test]
    fn an_empty_payload_degrades_to_no_data() {
        for payload in [json!({}), json!({ "rate_limit": {} }), json!([])] {
            let snapshot = snapshot(&payload, &[]);
            assert_eq!(snapshot.availability, Availability::Unknown, "{payload}");
            assert_eq!(snapshot.reason, Some(ReasonCode::UnsupportedPayload));
            assert!(snapshot.resources.is_empty());
            assert_eq!(snapshot.source, UsageSourceKind::Unknown);
        }
    }

    #[test]
    fn plan_names_follow_the_vendor_wording() {
        for (raw, expected) in [
            ("prolite", Some("Pro 5x")),
            ("pro", Some("Pro 20x")),
            ("PRO", Some("Pro 20x")),
            ("team_business", Some("Team Business")),
            ("plus", Some("Plus")),
        ] {
            let snapshot = snapshot(&json!({ "plan_type": raw }), &[]);
            assert_eq!(snapshot.plan.as_deref(), expected, "{raw}");
        }
        assert_eq!(snapshot(&json!({ "plan_type": "" }), &[]).plan, None);
        assert_eq!(snapshot(&json!({}), &[]).plan, None);
    }

    #[test]
    fn a_plan_survives_a_payload_that_carries_no_usage() {
        // The row still says which subscription it could not measure.
        let snapshot = snapshot(&json!({ "plan_type": "pro" }), &[]);
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.plan.as_deref(), Some("Pro 20x"));
    }
}
