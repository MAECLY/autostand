//! Pure mapping of Copilot's seat and org-billing payloads.
//!
//! No I/O and no clock: `now` is injected, so every branch below is reachable
//! from a fixture.
//!
//! Since usage-based billing arrived, every plan meters a `premium_interactions`
//! pool, surfaced here as `premiumCredits`, with `extraUsage` carrying overage
//! beyond it. Paid plans report `chat`/`completions` as the `-1` "unlimited"
//! sentinel, which is suppressed rather than drawn as a misleading `0%`; free
//! plans carry real counts, either in `quota_snapshots` or, on older responses,
//! as `limited_user_quotas` against `monthly_quotas`.
//!
//! A Copilot Business seat managed by an organization reports a zero-entitlement
//! placeholder for every bucket. That is a legitimate empty state, not a
//! failure: the plan is surfaced with no meters and the caller is told to look
//! at organization billing instead.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use serde_json::Value;

use crate::usage::model::{
    Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError, UsageResource,
    UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// Copilot's quota windows are monthly.
const MONTH_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// GitHub's "unlimited" sentinel on an entitlement or a remaining count.
const UNLIMITED_SENTINEL: f64 = -1.0;

/// Copilot's own failure modes on top of the shared ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CopilotProbeError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// The response parsed but carried no quota and no org-managed marker.
    #[error("quota unavailable")]
    QuotaUnavailable,
}

impl ProbeFailure for CopilotProbeError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm.
        match self {
            Self::Usage(error) => error.classify(),
            Self::QuotaUnavailable => (Availability::Unknown, ReasonCode::UnsupportedPayload),
        }
    }
}

/// A mapped seat: the snapshot, plus whether its usage lives in org billing.
#[derive(Debug, Clone, PartialEq)]
pub struct CopilotSeat {
    pub snapshot: ProviderSnapshot,
    /// True for an org-managed seat whose response carried no per-seat meters.
    ///
    /// Kept as an explicit flag rather than inferred from an empty resource
    /// list: a placeholder bucket carrying `overage_permitted` used to sneak a
    /// meaningless `extraUsage: 0` row in and block the org lookup.
    pub org_managed: bool,
}

/// Map a `/copilot_internal/user` body.
pub fn map(body: &Value, now: DateTime<Utc>) -> Result<CopilotSeat, CopilotProbeError> {
    let plan = body
        .get("copilot_plan")
        .and_then(parse::text)
        .map(parse::title_case);
    let resets_at = reset_date(body.get("quota_reset_date"))
        .or_else(|| reset_date(body.get("limited_user_reset_date")));

    let mut resources = Vec::new();
    let snapshots = body.get("quota_snapshots");
    let premium = snapshots.and_then(|value| value.get("premium_interactions"));

    // Overage only exists relative to an included pool, so it is tied to the
    // credits meter: an org placeholder can carry `overage_permitted: true` on a
    // zero-entitlement bucket, and rendering `0` for that would mean nothing.
    if let Some(credits) = percent_bucket("premiumCredits", premium, resets_at, now) {
        resources.push(credits);
        if let Some(overage) = overage(premium) {
            resources.push(overage);
        }
    }

    for (id, key) in [("chat", "chat"), ("completions", "completions")] {
        let bucket = snapshots.and_then(|value| value.get(key));
        if let Some(resource) = percent_bucket(id, bucket, resets_at, now) {
            resources.push(resource);
        }
    }

    // The legacy free-tier shape predates `quota_snapshots`. It is gated on
    // nothing else having been produced: a paid account with credits present and
    // chat/completions suppressed as unlimited would otherwise show free-tier
    // meters alongside its credits.
    if resources.is_empty() {
        let limited = body.get("limited_user_quotas");
        let monthly = body.get("monthly_quotas");
        for (id, key) in [("chat", "chat"), ("completions", "completions")] {
            let remaining = limited.and_then(|value| value.get(key));
            let total = monthly.and_then(|value| value.get(key));
            if let Some(resource) = remaining_of_total(id, remaining, total, resets_at, now) {
                resources.push(resource);
            }
        }
    }

    if resources.is_empty() {
        // A Business / token-based-billing seat exposes no per-seat quota: a
        // legitimate empty state whose real usage lives in org billing. Anything
        // else with no meters is a payload we do not understand.
        if body.get("token_based_billing").and_then(parse::boolean) == Some(true) {
            return Ok(CopilotSeat {
                snapshot: ProviderSnapshot::ok(
                    PROVIDER_ID,
                    UsageSourceKind::ProviderReported,
                    Vec::new(),
                    now,
                )
                .with_plan(plan),
                org_managed: true,
            });
        }
        return Err(CopilotProbeError::QuotaUnavailable);
    }

    Ok(CopilotSeat {
        snapshot: ProviderSnapshot::ok(
            PROVIDER_ID,
            UsageSourceKind::ProviderReported,
            resources,
            now,
        )
        .with_plan(plan),
        org_managed: false,
    })
}

/// Organization slugs from a `/user/orgs` body, in GitHub's order.
#[must_use]
pub fn org_logins(body: &Value) -> Vec<String> {
    body.as_array()
        .map(|entries| {
            entries
                .iter()
                .filter_map(|entry| entry.get("login").and_then(parse::text))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Org-level Copilot meters from a billing usage summary.
///
/// `None` when the summary carries no Copilot AI-credit items — the org does not
/// use Copilot credits, so the caller keeps probing other organizations. Only
/// credit-unit items count, so a seat-fee line item cannot pollute the totals.
///
/// The endpoint exposes no allotment, so these are consumed totals with no
/// limit: a percentage would have to be invented, and it is not.
#[must_use]
pub fn org_billing_resources(body: &Value) -> Option<Vec<UsageResource>> {
    let items = body.get("usageItems")?.as_array()?;
    let credit_items: Vec<&Value> = items
        .iter()
        .filter(|item| is_copilot(item) && is_credit_unit(item))
        .collect();
    if credit_items.is_empty() {
        return None;
    }

    let credits = sum_non_negative(&credit_items, "grossQuantity");
    let spend = sum_non_negative(&credit_items, "netAmount");
    Some(vec![
        UsageResource::consumption("orgCredits", UsageUnit::Credits).with_used(Some(credits)),
        UsageResource::consumption("orgSpend", UsageUnit::Usd).with_used(Some(spend)),
    ])
}

fn sum_non_negative(items: &[&Value], key: &str) -> f64 {
    items
        .iter()
        .filter_map(|item| item.get(key).and_then(parse::number))
        .map(|value| value.max(0.0))
        .sum()
}

fn is_copilot(item: &Value) -> bool {
    item.get("product")
        .and_then(parse::text)
        .is_some_and(|product| product.eq_ignore_ascii_case("copilot"))
}

fn is_credit_unit(item: &Value) -> bool {
    item.get("unitType")
        .and_then(parse::text)
        .is_some_and(|unit| {
            let unit = unit.to_ascii_lowercase();
            unit == "ai-units" || unit == "ai-credits"
        })
}

/// A `quota_snapshots` bucket as a percent-used meter, or `None` to suppress it.
///
/// Suppressed for a missing bucket; for an `unlimited` bucket or the `-1`
/// sentinel (paid chat and completions carry no real meter, so they are hidden
/// rather than shown as `0%`); and for a zero-entitlement placeholder, which is
/// an org-managed stub or credits on a free account — no allotment to measure.
fn percent_bucket(
    id: &str,
    bucket: Option<&Value>,
    resets_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<UsageResource> {
    let bucket = bucket?;
    if !bucket.is_object() {
        return None;
    }
    let entitlement = bucket.get("entitlement").and_then(parse::number);
    let remaining = bucket.get("remaining").and_then(parse::number);

    if bucket.get("unlimited").and_then(parse::boolean) == Some(true)
        || entitlement == Some(UNLIMITED_SENTINEL)
        || remaining == Some(UNLIMITED_SENTINEL)
        || entitlement == Some(0.0)
    {
        return None;
    }

    // GitHub states the percentage itself on most responses; where it does not,
    // it is arithmetic on two figures GitHub did send. A bucket with neither is
    // "No data", never `0%`.
    let percent_remaining = match bucket.get("percent_remaining").and_then(parse::number) {
        Some(percent_remaining) => percent_remaining,
        None => remaining? / entitlement.filter(|value| *value > 0.0)? * 100.0,
    };
    let used_percent = parse::clamp_percent(100.0 - percent_remaining)?;

    Some(
        UsageResource::percent(id, Some(used_percent))
            .with_resets_at(resets_at)
            .with_period_ms(Some(MONTH_MS))
            .derive_projection(now),
    )
}

/// Premium interactions consumed beyond the included pool.
///
/// Emitted only once the user has enabled overage spend; a real zero is then
/// shown, because it is measured. With overage off the figure is genuinely not
/// applicable and stays absent. No spending cap is exposed here, so this is an
/// unbounded count rather than a meter.
fn overage(bucket: Option<&Value>) -> Option<UsageResource> {
    let bucket = bucket?;
    if bucket.get("overage_permitted").and_then(parse::boolean) != Some(true) {
        return None;
    }
    let count = bucket
        .get("overage_count")
        .and_then(parse::number)
        .unwrap_or(0.0)
        .max(0.0);
    Some(UsageResource::consumption("extraUsage", UsageUnit::Count).with_used(Some(count)))
}

/// A free-tier bucket: `remaining` against a monthly `total`.
///
/// `None` unless both a positive total and a remaining count are present — with
/// no denominator there is no honest percentage.
fn remaining_of_total(
    id: &str,
    remaining: Option<&Value>,
    total: Option<&Value>,
    resets_at: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<UsageResource> {
    let total = total.and_then(parse::number).filter(|value| *value > 0.0)?;
    let remaining = remaining.and_then(parse::number)?;
    let used_percent = parse::clamp_percent(((total - remaining).max(0.0) / total) * 100.0)?;
    Some(
        UsageResource::percent(id, Some(used_percent))
            .with_resets_at(resets_at)
            .with_period_ms(Some(MONTH_MS))
            .derive_projection(now),
    )
}

/// A reset timestamp.
///
/// Paid tier sends a full RFC 3339 instant; the free tier sends a bare
/// `yyyy-mm-dd`, which the shared parser rejects, so that one Copilot-specific
/// shape is handled here.
fn reset_date(value: Option<&Value>) -> Option<DateTime<Utc>> {
    let raw = value.and_then(parse::text)?;
    if let Some(parsed) = parse::parse_rfc3339(raw) {
        return Some(parsed);
    }
    let day = NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()?;
    Some(Utc.from_utc_datetime(&day.and_hms_opt(0, 0, 0)?))
}

#[cfg(test)]
mod tests {
    use super::{map, org_billing_resources, org_logins, CopilotProbeError, MONTH_MS};
    use crate::usage::model::{
        Availability, ProbeFailure, ReasonCode, UsageResource, UsageSourceKind, UsageUnit,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{json, Value};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap()
    }

    fn paid_body() -> Value {
        json!({
            "copilot_plan": "pro",
            "quota_reset_date": "2099-01-15T00:00:00Z",
            "quota_snapshots": {
                "premium_interactions": { "entitlement": 300, "remaining": 123, "percent_remaining": 41, "quota_id": "premium" },
                "chat": { "entitlement": 1000, "remaining": 950, "percent_remaining": 95, "quota_id": "chat" }
            }
        })
    }

    fn org_summary_body() -> Value {
        json!({
            "timePeriod": { "year": 2026, "month": 7 },
            "organization": "acme",
            "usageItems": [{
                "product": "Copilot",
                "sku": "copilot_ai_unit",
                "unitType": "ai-units",
                "pricePerUnit": 0.01,
                "grossQuantity": 298.698_546,
                "grossAmount": 2.986_985_46,
                "netQuantity": 0.0,
                "netAmount": 0.0
            }]
        })
    }

    fn find<'a>(resources: &'a [UsageResource], id: &str) -> Option<&'a UsageResource> {
        resources.iter().find(|resource| resource.id == id)
    }

    #[test]
    fn maps_paid_credits_and_chat_as_percent_used() {
        let seat = map(&paid_body(), now()).unwrap();

        assert_eq!(seat.snapshot.plan.as_deref(), Some("Pro"));
        assert!(!seat.org_managed);
        let credits = find(&seat.snapshot.resources, "premiumCredits").unwrap();
        assert_eq!(credits.used_percent, Some(59.0));
        assert_eq!(credits.period_duration_ms, Some(MONTH_MS));
        assert!(credits.resets_at.is_some());
        assert_eq!(
            find(&seat.snapshot.resources, "chat").unwrap().used_percent,
            Some(5.0)
        );
    }

    #[test]
    fn unlimited_and_sentinel_buckets_are_suppressed_rather_than_drawn_as_zero() {
        let mut body = paid_body();
        body["quota_snapshots"]["chat"] =
            json!({ "unlimited": true, "entitlement": 0, "remaining": 0 });
        body["quota_snapshots"]["completions"] = json!({ "entitlement": -1, "remaining": -1 });

        let seat = map(&body, now()).unwrap();

        assert!(find(&seat.snapshot.resources, "chat").is_none());
        assert!(find(&seat.snapshot.resources, "completions").is_none());
        assert!(find(&seat.snapshot.resources, "premiumCredits").is_some());
    }

    #[test]
    fn extra_usage_appears_only_when_overage_is_enabled_and_shows_a_real_zero() {
        assert!(find(
            &map(&paid_body(), now()).unwrap().snapshot.resources,
            "extraUsage"
        )
        .is_none());

        let mut body = paid_body();
        body["quota_snapshots"]["premium_interactions"]["overage_permitted"] = json!(true);
        body["quota_snapshots"]["premium_interactions"]["overage_count"] = json!(36);
        let with_overage = map(&body, now()).unwrap();
        let extra = find(&with_overage.snapshot.resources, "extraUsage").unwrap();
        assert_eq!(extra.used, Some(36.0));
        assert_eq!(extra.unit, UsageUnit::Count);
        // Unbounded: GitHub exposes no spending cap here, so no limit is invented.
        assert_eq!(extra.limit, None);

        body["quota_snapshots"]["premium_interactions"]["overage_count"] = json!(0);
        let unused = map(&body, now()).unwrap();
        assert_eq!(
            find(&unused.snapshot.resources, "extraUsage").unwrap().used,
            Some(0.0)
        );
    }

    #[test]
    fn a_zero_entitlement_placeholder_never_becomes_a_zero_percent_meter() {
        let body = json!({
            "copilot_plan": "business",
            "quota_snapshots": {
                "premium_interactions": { "entitlement": 0, "remaining": 0, "percent_remaining": 100 },
                "chat": { "entitlement": 1000, "remaining": 800, "percent_remaining": 80 }
            }
        });

        let seat = map(&body, now()).unwrap();

        assert!(find(&seat.snapshot.resources, "premiumCredits").is_none());
        assert_eq!(
            find(&seat.snapshot.resources, "chat").unwrap().used_percent,
            Some(20.0)
        );
    }

    #[test]
    fn a_free_account_renders_chat_and_completions_and_no_credits() {
        let body = json!({
            "copilot_plan": "individual",
            "token_based_billing": true,
            "quota_reset_date": "2099-07-01",
            "quota_snapshots": {
                "chat": { "entitlement": 200, "remaining": 182, "percent_remaining": 91.0, "overage_permitted": false },
                "completions": { "entitlement": 2000, "remaining": 1989, "percent_remaining": 99.4, "overage_permitted": false },
                "premium_interactions": { "entitlement": 0, "remaining": 0, "percent_remaining": 0.0, "overage_permitted": false }
            }
        });

        let seat = map(&body, now()).unwrap();

        assert_eq!(seat.snapshot.plan.as_deref(), Some("Individual"));
        assert!(find(&seat.snapshot.resources, "premiumCredits").is_none());
        assert!(find(&seat.snapshot.resources, "extraUsage").is_none());
        let chat = find(&seat.snapshot.resources, "chat").unwrap();
        assert!((chat.used_percent.unwrap() - 9.0).abs() < 1e-9);
        // A bare `yyyy-mm-dd` reset is the free tier's shape and must still parse.
        assert!(chat.resets_at.is_some());
    }

    #[test]
    fn the_legacy_free_shape_maps_remaining_against_a_monthly_total() {
        let body = json!({
            "copilot_plan": "individual",
            "limited_user_quotas": { "chat": 250, "completions": 2000 },
            "monthly_quotas": { "chat": 500, "completions": 4000 },
            "limited_user_reset_date": "2099-02-15"
        });

        let seat = map(&body, now()).unwrap();

        assert_eq!(
            find(&seat.snapshot.resources, "chat").unwrap().used_percent,
            Some(50.0)
        );
        assert_eq!(
            find(&seat.snapshot.resources, "completions")
                .unwrap()
                .used_percent,
            Some(50.0)
        );
        assert!(find(&seat.snapshot.resources, "chat")
            .unwrap()
            .resets_at
            .is_some());
    }

    #[test]
    fn the_legacy_shape_never_shadows_a_real_credits_meter() {
        let mut body = paid_body();
        body["quota_snapshots"]["chat"] = json!({ "entitlement": -1, "remaining": -1 });
        body["quota_snapshots"]["completions"] = json!({ "entitlement": -1, "remaining": -1 });
        body["limited_user_quotas"] = json!({ "chat": 100, "completions": 1000 });
        body["monthly_quotas"] = json!({ "chat": 500, "completions": 4000 });

        let seat = map(&body, now()).unwrap();

        assert!(find(&seat.snapshot.resources, "premiumCredits").is_some());
        assert!(find(&seat.snapshot.resources, "chat").is_none());
        assert!(find(&seat.snapshot.resources, "completions").is_none());
    }

    #[test]
    fn an_org_managed_seat_keeps_its_plan_and_asks_for_the_org_lookup() {
        // The placeholder carries `overage_permitted: true` on a zero-entitlement
        // bucket: it must neither produce a meaningless extra-usage row nor stop
        // the org lookup.
        let body = json!({
            "copilot_plan": "business",
            "token_based_billing": true,
            "quota_snapshots": {
                "premium_interactions": {
                    "entitlement": 0, "remaining": 0, "unlimited": true,
                    "overage_permitted": true, "overage_count": 0
                }
            }
        });

        let seat = map(&body, now()).unwrap();

        assert!(seat.org_managed);
        assert_eq!(seat.snapshot.plan.as_deref(), Some("Business"));
        assert!(seat.snapshot.resources.is_empty());
        assert_eq!(seat.snapshot.availability, Availability::Available);
    }

    #[test]
    fn a_body_with_no_meters_and_no_org_marker_degrades_instead_of_guessing() {
        let error = map(&json!({ "copilot_plan": "pro" }), now()).unwrap_err();
        assert_eq!(error, CopilotProbeError::QuotaUnavailable);
        assert_eq!(
            error.classify(),
            (Availability::Unknown, ReasonCode::UnsupportedPayload)
        );
    }

    #[test]
    fn org_logins_survive_a_garbled_entry_and_a_garbled_body() {
        let body = json!([{ "login": "acme", "id": 1 }, { "login": "globex" }, { "id": 3 }]);
        assert_eq!(org_logins(&body), vec!["acme", "globex"]);
        assert!(org_logins(&json!({ "not": "an array" })).is_empty());
    }

    #[test]
    fn org_billing_sums_credit_items_only() {
        let resources = org_billing_resources(&org_summary_body()).unwrap();
        let credits = resources.iter().find(|r| r.id == "orgCredits").unwrap();
        assert!((credits.used.unwrap() - 298.698_546).abs() < 1e-6);
        assert_eq!(credits.unit, UsageUnit::Credits);
        // No allotment is exposed, so no limit and no percentage are invented.
        assert_eq!(credits.limit, None);
        assert_eq!(credits.used_percent, None);
        assert_eq!(
            resources.iter().find(|r| r.id == "orgSpend").unwrap().used,
            Some(0.0)
        );
    }

    #[test]
    fn org_billing_adds_up_several_credit_items() {
        let mut body = org_summary_body();
        body["usageItems"] = json!([
            { "product": "Copilot", "unitType": "ai-units", "grossQuantity": 100.5, "netAmount": 1.25 },
            { "product": "Copilot", "unitType": "ai-credits", "grossQuantity": 50, "netAmount": 0.5 }
        ]);

        let resources = org_billing_resources(&body).unwrap();

        assert!((resources[0].used.unwrap() - 150.5).abs() < 1e-9);
        assert!((resources[1].used.unwrap() - 1.75).abs() < 1e-9);
    }

    #[test]
    fn seat_fees_and_other_products_produce_no_org_meters() {
        let mut body = org_summary_body();
        body["usageItems"] = json!([
            { "product": "Actions", "unitType": "minutes", "grossQuantity": 120, "netAmount": 0.96 },
            { "product": "Copilot", "unitType": "user-months", "grossQuantity": 10, "netAmount": 190 }
        ]);

        assert!(org_billing_resources(&body).is_none());
        assert!(org_billing_resources(&json!({ "organization": "acme" })).is_none());
    }

    #[test]
    fn a_failure_never_carries_free_text_from_the_payload() {
        assert_eq!(
            CopilotProbeError::QuotaUnavailable.to_string(),
            "quota unavailable"
        );
    }

    #[test]
    fn the_seat_snapshot_is_marked_as_provider_reported() {
        assert_eq!(
            map(&paid_body(), now()).unwrap().snapshot.source,
            UsageSourceKind::ProviderReported
        );
    }
}
