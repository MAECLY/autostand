//! Normalised usage vocabulary shared by every provider probe.
//!
//! The types here are the *output* half of the probe contract: a mapper takes a
//! deserialized payload plus response headers and returns a
//! [`ProviderSnapshot`]. Nothing in this module performs I/O or reads a clock —
//! every timestamp is passed in — which is what makes the mappers fixture-testable.
//!
//! Two rules are structural, not stylistic:
//!
//! 1. **Never fabricate.** A field the provider did not send is `None`. There is
//!    no `unwrap_or(0.0)` anywhere in this module and there must not be one in a
//!    mapper either: the UI renders `None` as "No data", and `0%` is a claim.
//! 2. **Nothing secret escapes.** A snapshot carries a [`ReasonCode`] from a
//!    fixed vocabulary, never a token, a header, a URL or a response body.

use std::time::Duration;

use autostand_core::pace::{self, Pace};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// What a resource measures.
///
/// `Consumption` fills a meter (`used` of `limit`); `Balance` counts down
/// (`available` remaining).
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind {
    Consumption,
    Balance,
}

/// Unit the raw scalars on a [`UsageResource`] are expressed in.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageUnit {
    Percent,
    Usd,
    Credits,
    Requests,
    Tokens,
    Count,
}

/// Provider availability, independent of whether an exact quota is known.
///
/// Mirrors the IPC-level `ProviderAvailability`; the adapters crate cannot
/// depend on the Tauri app, so the app converts at its own boundary.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Low,
    Exhausted,
    RateLimited,
    AuthRequired,
    ModelUnavailable,
    Unavailable,
    /// No usable signal. Absence of data is never treated as exhaustion.
    #[default]
    Unknown,
}

/// Where a reading came from. Mirrors the IPC-level `UsageSource`.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageSourceKind {
    /// Structured data in the provider's own usage response body.
    ProviderReported,
    /// Values recovered from rate-limit response headers.
    ResponseHeaders,
    /// A separately authenticated organisation/management API.
    ManagementApi,
    /// Availability inferred from a classified failure, never an exact quota.
    FailureInferred,
    #[default]
    Unknown,
}

/// Stable, lowercase, secret-free failure identifier.
///
/// An enum rather than a `&str` so a typo is a compile error and so no caller
/// can smuggle a response body into the field. The UI owns the human string.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReasonCode {
    /// No credential on this machine at all.
    NotLoggedIn,
    /// A credential exists but the provider rejected it.
    SessionExpired,
    /// The saved login cannot read usage, though inference still works.
    MissingProfileScope,
    /// An API key was found where a subscription login is required.
    UsageRequiresCliLogin,
    RateLimited,
    /// Transport failure: DNS, TLS, connection reset, offline.
    Network,
    Timeout,
    /// The response parsed but did not contain the documented fields.
    UnsupportedPayload,
    /// The endpoint answered with a status the probe does not model.
    UnexpectedStatus,
    /// This provider exposes no usage contract at all.
    UsageNotSupported,
    /// A local credential store could not be consulted (locked, denied).
    CredentialStoreUnavailable,
}

impl ReasonCode {
    /// Wire spelling, identical to the serde representation.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NotLoggedIn => "not_logged_in",
            Self::SessionExpired => "session_expired",
            Self::MissingProfileScope => "missing_profile_scope",
            Self::UsageRequiresCliLogin => "usage_requires_cli_login",
            Self::RateLimited => "rate_limited",
            Self::Network => "network",
            Self::Timeout => "timeout",
            Self::UnsupportedPayload => "unsupported_payload",
            Self::UnexpectedStatus => "unexpected_status",
            Self::UsageNotSupported => "usage_not_supported",
            Self::CredentialStoreUnavailable => "credential_store_unavailable",
        }
    }
}

impl std::fmt::Display for ReasonCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A probe failure that classifies itself.
///
/// Every provider defines its own `thiserror` enum and implements this with a
/// `match` that has **no wildcard arm**, so adding a variant is a compile error
/// rather than a silent fallthrough to `unknown`.
pub trait ProbeFailure {
    /// Map this failure to the pair the UI needs. Must be exhaustive.
    fn classify(&self) -> (Availability, ReasonCode);
}

/// Failure classes shared by every provider.
///
/// A provider with no extra failure modes can use this directly; one with its
/// own (an unreadable local database, a missing account id) wraps it in its own
/// enum and forwards `classify`.
///
/// `Display` strings are deliberately constant: they never interpolate a URL, a
/// body or a token, so an accidental `{err}` in a log line cannot leak.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum UsageError {
    /// No credential file and no keychain item.
    #[error("not logged in")]
    NotLoggedIn,
    /// A credential exists but is expired or was rejected. Under the read-only
    /// decision autostand reports this instead of refreshing the token.
    #[error("session expired")]
    SessionExpired,
    /// The login lacks the scope required to read usage.
    #[error("missing profile scope")]
    MissingProfileScope,
    /// An API key cannot see subscription quota.
    #[error("usage requires a CLI login")]
    UsageRequiresCliLogin,
    /// 429, with the vendor's cooldown when it supplied one.
    #[error("rate limited")]
    RateLimited { retry_after_secs: Option<u64> },
    #[error("network unavailable")]
    Network,
    #[error("request timed out")]
    Timeout,
    /// Valid JSON without the documented fields — a payload change degrades the
    /// provider instead of producing wrong numbers.
    #[error("unsupported payload")]
    UnsupportedPayload,
    /// Status is safe to carry: it is a number, not content.
    #[error("unexpected status")]
    UnexpectedStatus { status: u16 },
    #[error("usage not supported")]
    UsageNotSupported,
    /// The keychain was locked, denied, or the prompt was cancelled. Distinct
    /// from [`Self::NotLoggedIn`]: a failed lookup is not proof of a logout.
    #[error("credential store unavailable")]
    CredentialStoreUnavailable,
}

impl ProbeFailure for UsageError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm. Adding a variant must not silently
        // become `unknown`.
        match self {
            Self::NotLoggedIn => (Availability::AuthRequired, ReasonCode::NotLoggedIn),
            Self::SessionExpired => (Availability::AuthRequired, ReasonCode::SessionExpired),
            // Inference still works; only the meters are unavailable, so the row
            // stays usable rather than reading as a failure.
            Self::MissingProfileScope => (Availability::Available, ReasonCode::MissingProfileScope),
            // Same reasoning: an API key renders fine, it just cannot see quota.
            Self::UsageRequiresCliLogin => {
                (Availability::Unknown, ReasonCode::UsageRequiresCliLogin)
            }
            Self::RateLimited { .. } => (Availability::RateLimited, ReasonCode::RateLimited),
            // Everything below is "we could not measure", never "you are out".
            Self::Network => (Availability::Unknown, ReasonCode::Network),
            Self::Timeout => (Availability::Unknown, ReasonCode::Timeout),
            Self::UnsupportedPayload => (Availability::Unknown, ReasonCode::UnsupportedPayload),
            Self::UnexpectedStatus { .. } => (Availability::Unknown, ReasonCode::UnexpectedStatus),
            Self::UsageNotSupported => (Availability::Unknown, ReasonCode::UsageNotSupported),
            Self::CredentialStoreUnavailable => (
                Availability::Unknown,
                ReasonCode::CredentialStoreUnavailable,
            ),
        }
    }
}

/// One normalised quota window.
///
/// Construct through [`UsageResource::consumption`] or [`UsageResource::balance`]
/// and refine with the `with_*` methods, so a new field can be added without
/// touching every provider.
#[derive(Debug, Clone, PartialEq)]
pub struct UsageResource {
    /// Stable, provider-defined id: `session`, `weekly`, `sonnet`, `credits`, …
    pub id: String,
    pub kind: ResourceKind,
    pub unit: UsageUnit,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub used: Option<f64>,
    pub limit: Option<f64>,
    pub available: Option<f64>,
    pub resets_at: Option<DateTime<Utc>>,
    pub period_duration_ms: Option<i64>,
    pub label: Option<String>,
    pub pace: Option<Pace>,
}

impl UsageResource {
    /// A meter: `used` of `limit`, in `unit`.
    #[must_use]
    pub fn consumption(id: impl Into<String>, unit: UsageUnit) -> Self {
        Self {
            id: id.into(),
            kind: ResourceKind::Consumption,
            unit,
            used_percent: None,
            remaining_percent: None,
            used: None,
            limit: None,
            available: None,
            resets_at: None,
            period_duration_ms: None,
            label: None,
            pace: None,
        }
    }

    /// A countdown: `available` remaining, in `unit`.
    #[must_use]
    pub fn balance(id: impl Into<String>, unit: UsageUnit, available: Option<f64>) -> Self {
        Self {
            kind: ResourceKind::Balance,
            available,
            ..Self::consumption(id, unit)
        }
    }

    /// A percentage meter, the most common shape.
    ///
    /// `used_percent` is expected to be pre-clamped by
    /// [`crate::usage::parse::clamp_percent`]; `remaining_percent` is derived
    /// only when a percentage is actually present, never invented.
    #[must_use]
    pub fn percent(id: impl Into<String>, used_percent: Option<f64>) -> Self {
        Self {
            used_percent,
            remaining_percent: used_percent.map(|used| (100.0 - used).clamp(0.0, 100.0)),
            used: used_percent,
            limit: used_percent.map(|_| 100.0),
            ..Self::consumption(id, UsageUnit::Percent)
        }
    }

    #[must_use]
    pub fn with_limit(mut self, limit: Option<f64>) -> Self {
        self.limit = limit;
        self
    }

    #[must_use]
    pub fn with_used(mut self, used: Option<f64>) -> Self {
        self.used = used;
        self
    }

    #[must_use]
    pub fn with_available(mut self, available: Option<f64>) -> Self {
        self.available = available;
        self
    }

    #[must_use]
    pub fn with_resets_at(mut self, resets_at: Option<DateTime<Utc>>) -> Self {
        self.resets_at = resets_at;
        self
    }

    #[must_use]
    pub fn with_period(mut self, period: Option<Duration>) -> Self {
        self.period_duration_ms = period.and_then(|p| i64::try_from(p.as_millis()).ok());
        self
    }

    #[must_use]
    pub fn with_period_ms(mut self, period_duration_ms: Option<i64>) -> Self {
        self.period_duration_ms = period_duration_ms;
        self
    }

    #[must_use]
    pub fn with_label(mut self, label: Option<String>) -> Self {
        self.label = label;
        self
    }

    #[must_use]
    pub fn with_pace(mut self, pace: Option<Pace>) -> Self {
        self.pace = pace;
        self
    }

    /// Window length, when the provider reported one.
    #[must_use]
    pub fn period(&self) -> Option<Duration> {
        self.period_duration_ms
            .and_then(|ms| u64::try_from(ms).ok())
            .map(Duration::from_millis)
    }

    /// Derive [`Self::pace`] from the reset time and the window length.
    ///
    /// Elapsed time is `period - (resets_at - now)`, so the projection needs
    /// both a period and a reset — with neither, `pace` stays `None` rather
    /// than being guessed from a percentage alone.
    #[must_use]
    pub fn derive_pace(mut self, now: DateTime<Utc>) -> Self {
        self.pace = self.projected_pace(now);
        self
    }

    fn projected_pace(&self, now: DateTime<Utc>) -> Option<Pace> {
        let period = self.period()?;
        let resets_at = self.resets_at?;
        let used = self.used?;
        let limit = self.limit?;
        let remaining_secs = (resets_at - now).num_seconds();
        if remaining_secs < 0 {
            return None;
        }
        let elapsed = period.checked_sub(Duration::from_secs(remaining_secs.unsigned_abs()))?;
        pace::evaluate(used, limit, elapsed, period)
    }
}

/// The normalised result of one provider refresh.
///
/// A probe returns this even on failure — a failure is a classified snapshot,
/// not an `Err` that blanks the row.
#[derive(Debug, Clone, PartialEq)]
pub struct ProviderSnapshot {
    /// Stable provider id, matching `LlmAdapter::id()`.
    pub provider: String,
    pub availability: Availability,
    pub source: UsageSourceKind,
    pub resources: Vec<UsageResource>,
    /// Subscription tier as the provider names it (`"Max 20x"`).
    pub plan: Option<String>,
    pub reason: Option<ReasonCode>,
    /// Non-fatal, app-authored copy shown beside the provider name.
    pub notice: Option<String>,
    /// True when this snapshot restates cached values after a failed refresh.
    pub stale: bool,
    /// Observation time, injected so mappers stay pure.
    pub checked_at: DateTime<Utc>,
}

impl ProviderSnapshot {
    /// A successful reading.
    #[must_use]
    pub fn ok(
        provider: impl Into<String>,
        source: UsageSourceKind,
        resources: Vec<UsageResource>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            provider: provider.into(),
            availability: Availability::Available,
            source,
            resources,
            plan: None,
            reason: None,
            notice: None,
            stale: false,
            checked_at: now,
        }
    }

    /// A snapshot with no reading and a stated reason.
    #[must_use]
    pub fn unknown(provider: impl Into<String>, reason: ReasonCode, now: DateTime<Utc>) -> Self {
        Self {
            availability: Availability::Unknown,
            reason: Some(reason),
            ..Self::ok(provider, UsageSourceKind::Unknown, Vec::new(), now)
        }
    }

    /// Classify a typed failure into a snapshot. The single conversion point, so
    /// no provider invents its own availability mapping.
    #[must_use]
    pub fn from_failure<E: ProbeFailure>(
        provider: impl Into<String>,
        error: &E,
        now: DateTime<Utc>,
    ) -> Self {
        let (availability, reason) = error.classify();
        Self {
            availability,
            reason: Some(reason),
            ..Self::ok(provider, UsageSourceKind::Unknown, Vec::new(), now)
        }
    }

    #[must_use]
    pub fn with_plan(mut self, plan: Option<String>) -> Self {
        self.plan = plan;
        self
    }

    #[must_use]
    pub fn with_notice(mut self, notice: Option<String>) -> Self {
        self.notice = notice;
        self
    }

    #[must_use]
    pub fn with_availability(mut self, availability: Availability) -> Self {
        self.availability = availability;
        self
    }

    #[must_use]
    pub fn with_source(mut self, source: UsageSourceKind) -> Self {
        self.source = source;
        self
    }

    #[must_use]
    pub fn with_stale(mut self, stale: bool) -> Self {
        self.stale = stale;
        self
    }

    /// Lowest remaining percentage across every window that reports one.
    ///
    /// `None` when no window carries a percentage — which is why availability is
    /// never derived by treating "no data" as zero.
    #[must_use]
    pub fn tightest_remaining_percent(&self) -> Option<f64> {
        self.resources
            .iter()
            .filter_map(|resource| resource.remaining_percent)
            .fold(None, |lowest: Option<f64>, value| {
                Some(lowest.map_or(value, |current| current.min(value)))
            })
    }

    /// Downgrade [`Availability::Available`] to `Low`/`Exhausted` from the
    /// tightest window, using the caller's configured threshold.
    ///
    /// Leaves any non-`Available` availability alone: an auth or rate-limit
    /// classification outranks a percentage.
    #[must_use]
    pub fn graded(mut self, low_threshold_percent: f64) -> Self {
        if self.availability != Availability::Available {
            return self;
        }
        if let Some(remaining) = self.tightest_remaining_percent() {
            self.availability = if remaining <= 0.0 {
                Availability::Exhausted
            } else if remaining <= low_threshold_percent {
                Availability::Low
            } else {
                Availability::Available
            };
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Availability, ProbeFailure, ProviderSnapshot, ReasonCode, ResourceKind, UsageError,
        UsageResource, UsageSourceKind, UsageUnit,
    };
    use autostand_core::pace::Pace;
    use chrono::{DateTime, TimeZone, Utc};
    use std::time::Duration;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    #[test]
    fn every_error_variant_classifies_without_a_wildcard() {
        let cases = [
            (UsageError::NotLoggedIn, Availability::AuthRequired),
            (UsageError::SessionExpired, Availability::AuthRequired),
            (UsageError::MissingProfileScope, Availability::Available),
            (UsageError::UsageRequiresCliLogin, Availability::Unknown),
            (
                UsageError::RateLimited {
                    retry_after_secs: Some(300),
                },
                Availability::RateLimited,
            ),
            (UsageError::Network, Availability::Unknown),
            (UsageError::Timeout, Availability::Unknown),
            (UsageError::UnsupportedPayload, Availability::Unknown),
            (
                UsageError::UnexpectedStatus { status: 500 },
                Availability::Unknown,
            ),
            (UsageError::UsageNotSupported, Availability::Unknown),
            (
                UsageError::CredentialStoreUnavailable,
                Availability::Unknown,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(error.classify().0, expected, "{error:?}");
        }
    }

    #[test]
    fn a_transport_failure_is_never_reported_as_exhausted() {
        // Absence of data is not evidence of exhaustion — a network blip must
        // not make the fallback chain skip a healthy provider.
        for error in [
            UsageError::Network,
            UsageError::Timeout,
            UsageError::UnsupportedPayload,
        ] {
            assert_eq!(error.classify().0, Availability::Unknown);
        }
    }

    #[test]
    fn reason_codes_are_lowercase_and_secret_free() {
        let codes = [
            ReasonCode::NotLoggedIn,
            ReasonCode::SessionExpired,
            ReasonCode::MissingProfileScope,
            ReasonCode::UsageRequiresCliLogin,
            ReasonCode::RateLimited,
            ReasonCode::Network,
            ReasonCode::Timeout,
            ReasonCode::UnsupportedPayload,
            ReasonCode::UnexpectedStatus,
            ReasonCode::UsageNotSupported,
            ReasonCode::CredentialStoreUnavailable,
        ];
        for code in codes {
            let text = code.as_str();
            assert_eq!(text, text.to_lowercase(), "{text}");
            assert!(text.chars().all(|c| c.is_ascii_lowercase() || c == '_'));
        }
    }

    #[test]
    fn reason_code_wire_string_matches_serde() {
        let json = serde_json::to_string(&ReasonCode::MissingProfileScope).unwrap();
        assert_eq!(json, "\"missing_profile_scope\"");
        assert_eq!(
            ReasonCode::MissingProfileScope.as_str(),
            "missing_profile_scope"
        );
    }

    #[test]
    fn percent_resource_derives_remaining_but_never_invents_it() {
        let known = UsageResource::percent("session", Some(30.0));
        assert_eq!(known.remaining_percent, Some(70.0));
        assert_eq!(known.limit, Some(100.0));
        assert_eq!(known.kind, ResourceKind::Consumption);

        let unknown = UsageResource::percent("session", None);
        assert_eq!(unknown.remaining_percent, None);
        assert_eq!(unknown.used, None);
        assert_eq!(unknown.limit, None);
    }

    #[test]
    fn balance_resource_keeps_its_countdown_shape() {
        let credits = UsageResource::balance("credits", UsageUnit::Credits, Some(821.0));
        assert_eq!(credits.kind, ResourceKind::Balance);
        assert_eq!(credits.available, Some(821.0));
        assert_eq!(credits.used_percent, None);
    }

    #[test]
    fn pace_derives_from_period_and_reset() {
        // Half of a five-hour window elapsed with 10% used projects to 20%.
        let five_hours = Duration::from_secs(18_000);
        let resource = UsageResource::percent("session", Some(10.0))
            .with_period(Some(five_hours))
            .with_resets_at(Some(now() + chrono::Duration::seconds(9_000)))
            .derive_pace(now());
        assert_eq!(resource.pace, Some(Pace::Ahead));
    }

    #[test]
    fn pace_stays_none_without_a_period_or_reset() {
        let no_period = UsageResource::percent("session", Some(90.0))
            .with_resets_at(Some(now() + chrono::Duration::seconds(60)))
            .derive_pace(now());
        assert_eq!(no_period.pace, None);

        let no_reset = UsageResource::percent("session", Some(90.0))
            .with_period(Some(Duration::from_secs(18_000)))
            .derive_pace(now());
        assert_eq!(no_reset.pace, None);
    }

    #[test]
    fn pace_stays_none_immediately_after_a_reset() {
        // Zero elapsed: one request in the first seconds extrapolates absurdly.
        let resource = UsageResource::percent("session", Some(5.0))
            .with_period(Some(Duration::from_secs(18_000)))
            .with_resets_at(Some(now() + chrono::Duration::seconds(18_000)))
            .derive_pace(now());
        assert_eq!(resource.pace, None);
    }

    #[test]
    fn tightest_remaining_ignores_windows_without_data() {
        let snapshot = ProviderSnapshot::ok(
            "claude",
            UsageSourceKind::ProviderReported,
            vec![
                UsageResource::percent("session", Some(40.0)),
                UsageResource::percent("weekly", None),
                UsageResource::percent("sonnet", Some(85.0)),
            ],
            now(),
        );
        assert_eq!(snapshot.tightest_remaining_percent(), Some(15.0));
    }

    #[test]
    fn tightest_remaining_is_none_when_nothing_reports_a_percentage() {
        let snapshot = ProviderSnapshot::ok(
            "codex",
            UsageSourceKind::ProviderReported,
            vec![UsageResource::balance(
                "credits",
                UsageUnit::Credits,
                Some(10.0),
            )],
            now(),
        );
        assert_eq!(snapshot.tightest_remaining_percent(), None);
    }

    #[test]
    fn grading_never_downgrades_a_provider_with_no_percentage() {
        let snapshot = ProviderSnapshot::ok(
            "codex",
            UsageSourceKind::ProviderReported,
            vec![UsageResource::balance("credits", UsageUnit::Credits, None)],
            now(),
        )
        .graded(20.0);
        assert_eq!(snapshot.availability, Availability::Available);
    }

    #[test]
    fn grading_marks_low_and_exhausted_from_the_tightest_window() {
        let low = ProviderSnapshot::ok(
            "claude",
            UsageSourceKind::ProviderReported,
            vec![UsageResource::percent("weekly", Some(85.0))],
            now(),
        )
        .graded(20.0);
        assert_eq!(low.availability, Availability::Low);

        let spent = ProviderSnapshot::ok(
            "claude",
            UsageSourceKind::ProviderReported,
            vec![UsageResource::percent("weekly", Some(100.0))],
            now(),
        )
        .graded(20.0);
        assert_eq!(spent.availability, Availability::Exhausted);
    }

    #[test]
    fn grading_does_not_override_an_auth_classification() {
        let snapshot =
            ProviderSnapshot::from_failure("claude", &UsageError::NotLoggedIn, now()).graded(20.0);
        assert_eq!(snapshot.availability, Availability::AuthRequired);
        assert_eq!(snapshot.reason, Some(ReasonCode::NotLoggedIn));
    }

    #[test]
    fn a_failure_snapshot_carries_no_resources_and_no_free_text() {
        let snapshot = ProviderSnapshot::from_failure(
            "codex",
            &UsageError::UnexpectedStatus { status: 503 },
            now(),
        );
        assert!(snapshot.resources.is_empty());
        assert_eq!(snapshot.reason, Some(ReasonCode::UnexpectedStatus));
        assert_eq!(snapshot.notice, None);
        assert_eq!(snapshot.plan, None);
    }

    #[test]
    fn error_display_never_interpolates_payload_detail() {
        // A stray `{err}` in a log line must not leak a status, a URL or a body.
        assert_eq!(
            UsageError::UnexpectedStatus { status: 503 }.to_string(),
            "unexpected status"
        );
        assert_eq!(
            UsageError::RateLimited {
                retry_after_secs: Some(42)
            }
            .to_string(),
            "rate limited"
        );
    }

    #[test]
    fn unknown_snapshot_states_its_reason() {
        let snapshot = ProviderSnapshot::unknown("grok", ReasonCode::UsageNotSupported, now());
        assert_eq!(snapshot.availability, Availability::Unknown);
        assert_eq!(snapshot.source, UsageSourceKind::Unknown);
        assert_eq!(snapshot.reason, Some(ReasonCode::UsageNotSupported));
    }
}
