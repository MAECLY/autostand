//! Pure mapping of Cursor's usage payloads.
//!
//! No I/O and no clock: `now` is injected, so every branch is reachable from a
//! fixture.
//!
//! Cursor reports the same account three different ways, and the shape decides
//! the unit of `totalUsage`:
//!
//! | Account | `totalUsage` |
//! | --- | --- |
//! | Individual, spend-metered | percent of the plan allowance |
//! | Team / pooled | dollars spent of a dollar cap |
//! | Request-metered | requests of an included allowance |
//!
//! That is why the resource carries an explicit [`UsageUnit`] rather than being
//! assumed to be a percentage. Money arrives in **cents** throughout and is
//! converted once, at the point of construction.

use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::usage::model::{
    Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError, UsageResource,
    UsageSourceKind, UsageUnit,
};
use crate::usage::parse;

use super::PROVIDER_ID;

/// Cursor bills monthly; this is the window used whenever the payload carries no
/// explicit cycle bounds.
const BILLING_PERIOD_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// Cursor's own failure modes on top of the shared ones.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum CursorProbeError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// The account is disabled or carries no plan at all.
    #[error("no active subscription")]
    NoActiveSubscription,
    /// A plan whose allowance the payload never states — there is no
    /// denominator, so there is no honest meter.
    #[error("usage limit missing")]
    UsageLimitMissing,
}

impl ProbeFailure for CursorProbeError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm.
        match self {
            Self::Usage(error) => error.classify(),
            // Both remaining variants mean "we could not measure", never "you
            // are out": absence of a denominator is not exhaustion.
            Self::NoActiveSubscription | Self::UsageLimitMissing => {
                (Availability::Unknown, ReasonCode::UnsupportedPayload)
            }
        }
    }
}

/// Everything the three usage decisions read off the payload, decoded once.
///
/// The map guard, the request-based fallback and the generic fallback all key on
/// these same facts; reading them in one place is what keeps the three from
/// drifting apart.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlanUsageFacts {
    /// `enabled` is only "off" when explicitly `false`; absent reads as enabled.
    pub is_enabled: bool,
    pub has_plan_usage: bool,
    pub limit_cents: Option<f64>,
    pub total_percent_used: Option<f64>,
    pub is_team_by_shape: bool,
}

impl PlanUsageFacts {
    /// Decode the facts from a `GetCurrentPeriodUsage` body.
    #[must_use]
    pub fn read(usage: &Value) -> Self {
        let plan_usage = usage.get("planUsage").filter(|value| value.is_object());
        let spend_limit = usage.get("spendLimitUsage");
        let pooled_limit = spend_limit
            .and_then(|value| value.get("pooledLimit"))
            .and_then(parse::number)
            .unwrap_or(0.0);
        let limit_type = spend_limit
            .and_then(|value| value.get("limitType"))
            .and_then(parse::text)
            .map(str::to_ascii_lowercase);

        Self {
            is_enabled: usage.get("enabled").and_then(parse::boolean) != Some(false),
            has_plan_usage: plan_usage.is_some(),
            limit_cents: plan_usage
                .and_then(|value| value.get("limit"))
                .and_then(parse::number),
            total_percent_used: plan_usage
                .and_then(|value| value.get("totalPercentUsed"))
                .and_then(parse::number),
            is_team_by_shape: limit_type.as_deref() == Some("team") || pooled_limit > 0.0,
        }
    }

    /// A `planUsage` that exists but states no allowance.
    #[must_use]
    pub fn plan_usage_limit_missing(self) -> bool {
        self.has_plan_usage && self.limit_cents.is_none()
    }

    /// A `planUsage` that is absent, or present but unusable.
    #[must_use]
    pub fn plan_usage_unusable(self) -> bool {
        !self.has_plan_usage || self.plan_usage_limit_missing()
    }

    /// An enabled account whose `planUsage` carries neither an allowance nor a
    /// total percentage: worth asking the request-metered endpoint.
    #[must_use]
    pub fn should_try_request_endpoint(self) -> bool {
        self.is_enabled
            && self.has_plan_usage
            && self.plan_usage_limit_missing()
            && self.total_percent_used.is_none()
    }
}

/// The payloads one Cursor refresh gathered.
///
/// Every optional endpoint is exactly that: a failure to fetch it costs one
/// resource, never the snapshot.
#[derive(Debug, Clone, Copy)]
pub struct CursorPayloads<'a> {
    /// `GetCurrentPeriodUsage`, the only required payload.
    pub usage: &'a Value,
    pub plan_name: Option<&'a str>,
    /// `GetCreditGrantsBalance`.
    pub credit_grants: Option<&'a Value>,
    /// Prepaid balance, already normalised to a non-negative number of cents.
    pub stripe_balance_cents: f64,
}

/// Map the primary usage payload.
pub fn map(
    payloads: &CursorPayloads<'_>,
    now: DateTime<Utc>,
) -> Result<ProviderSnapshot, CursorProbeError> {
    let usage = payloads.usage;
    let facts = PlanUsageFacts::read(usage);
    let plan_usage = usage.get("planUsage").filter(|value| value.is_object());
    if !facts.is_enabled || plan_usage.is_none() {
        return Err(CursorProbeError::NoActiveSubscription);
    }
    if facts.limit_cents.is_none() && facts.total_percent_used.is_none() {
        return Err(CursorProbeError::UsageLimitMissing);
    }
    let plan_usage = plan_usage.unwrap_or(&Value::Null);
    let cycle = billing_cycle(usage);

    let mut resources = Vec::new();
    if let Some(credits) = credits(payloads.credit_grants, payloads.stripe_balance_cents) {
        resources.push(credits);
    }
    resources.push(total_usage(
        &facts,
        plan_usage,
        payloads.plan_name,
        cycle,
        now,
    )?);
    for (id, key) in [
        ("autoUsage", "autoPercentUsed"),
        ("apiUsage", "apiPercentUsed"),
    ] {
        if let Some(percent) = plan_usage.get(key).and_then(parse::number) {
            resources.push(
                UsageResource::percent(id, parse::clamp_percent(percent))
                    .with_resets_at(cycle.resets_at)
                    .with_period_ms(Some(cycle.period_ms))
                    .derive_projection(now),
            );
        }
    }
    if let Some(on_demand) = on_demand(usage.get("spendLimitUsage")) {
        resources.push(on_demand);
    }

    Ok(ProviderSnapshot::ok(
        PROVIDER_ID,
        UsageSourceKind::ProviderReported,
        resources,
        now,
    )
    .with_plan(payloads.plan_name.map(parse::title_case)))
}

/// Map the request-metered payload from `cursor.com/api/usage`.
pub fn map_request_based(
    usage: &Value,
    plan_name: Option<&str>,
    now: DateTime<Utc>,
) -> Result<ProviderSnapshot, CursorProbeError> {
    let bucket = usage
        .get("gpt-4")
        .ok_or(CursorProbeError::UsageLimitMissing)?;
    let limit = bucket
        .get("maxRequestUsage")
        .and_then(parse::number)
        .filter(|value| *value > 0.0)
        .ok_or(CursorProbeError::UsageLimitMissing)?;
    let used = bucket
        .get("numRequests")
        .and_then(parse::number)
        .or_else(|| bucket.get("numRequestsTotal").and_then(parse::number))
        .unwrap_or(0.0)
        .max(0.0);
    let resets_at = usage
        .get("startOfMonth")
        .and_then(parse::text)
        .and_then(parse::parse_rfc3339)
        .and_then(|start| {
            start.checked_add_signed(chrono::Duration::milliseconds(BILLING_PERIOD_MS))
        });

    let requests = bounded("requests", UsageUnit::Requests, used, limit)
        .with_resets_at(resets_at)
        .with_period_ms(Some(BILLING_PERIOD_MS))
        .derive_projection(now);

    Ok(ProviderSnapshot::ok(
        PROVIDER_ID,
        UsageSourceKind::ProviderReported,
        vec![requests],
        now,
    )
    .with_plan(plan_name.map(parse::title_case)))
}

/// Whether this account's usage lives at the request-metered endpoint instead.
///
/// Enterprise and team accounts report a `planUsage` with no allowance; so does
/// an account whose plan metadata could not be read at all. In each case the
/// spend-shaped mapping has no denominator and the request endpoint does.
#[must_use]
pub fn should_use_request_fallback(
    usage: &Value,
    plan_name: Option<&str>,
    plan_info_unavailable: bool,
) -> bool {
    let facts = PlanUsageFacts::read(usage);
    if !facts.is_enabled {
        return false;
    }
    let plan = plan_name.unwrap_or_default().trim().to_ascii_lowercase();
    if facts.plan_usage_unusable() && (plan == "enterprise" || plan == "team") {
        return true;
    }
    if facts.plan_usage_unusable()
        && facts.total_percent_used.is_none()
        && plan.is_empty()
        && plan_info_unavailable
    {
        return true;
    }
    facts.is_team_by_shape && facts.plan_usage_limit_missing()
}

/// The plan name from a `GetPlanInfo` body.
#[must_use]
pub fn plan_name(body: &Value) -> Option<&str> {
    body.get("planInfo")?.get("planName").and_then(parse::text)
}

/// The prepaid balance, in cents.
///
/// The billing provider reports a credit as a *negative* customer balance; any
/// other value means there is no prepaid credit to show.
#[must_use]
pub fn stripe_balance_cents(body: &Value) -> f64 {
    body.get("customerBalance")
        .and_then(parse::number)
        .filter(|balance| *balance < 0.0)
        .map_or(0.0, f64::abs)
}

/// `totalUsage`, in whichever unit this account shape reports.
fn total_usage(
    facts: &PlanUsageFacts,
    plan_usage: &Value,
    plan_name: Option<&str>,
    cycle: BillingCycle,
    now: DateTime<Utc>,
) -> Result<UsageResource, CursorProbeError> {
    let spent_cents = plan_usage
        .get("totalSpend")
        .and_then(parse::number)
        .or_else(|| {
            let limit = facts.limit_cents?;
            let remaining = plan_usage.get("remaining").and_then(parse::number)?;
            Some(limit - remaining)
        });

    let is_team = plan_name
        .unwrap_or_default()
        .trim()
        .eq_ignore_ascii_case("team")
        || facts.is_team_by_shape;
    let resource = if is_team {
        // A team plan is metered in money, not percent: report the dollars.
        let limit = facts
            .limit_cents
            .ok_or(CursorProbeError::UsageLimitMissing)?;
        let spent = spent_cents.ok_or(CursorProbeError::UsageLimitMissing)?;
        bounded(
            "totalUsage",
            UsageUnit::Usd,
            parse::cents_to_dollars(spent.max(0.0)),
            parse::cents_to_dollars(limit),
        )
    } else {
        // An individual plan states its own percentage; when it does not, the
        // percentage is computed from the spend against the allowance. A plan
        // with neither stays "No data" rather than reading as 0%.
        let percent = facts.total_percent_used.or_else(|| {
            let limit = facts.limit_cents.filter(|value| *value > 0.0)?;
            Some(spent_cents? / limit * 100.0)
        });
        UsageResource::percent("totalUsage", percent.and_then(parse::clamp_percent))
    };

    Ok(resource
        .with_resets_at(cycle.resets_at)
        .with_period_ms(Some(cycle.period_ms))
        .derive_projection(now))
}

/// `onDemand`: spend beyond the plan allowance.
///
/// A stated cap makes it a meter; without one it is a spent total with no limit,
/// which is exactly what Cursor reports for a user-scoped on-demand budget.
fn on_demand(spend_limit: Option<&Value>) -> Option<UsageResource> {
    let spend_limit = spend_limit?;
    let limit = first_number(spend_limit, &["individualLimit", "pooledLimit"]).unwrap_or(0.0);
    let remaining =
        first_number(spend_limit, &["individualRemaining", "pooledRemaining"]).unwrap_or(0.0);
    let spent = spent_cents(spend_limit, limit, remaining);

    if limit > 0.0 {
        return Some(bounded(
            "onDemand",
            UsageUnit::Usd,
            parse::cents_to_dollars(spent.max(0.0)),
            parse::cents_to_dollars(limit),
        ));
    }
    if spent > 0.0 {
        return Some(
            UsageResource::consumption("onDemand", UsageUnit::Usd)
                .with_used(Some(parse::cents_to_dollars(spent))),
        );
    }
    None
}

/// On-demand spend, in cents.
///
/// Cursor reports it under three different keys depending on the account shape,
/// and a zero in the first of them must not mask a real figure in another — so
/// the first *positive* report wins, and only then does the cap-minus-remaining
/// inference apply.
fn spent_cents(spend_limit: &Value, limit: f64, remaining: f64) -> f64 {
    let reported: Vec<f64> = ["individualUsed", "pooledUsed", "totalSpend"]
        .iter()
        .filter_map(|key| spend_limit.get(*key).and_then(parse::number))
        .collect();
    if let Some(positive) = reported.iter().copied().find(|value| *value > 0.0) {
        return positive;
    }
    let inferred = (limit - remaining).max(0.0);
    if inferred > 0.0 {
        inferred
    } else {
        reported.first().copied().unwrap_or(0.0)
    }
}

/// Prepaid credit left, in dollars.
///
/// Grants and the prepaid balance are one pool from the user's point of view, so
/// they are summed. `None` when there is no pool at all — an account with no
/// prepaid credit shows "No data", not `$0`.
fn credits(grants: Option<&Value>, stripe_balance_cents: f64) -> Option<UsageResource> {
    let has_grants = grants
        .and_then(|value| value.get("hasCreditGrants"))
        .and_then(parse::boolean)
        == Some(true);
    let total = has_grants
        .then(|| {
            grants
                .and_then(|value| value.get("totalCents"))
                .and_then(parse::number)
        })
        .flatten()
        .unwrap_or(0.0);
    let used = has_grants
        .then(|| {
            grants
                .and_then(|value| value.get("usedCents"))
                .and_then(parse::number)
        })
        .flatten()
        .unwrap_or(0.0);
    let granted = if total > 0.0 { total } else { 0.0 };

    let combined = granted + stripe_balance_cents;
    if combined <= 0.0 {
        return None;
    }
    let remaining = (combined - if granted > 0.0 { used } else { 0.0 }).max(0.0);
    Some(UsageResource::balance(
        "credits",
        UsageUnit::Usd,
        Some(parse::cents_to_dollars(remaining)),
    ))
}

/// A meter whose percentage is arithmetic on two reported scalars.
///
/// Filling `used_percent` here is not fabrication — both operands came from the
/// provider — and it is what lets a dollar or request meter take part in the
/// low-quota grading that only reads percentages.
fn bounded(id: &str, unit: UsageUnit, used: f64, limit: f64) -> UsageResource {
    let used_percent = if limit > 0.0 {
        parse::clamp_percent(used / limit * 100.0)
    } else {
        None
    };
    UsageResource {
        used_percent,
        remaining_percent: used_percent.map(|percent| (100.0 - percent).clamp(0.0, 100.0)),
        used: Some(used),
        limit: Some(limit),
        ..UsageResource::consumption(id, unit)
    }
}

fn first_number(value: &Value, keys: &[&str]) -> Option<f64> {
    keys.iter()
        .find_map(|key| value.get(*key).and_then(parse::number))
}

/// The billing window, from the cycle bounds when the payload states them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BillingCycle {
    resets_at: Option<DateTime<Utc>>,
    period_ms: i64,
}

fn billing_cycle(usage: &Value) -> BillingCycle {
    let start = usage.get("billingCycleStart").and_then(parse::number);
    let end = usage.get("billingCycleEnd").and_then(parse::number);
    let resets_at = end.and_then(parse::parse_epoch);
    match (start, end) {
        (Some(start), Some(end)) if end > start => BillingCycle {
            resets_at,
            #[allow(clippy::cast_possible_truncation)]
            period_ms: (end - start) as i64,
        },
        _ => BillingCycle {
            resets_at,
            period_ms: BILLING_PERIOD_MS,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        map, map_request_based, plan_name, should_use_request_fallback, stripe_balance_cents,
        CursorPayloads, CursorProbeError, PlanUsageFacts, BILLING_PERIOD_MS,
    };
    use crate::usage::model::{
        Availability, ProbeFailure, ReasonCode, ResourceKind, UsageResource, UsageUnit,
    };
    use chrono::{DateTime, TimeZone, Utc};
    use serde_json::{json, Value};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 3, 18, 12, 0, 0).unwrap()
    }

    fn individual_usage() -> Value {
        json!({
            "enabled": true,
            "billingCycleStart": 1_770_000_000_000_i64,
            "billingCycleEnd": 1_772_592_000_000_i64,
            "planUsage": {
                "limit": 40_000,
                "remaining": 32_000,
                "totalPercentUsed": 20,
                "autoPercentUsed": 12.5,
                "apiPercentUsed": 7.5
            },
            "spendLimitUsage": { "individualLimit": 5_000, "individualRemaining": 1_000 }
        })
    }

    fn payloads<'a>(usage: &'a Value, plan: Option<&'a str>) -> CursorPayloads<'a> {
        CursorPayloads {
            usage,
            plan_name: plan,
            credit_grants: None,
            stripe_balance_cents: 0.0,
        }
    }

    fn find<'a>(resources: &'a [UsageResource], id: &str) -> Option<&'a UsageResource> {
        resources.iter().find(|resource| resource.id == id)
    }

    #[test]
    fn maps_an_individual_account_as_percentages_with_a_bounded_on_demand_meter() {
        let usage = individual_usage();
        let snapshot = map(&payloads(&usage, Some("pro plan")), now()).unwrap();

        assert_eq!(snapshot.plan.as_deref(), Some("Pro Plan"));
        let total = find(&snapshot.resources, "totalUsage").unwrap();
        assert_eq!(total.used_percent, Some(20.0));
        assert_eq!(total.unit, UsageUnit::Percent);
        assert_eq!(
            find(&snapshot.resources, "autoUsage").unwrap().used_percent,
            Some(12.5)
        );
        assert_eq!(
            find(&snapshot.resources, "apiUsage").unwrap().used_percent,
            Some(7.5)
        );

        let on_demand = find(&snapshot.resources, "onDemand").unwrap();
        assert_eq!(on_demand.unit, UsageUnit::Usd);
        assert_eq!(on_demand.used, Some(40.0));
        assert_eq!(on_demand.limit, Some(50.0));
        assert_eq!(on_demand.used_percent, Some(80.0));
    }

    #[test]
    fn the_billing_cycle_sets_the_window_and_the_reset() {
        let usage = individual_usage();
        let snapshot = map(&payloads(&usage, None), now()).unwrap();
        let total = find(&snapshot.resources, "totalUsage").unwrap();

        assert_eq!(total.period_duration_ms, Some(2_592_000_000));
        assert_eq!(
            total.resets_at,
            Some(Utc.timestamp_millis_opt(1_772_592_000_000).unwrap())
        );
    }

    #[test]
    fn a_payload_without_cycle_bounds_falls_back_to_a_monthly_window() {
        let mut usage = individual_usage();
        let object = usage.as_object_mut().unwrap();
        object.remove("billingCycleStart");
        object.remove("billingCycleEnd");

        let snapshot = map(&payloads(&usage, None), now()).unwrap();
        let total = find(&snapshot.resources, "totalUsage").unwrap();

        assert_eq!(total.period_duration_ms, Some(BILLING_PERIOD_MS));
        assert_eq!(total.resets_at, None);
    }

    #[test]
    fn a_team_account_reports_total_usage_in_dollars() {
        let usage = json!({
            "enabled": true,
            "billingCycleStart": 1_770_000_000_000_i64,
            "billingCycleEnd": 1_772_592_000_000_i64,
            "planUsage": { "limit": 40_000, "totalSpend": 10_000, "bonusSpend": 2_500 }
        });

        let snapshot = map(&payloads(&usage, Some("Team")), now()).unwrap();
        let total = find(&snapshot.resources, "totalUsage").unwrap();

        assert_eq!(total.unit, UsageUnit::Usd);
        assert_eq!(total.used, Some(100.0));
        assert_eq!(total.limit, Some(400.0));
        // `bonusSpend` has no resource in the contract, so nothing is emitted
        // for it rather than an orphaned row.
        assert_eq!(snapshot.resources.len(), 1);
    }

    #[test]
    fn a_non_numeric_total_spend_falls_back_to_the_allowance_minus_what_is_left() {
        // `totalSpend: true` is a flag, not a quantity: reading it as 1 would
        // report a cent of spend on a $400 plan.
        let usage = json!({
            "enabled": true,
            "planUsage": { "limit": 40_000, "remaining": 32_000, "totalSpend": true }
        });

        let snapshot = map(&payloads(&usage, Some("Team")), now()).unwrap();
        let total = find(&snapshot.resources, "totalUsage").unwrap();

        assert_eq!(total.used, Some(80.0));
        assert_eq!(total.limit, Some(400.0));
    }

    #[test]
    fn an_individual_account_computes_its_percentage_from_spend_when_none_is_stated() {
        let usage = json!({
            "enabled": true,
            "planUsage": { "limit": 40_000, "totalSpend": 10_000 }
        });

        let snapshot = map(&payloads(&usage, Some("Pro")), now()).unwrap();

        assert_eq!(
            find(&snapshot.resources, "totalUsage")
                .unwrap()
                .used_percent,
            Some(25.0)
        );
    }

    #[test]
    fn a_zero_on_demand_report_never_masks_a_real_one() {
        let usage = json!({
            "enabled": true,
            "planUsage": { "limit": 40_000, "totalPercentUsed": 20 },
            "spendLimitUsage": {
                "individualLimit": 5_000,
                "individualRemaining": 4_500,
                "individualUsed": 0,
                "totalSpend": 1_200
            }
        });

        let snapshot = map(&payloads(&usage, Some("Ultra")), now()).unwrap();
        let on_demand = find(&snapshot.resources, "onDemand").unwrap();

        assert_eq!(on_demand.used, Some(12.0));
        assert_eq!(on_demand.used_percent, Some(24.0));
    }

    #[test]
    fn on_demand_without_a_cap_is_a_spent_total_with_no_invented_limit() {
        let usage = json!({
            "enabled": true,
            "planUsage": { "limit": 40_000, "totalPercentUsed": 26.346, "totalSpend": 52_692 },
            "spendLimitUsage": { "individualUsed": 16_474, "limitType": "user", "totalSpend": 16_474 }
        });

        let snapshot = map(&payloads(&usage, Some("Ultra")), now()).unwrap();
        let on_demand = find(&snapshot.resources, "onDemand").unwrap();

        assert_eq!(on_demand.used, Some(164.74));
        assert_eq!(on_demand.limit, None);
        assert_eq!(on_demand.used_percent, None);
    }

    #[test]
    fn credits_sum_the_grants_and_the_prepaid_balance() {
        let usage = individual_usage();
        let grants =
            json!({ "hasCreditGrants": true, "totalCents": "1000000", "usedCents": "264729" });
        let snapshot = map(
            &CursorPayloads {
                usage: &usage,
                plan_name: Some("pro plan"),
                credit_grants: Some(&grants),
                stripe_balance_cents: 991_544.0,
            },
            now(),
        )
        .unwrap();

        let credits = find(&snapshot.resources, "credits").unwrap();
        assert_eq!(credits.kind, ResourceKind::Balance);
        assert!((credits.available.unwrap() - 17_268.15).abs() < 0.001);
    }

    #[test]
    fn an_account_with_no_prepaid_credit_shows_no_data_rather_than_zero() {
        let usage = individual_usage();
        let grants = json!({ "hasCreditGrants": false });
        let snapshot = map(
            &CursorPayloads {
                usage: &usage,
                plan_name: None,
                credit_grants: Some(&grants),
                stripe_balance_cents: 0.0,
            },
            now(),
        )
        .unwrap();

        assert!(find(&snapshot.resources, "credits").is_none());
    }

    #[test]
    fn the_prepaid_balance_is_only_read_when_it_is_a_credit() {
        assert!(
            (stripe_balance_cents(&json!({ "customerBalance": "-50000" })) - 50_000.0).abs() < 1e-9
        );
        assert!(stripe_balance_cents(&json!({ "customerBalance": 5_000 })).abs() < f64::EPSILON);
        assert!(stripe_balance_cents(&json!({})).abs() < f64::EPSILON);
    }

    #[test]
    fn a_disabled_account_and_a_plan_with_no_denominator_both_degrade() {
        let disabled = json!({ "enabled": false, "planUsage": { "limit": 10 } });
        assert_eq!(
            map(&payloads(&disabled, None), now()),
            Err(CursorProbeError::NoActiveSubscription)
        );

        let no_limit = json!({ "enabled": true, "planUsage": { "remaining": 5 } });
        assert_eq!(
            map(&payloads(&no_limit, None), now()),
            Err(CursorProbeError::UsageLimitMissing)
        );

        for error in [
            CursorProbeError::NoActiveSubscription,
            CursorProbeError::UsageLimitMissing,
        ] {
            assert_eq!(
                error.classify(),
                (Availability::Unknown, ReasonCode::UnsupportedPayload)
            );
        }
    }

    #[test]
    fn maps_the_request_metered_shape() {
        let usage = json!({
            "gpt-4": { "numRequests": 39, "maxRequestUsage": 500 },
            "startOfMonth": "2026-02-09T17:36:37.000Z"
        });

        let snapshot = map_request_based(&usage, Some("Team"), now()).unwrap();
        let requests = find(&snapshot.resources, "requests").unwrap();

        assert_eq!(snapshot.plan.as_deref(), Some("Team"));
        assert_eq!(requests.unit, UsageUnit::Requests);
        assert_eq!(requests.used, Some(39.0));
        assert_eq!(requests.limit, Some(500.0));
        assert_eq!(requests.period_duration_ms, Some(BILLING_PERIOD_MS));
        assert!(requests.resets_at.is_some());
        // 39 of 500 is 7.8%, which is what lets a request meter grade.
        assert!((requests.used_percent.unwrap() - 7.8).abs() < 1e-9);
    }

    #[test]
    fn a_request_payload_with_no_allowance_degrades() {
        assert_eq!(
            map_request_based(&json!({ "gpt-4": { "numRequests": 39 } }), None, now()),
            Err(CursorProbeError::UsageLimitMissing)
        );
        assert_eq!(
            map_request_based(&json!({}), None, now()),
            Err(CursorProbeError::UsageLimitMissing)
        );
    }

    #[test]
    fn the_request_fallback_triggers_only_where_the_spend_shape_has_no_denominator() {
        let unusable = json!({ "enabled": true, "planUsage": { "remaining": 1 } });
        assert!(should_use_request_fallback(
            &unusable,
            Some("Enterprise"),
            false
        ));
        assert!(should_use_request_fallback(&unusable, Some("team"), false));
        assert!(should_use_request_fallback(&unusable, None, true));
        // Plan metadata that simply has not been read is not a reason on its own.
        assert!(!should_use_request_fallback(&unusable, None, false));
        // A usable spend shape never falls back.
        assert!(!should_use_request_fallback(
            &individual_usage(),
            Some("Pro"),
            false
        ));
        // A disabled account has nothing to fall back to.
        assert!(!should_use_request_fallback(
            &json!({ "enabled": false }),
            Some("Team"),
            false
        ));
    }

    #[test]
    fn a_pooled_shape_alone_identifies_a_team_account() {
        let pooled = json!({
            "enabled": true,
            "planUsage": { "remaining": 1 },
            "spendLimitUsage": { "pooledLimit": 10_000 }
        });
        assert!(PlanUsageFacts::read(&pooled).is_team_by_shape);
        assert!(should_use_request_fallback(&pooled, None, false));
    }

    #[test]
    fn the_plan_name_is_read_from_its_own_payload() {
        assert_eq!(
            plan_name(&json!({ "planInfo": { "planName": "pro plan" } })),
            Some("pro plan")
        );
        assert_eq!(plan_name(&json!({ "planInfo": {} })), None);
        assert_eq!(plan_name(&json!({})), None);
    }

    #[test]
    fn a_failure_never_carries_free_text_from_the_payload() {
        assert_eq!(
            CursorProbeError::NoActiveSubscription.to_string(),
            "no active subscription"
        );
        assert_eq!(
            CursorProbeError::UsageLimitMissing.to_string(),
            "usage limit missing"
        );
    }
}
