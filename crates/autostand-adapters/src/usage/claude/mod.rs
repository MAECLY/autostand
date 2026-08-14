//! Claude (Anthropic) usage probe.
//!
//! The provider that renders most standups and, until now, the one that
//! reported nothing at all. Three pieces, per the contract in
//! [`crate::usage`]:
//!
//! - [`auth`] — read-only credential discovery (keychain, file, environment).
//! - [`client`] — one `GET /api/oauth/usage`.
//! - [`mapper`] — a pure payload → [`ProviderSnapshot`] mapping.
//!
//! # Decisions this module encodes
//!
//! - **Read-only.** An expired token is reported as `auth_required`. autostand
//!   never calls Anthropic's refresh endpoint, which also means it can never
//!   cause the `refresh_token_reused` race a rotating client can.
//! - **The keychain wins, and the loop only advances on expiry.** A stale
//!   `~/.claude/.credentials.json` must not outrank the live keychain session,
//!   but a fresh `claude` re-login in *either* store must be picked up.
//! - **A 429 stops the traffic.** During Anthropic's cooldown the endpoint is
//!   not called at all — not even on a manual refresh, which is exactly when a
//!   user hammering the button would make it worse. The last good reading is
//!   served with `stale: true`.
//! - **The cooldown is keyed by token fingerprint,** so signing into a different
//!   account starts from clean state instead of inheriting the old one's
//!   penalty box.

pub mod auth;
pub mod client;
pub mod mapper;

use std::sync::Mutex;

use async_trait::async_trait;
use chrono::{DateTime, Duration as ChronoDuration, Utc};

use super::creds::fingerprint;
use super::model::{Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError};
use super::{ProbeContext, UsageProbe};
use auth::{CandidateLoad, ClaudeCredential, LiveUsage};
use mapper::PlanFacts;

/// Stable provider id, matching `LlmAdapter::id()`.
pub const PROVIDER_ID: &str = "claude";

/// Cooldown applied to a 429 that arrives without a `Retry-After`.
///
/// Five minutes matches the refresh interval, so the next scheduled pass is the
/// first one allowed to try again.
const DEFAULT_COOLDOWN_SECS: u64 = 300;

/// Shown when the only login lives in the keychain and this was a background
/// pass, which must never raise a macOS dialog. One manual refresh resolves it.
const KEYCHAIN_DEFERRED_NOTICE: &str = "Refresh to read live usage";

/// Claude's failure modes: the shared ones, plus the one only Claude has.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ClaudeUsageError {
    #[error(transparent)]
    Usage(#[from] UsageError),
    /// A keychain login exists but this background pass declined to read it.
    /// Not a logout, and not a broken store — a deliberate deferral.
    #[error("credential read deferred")]
    KeychainDeferred,
}

impl ProbeFailure for ClaudeUsageError {
    fn classify(&self) -> (Availability, ReasonCode) {
        // Exhaustive on purpose — no `_` arm, so a new variant is a compile
        // error rather than a silent fallthrough to `unknown`.
        match self {
            Self::Usage(inner) => inner.classify(),
            Self::KeychainDeferred => (
                Availability::Unknown,
                ReasonCode::CredentialStoreUnavailable,
            ),
        }
    }
}

/// The Claude usage probe.
///
/// Holds only derived state: a token fingerprint, the last good snapshot, and
/// any active cooldown. No token and no response body is ever retained.
#[derive(Debug, Default)]
pub struct ClaudeProbe {
    state: Mutex<ProbeState>,
}

impl ClaudeProbe {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Per-account state carried between refreshes.
#[derive(Debug, Default)]
struct ProbeState {
    /// `sha256` of the access token this state belongs to — the only thing
    /// autostand ever retains about a third-party credential.
    fingerprint: Option<String>,
    last_good: Option<ProviderSnapshot>,
    rate_limited_until: Option<DateTime<Utc>>,
}

impl ProbeState {
    /// Point this state at `fingerprint`, discarding everything if the account
    /// changed. Without this, a new login would inherit the previous account's
    /// cooldown and briefly show its quota.
    fn rebind(&mut self, fingerprint: &str) {
        if self.fingerprint.as_deref() != Some(fingerprint) {
            self.fingerprint = Some(fingerprint.to_string());
            self.last_good = None;
            self.rate_limited_until = None;
        }
    }
}

impl ClaudeProbe {
    /// A poisoned lock is recovered rather than propagated: a probe must never
    /// panic, and the state it guards is a cache, not a correctness invariant.
    fn state(&self) -> std::sync::MutexGuard<'_, ProbeState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Cooldown and last-good reading for this account, rebinding first.
    fn account_state(
        &self,
        fingerprint: &str,
    ) -> (Option<DateTime<Utc>>, Option<ProviderSnapshot>) {
        let mut state = self.state();
        state.rebind(fingerprint);
        (state.rate_limited_until, state.last_good.clone())
    }

    fn start_cooldown(&self, fingerprint: &str, until: DateTime<Utc>) {
        let mut state = self.state();
        state.rebind(fingerprint);
        state.rate_limited_until = Some(until);
    }

    /// Remember a reading worth restating during a cooldown.
    ///
    /// Only a genuine reading qualifies: caching a degraded snapshot would let
    /// "no data" masquerade as last-known-good for the next five minutes.
    fn remember(&self, fingerprint: &str, snapshot: &ProviderSnapshot) {
        if snapshot.reason.is_some() || snapshot.resources.is_empty() {
            return;
        }
        let mut state = self.state();
        state.rebind(fingerprint);
        state.rate_limited_until = None;
        state.last_good = Some(snapshot.clone());
    }
}

#[async_trait]
impl UsageProbe for ClaudeProbe {
    fn id(&self) -> &'static str {
        PROVIDER_ID
    }

    async fn has_local_credentials(&self) -> bool {
        auth::has_local_credentials().await
    }

    async fn probe(&self, ctx: &ProbeContext) -> ProviderSnapshot {
        // The clock is read here, at the I/O boundary, and injected downwards:
        // every mapper below stays pure.
        let now = Utc::now();
        let load = auth::load_candidates(ctx.keychain_access()).await;
        let selection = select(&load.candidates);

        if selection.live.is_empty() {
            return blocked_snapshot(&load, &selection, now);
        }

        let mut last_failure = None;
        for &index in &selection.live {
            let credential = &load.candidates[index];
            let plan = plan_facts(credential);
            let Some(token) = credential.oauth.access_token() else {
                continue;
            };
            let account = fingerprint(token);

            let (cooldown, last_good) = self.account_state(&account);
            if let Some(until) = cooldown.filter(|until| now < *until) {
                let remaining = (until - now).num_seconds().max(0).unsigned_abs();
                return mapper::rate_limited_snapshot(
                    last_good.as_ref(),
                    Some(remaining),
                    plan,
                    now,
                );
            }

            match self
                .fetch(token, &account, plan, last_good.as_ref(), now)
                .await
            {
                Outcome::Answered(snapshot) => return *snapshot,
                // An expiry-class rejection is the one case worth retrying with
                // a different store: a re-login may have landed in the other one.
                Outcome::Expired => last_failure = Some(UsageError::SessionExpired),
            }
        }

        let plan = selection
            .live
            .first()
            .map_or_else(PlanFacts::default, |&index| {
                plan_facts(&load.candidates[index])
            });
        failure_snapshot(
            &ClaudeUsageError::Usage(last_failure.unwrap_or(UsageError::SessionExpired)),
            plan,
            now,
        )
    }
}

/// What one credential attempt produced.
enum Outcome {
    /// A snapshot to return, successful or classified.
    Answered(Box<ProviderSnapshot>),
    /// The token was rejected as expired; try the next store.
    Expired,
}

impl Outcome {
    fn answered(snapshot: ProviderSnapshot) -> Self {
        Self::Answered(Box::new(snapshot))
    }
}

impl ClaudeProbe {
    async fn fetch(
        &self,
        token: &str,
        account: &str,
        plan: PlanFacts<'_>,
        last_good: Option<&ProviderSnapshot>,
        now: DateTime<Utc>,
    ) -> Outcome {
        let response = match client::fetch_usage(token).await {
            Ok(response) => response,
            Err(error) => {
                return Outcome::answered(failure_snapshot(&error.into(), plan, now));
            }
        };

        match response.error_for_status(now) {
            Ok(()) => {}
            Err(UsageError::SessionExpired) => return Outcome::Expired,
            Err(UsageError::RateLimited { retry_after_secs }) => {
                let seconds = retry_after_secs.unwrap_or(DEFAULT_COOLDOWN_SECS);
                if let Some(offset) =
                    ChronoDuration::try_seconds(i64::try_from(seconds).unwrap_or(i64::MAX))
                {
                    self.start_cooldown(account, now + offset);
                }
                return Outcome::answered(mapper::rate_limited_snapshot(
                    last_good,
                    Some(seconds),
                    plan,
                    now,
                ));
            }
            Err(error) => return Outcome::answered(failure_snapshot(&error.into(), plan, now)),
        }

        let payload = match response.json_value() {
            Ok(payload) => payload,
            Err(error) => return Outcome::answered(failure_snapshot(&error.into(), plan, now)),
        };
        let snapshot = mapper::map(&payload, &response, plan, now);
        self.remember(account, &snapshot);
        Outcome::answered(snapshot)
    }
}

/// Which candidates can actually read usage, and in what order.
#[derive(Debug, Default, PartialEq, Eq)]
struct Selection {
    /// Indices into the candidate list, live-capable, **non-expired first**.
    live: Vec<usize>,
    /// When nothing is live-capable, the first candidate and why it is not.
    blocked: Option<(usize, LiveUsage)>,
}

/// Order candidates for the usage call.
///
/// Source order (keychain, file, environment) is the primary ranking and is
/// preserved; a credential whose stored expiry has already passed is merely
/// demoted behind its live siblings, never dropped — the endpoint stays the
/// authority on whether a token still works.
fn select(candidates: &[ClaudeCredential]) -> Selection {
    let mut fresh = Vec::new();
    let mut expired = Vec::new();
    let now = Utc::now();

    for (index, candidate) in candidates.iter().enumerate() {
        if auth::live_usage(candidate) != LiveUsage::Available {
            continue;
        }
        if candidate.oauth.is_expired(now) {
            expired.push(index);
        } else {
            fresh.push(index);
        }
    }
    fresh.extend(expired);

    let blocked = if fresh.is_empty() {
        candidates
            .first()
            .map(|candidate| (0, auth::live_usage(candidate)))
    } else {
        None
    };
    Selection {
        live: fresh,
        blocked,
    }
}

fn plan_facts(credential: &ClaudeCredential) -> PlanFacts<'_> {
    PlanFacts {
        subscription_type: credential.oauth.subscription_type.as_deref(),
        rate_limit_tier: credential.oauth.rate_limit_tier.as_deref(),
    }
}

/// The snapshot for "nothing here can read usage".
fn blocked_snapshot(
    load: &CandidateLoad,
    selection: &Selection,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    match selection.blocked {
        Some((index, LiveUsage::MissingProfileScope)) => {
            mapper::missing_scope_snapshot(plan_facts(&load.candidates[index]), now)
        }
        Some((index, LiveUsage::InferenceOnlyToken)) => {
            mapper::inference_only_snapshot(plan_facts(&load.candidates[index]), now)
        }
        // `Available` here would mean a live candidate that `select` dropped,
        // which it cannot do; treat it as the no-candidate case rather than
        // inventing a reading.
        Some((_, LiveUsage::Available)) | None => no_candidate_snapshot(load, now),
    }
}

/// Distinguish "signed out" from "we chose not to look" from "the store broke".
/// Collapsing the three would send a signed-in user to re-authenticate for no
/// reason.
fn no_candidate_snapshot(load: &CandidateLoad, now: DateTime<Utc>) -> ProviderSnapshot {
    if load.keychain_deferred {
        return ProviderSnapshot::from_failure(
            PROVIDER_ID,
            &ClaudeUsageError::KeychainDeferred,
            now,
        )
        .with_notice(Some(KEYCHAIN_DEFERRED_NOTICE.to_string()));
    }
    let error = if load.store_unavailable {
        UsageError::CredentialStoreUnavailable
    } else {
        UsageError::NotLoggedIn
    };
    ProviderSnapshot::from_failure(PROVIDER_ID, &ClaudeUsageError::Usage(error), now)
}

fn failure_snapshot(
    error: &ClaudeUsageError,
    plan: PlanFacts<'_>,
    now: DateTime<Utc>,
) -> ProviderSnapshot {
    ProviderSnapshot::from_failure(PROVIDER_ID, error, now).with_plan(mapper::format_plan(plan))
}

#[cfg(test)]
mod tests {
    use super::{
        blocked_snapshot, no_candidate_snapshot, plan_facts, select, ClaudeProbe, ClaudeUsageError,
        ProbeState, Selection, KEYCHAIN_DEFERRED_NOTICE, PROVIDER_ID,
    };
    use crate::usage::claude::auth::{ClaudeCredential, ClaudeOAuth};
    use crate::usage::claude::mapper::{PlanFacts, MISSING_SCOPE_NOTICE};
    use crate::usage::creds::{fingerprint, CredentialSource};
    use crate::usage::model::{
        Availability, ProbeFailure, ProviderSnapshot, ReasonCode, UsageError, UsageResource,
        UsageSourceKind,
    };
    use crate::usage::{UsageProbe, UsageRegistry};
    use chrono::{DateTime, TimeZone, Utc};

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap()
    }

    fn credential(source: CredentialSource, oauth: ClaudeOAuth) -> ClaudeCredential {
        ClaudeCredential {
            oauth,
            source,
            inference_only: source == CredentialSource::Environment,
        }
    }

    fn with_scope() -> ClaudeOAuth {
        ClaudeOAuth {
            access_token: Some("token".into()),
            subscription_type: Some("max".into()),
            rate_limit_tier: Some("default_claude_max_20x".into()),
            scopes: Some(vec!["user:profile".into()]),
            expires_at: None,
        }
    }

    fn good_snapshot() -> ProviderSnapshot {
        ProviderSnapshot::ok(
            PROVIDER_ID,
            UsageSourceKind::ProviderReported,
            vec![UsageResource::percent("session", Some(25.0))],
            now(),
        )
    }

    // ---- error classification -------------------------------------------------

    #[test]
    fn every_claude_failure_classifies_without_a_wildcard() {
        assert_eq!(
            ClaudeUsageError::KeychainDeferred.classify(),
            (
                Availability::Unknown,
                ReasonCode::CredentialStoreUnavailable
            )
        );
        // A deferral is never read as exhaustion, or the fallback chain would
        // skip a perfectly healthy provider.
        assert_ne!(
            ClaudeUsageError::KeychainDeferred.classify().0,
            Availability::Exhausted
        );
        assert_eq!(
            ClaudeUsageError::Usage(UsageError::NotLoggedIn).classify(),
            (Availability::AuthRequired, ReasonCode::NotLoggedIn)
        );
    }

    #[test]
    fn a_claude_failure_never_interpolates_detail_into_its_message() {
        assert_eq!(
            ClaudeUsageError::from(UsageError::UnexpectedStatus { status: 503 }).to_string(),
            "unexpected status"
        );
        assert_eq!(
            ClaudeUsageError::KeychainDeferred.to_string(),
            "credential read deferred"
        );
    }

    // ---- candidate selection --------------------------------------------------

    #[test]
    fn selection_keeps_source_order_and_demotes_an_expired_token() {
        // Keychain first, but an already-expired keychain token must not shadow
        // a fresh file login an external `claude` re-login wrote.
        let candidates = vec![
            credential(
                CredentialSource::KeychainCurrentUser,
                ClaudeOAuth {
                    expires_at: Some(1_000_000_000_000.0),
                    ..with_scope()
                },
            ),
            credential(CredentialSource::File, with_scope()),
        ];
        assert_eq!(select(&candidates).live, vec![1, 0]);
    }

    #[test]
    fn selection_prefers_the_keychain_when_both_are_live() {
        let candidates = vec![
            credential(CredentialSource::KeychainCurrentUser, with_scope()),
            credential(CredentialSource::File, with_scope()),
        ];
        assert_eq!(select(&candidates).live, vec![0, 1]);
    }

    #[test]
    fn an_environment_token_never_becomes_a_live_candidate() {
        let candidates = vec![credential(CredentialSource::Environment, with_scope())];
        let selection = select(&candidates);
        assert!(selection.live.is_empty());
        assert_eq!(
            selection.blocked,
            Some((0, super::LiveUsage::InferenceOnlyToken))
        );
    }

    #[test]
    fn a_login_without_the_profile_scope_is_blocked_not_probed() {
        let candidates = vec![credential(
            CredentialSource::KeychainCurrentUser,
            ClaudeOAuth {
                scopes: Some(vec!["user:inference".into()]),
                ..with_scope()
            },
        )];
        let selection = select(&candidates);
        assert!(selection.live.is_empty());
        assert_eq!(
            selection.blocked,
            Some((0, super::LiveUsage::MissingProfileScope))
        );
    }

    #[test]
    fn a_scoped_file_login_outranks_a_scopeless_keychain_one() {
        // The scope gate must not blank the meters when another store can read
        // them.
        let candidates = vec![
            credential(
                CredentialSource::KeychainCurrentUser,
                ClaudeOAuth {
                    scopes: Some(vec!["user:inference".into()]),
                    ..with_scope()
                },
            ),
            credential(CredentialSource::File, with_scope()),
        ];
        assert_eq!(select(&candidates).live, vec![1]);
    }

    #[test]
    fn no_candidates_selects_nothing_and_blocks_on_nothing() {
        assert_eq!(select(&[]), Selection::default());
    }

    // ---- blocked snapshots ----------------------------------------------------

    #[test]
    fn a_missing_scope_reports_the_re_login_notice_and_keeps_the_plan() {
        let load = super::CandidateLoad {
            candidates: vec![credential(
                CredentialSource::KeychainCurrentUser,
                ClaudeOAuth {
                    scopes: Some(vec!["user:inference".into()]),
                    ..with_scope()
                },
            )],
            ..super::CandidateLoad::default()
        };
        let selection = select(&load.candidates);
        let snapshot = blocked_snapshot(&load, &selection, now());

        assert_eq!(snapshot.availability, Availability::Available);
        assert_eq!(snapshot.reason, Some(ReasonCode::MissingProfileScope));
        assert_eq!(snapshot.notice.as_deref(), Some(MISSING_SCOPE_NOTICE));
        assert_eq!(snapshot.plan.as_deref(), Some("Max 20x"));
    }

    #[test]
    fn a_setup_token_reports_that_it_cannot_read_subscription_limits() {
        let load = super::CandidateLoad {
            candidates: vec![credential(CredentialSource::Environment, with_scope())],
            ..super::CandidateLoad::default()
        };
        let selection = select(&load.candidates);
        let snapshot = blocked_snapshot(&load, &selection, now());
        assert_eq!(snapshot.reason, Some(ReasonCode::UsageRequiresCliLogin));
    }

    #[test]
    fn the_three_no_credential_cases_stay_distinct() {
        let signed_out = no_candidate_snapshot(&super::CandidateLoad::default(), now());
        assert_eq!(signed_out.availability, Availability::AuthRequired);
        assert_eq!(signed_out.reason, Some(ReasonCode::NotLoggedIn));
        assert_eq!(signed_out.notice, None);

        let broken = no_candidate_snapshot(
            &super::CandidateLoad {
                store_unavailable: true,
                ..super::CandidateLoad::default()
            },
            now(),
        );
        // A store we could not read is not proof of a logout.
        assert_eq!(broken.availability, Availability::Unknown);
        assert_eq!(broken.reason, Some(ReasonCode::CredentialStoreUnavailable));

        let deferred = no_candidate_snapshot(
            &super::CandidateLoad {
                keychain_deferred: true,
                store_unavailable: true,
                ..super::CandidateLoad::default()
            },
            now(),
        );
        assert_eq!(deferred.availability, Availability::Unknown);
        assert_eq!(deferred.notice.as_deref(), Some(KEYCHAIN_DEFERRED_NOTICE));
    }

    // ---- cooldown state -------------------------------------------------------

    #[test]
    fn state_survives_a_refresh_for_the_same_account() {
        let probe = ClaudeProbe::new();
        let account = fingerprint("token-a");
        probe.remember(&account, &good_snapshot());
        probe.start_cooldown(&account, now());

        let (cooldown, last_good) = probe.account_state(&account);
        assert_eq!(cooldown, Some(now()));
        assert!(last_good.is_some());
    }

    #[test]
    fn a_different_account_starts_with_a_clean_cooldown() {
        // Otherwise a new login inherits the previous account's penalty box and
        // briefly shows its quota.
        let probe = ClaudeProbe::new();
        let first = fingerprint("token-a");
        probe.remember(&first, &good_snapshot());
        probe.start_cooldown(&first, now());

        let (cooldown, last_good) = probe.account_state(&fingerprint("token-b"));
        assert_eq!(cooldown, None);
        assert_eq!(last_good, None);
    }

    #[test]
    fn only_a_real_reading_is_remembered_as_last_good() {
        let probe = ClaudeProbe::new();
        let account = fingerprint("token-a");

        probe.remember(
            &account,
            &ProviderSnapshot::from_failure(PROVIDER_ID, &UsageError::Network, now()),
        );
        assert_eq!(probe.account_state(&account).1, None);

        probe.remember(
            &account,
            &ProviderSnapshot::ok(
                PROVIDER_ID,
                UsageSourceKind::ProviderReported,
                vec![],
                now(),
            ),
        );
        assert_eq!(probe.account_state(&account).1, None);

        probe.remember(&account, &good_snapshot());
        assert!(probe.account_state(&account).1.is_some());
    }

    #[test]
    fn a_successful_reading_clears_an_active_cooldown() {
        let probe = ClaudeProbe::new();
        let account = fingerprint("token-a");
        probe.start_cooldown(&account, now());
        probe.remember(&account, &good_snapshot());
        assert_eq!(probe.account_state(&account).0, None);
    }

    #[test]
    fn the_retained_fingerprint_is_never_the_token() {
        let probe = ClaudeProbe::new();
        probe.start_cooldown(&fingerprint("sk-ant-secret"), now());
        let rendered = format!("{:?}", probe.state.lock().unwrap());
        assert!(!rendered.contains("sk-ant-secret"), "{rendered}");
    }

    #[test]
    fn rebinding_is_idempotent_for_the_same_fingerprint() {
        let mut state = ProbeState::default();
        state.rebind("abc");
        state.rate_limited_until = Some(now());
        state.rebind("abc");
        assert_eq!(state.rate_limited_until, Some(now()));
    }

    // ---- wiring ---------------------------------------------------------------

    #[test]
    fn plan_facts_read_only_the_display_fields() {
        let stored = credential(CredentialSource::File, with_scope());
        let facts = plan_facts(&stored);
        assert_eq!(
            facts,
            PlanFacts {
                subscription_type: Some("max"),
                rate_limit_tier: Some("default_claude_max_20x"),
            }
        );
    }

    #[test]
    fn the_probe_is_registered_under_its_stable_id() {
        let registry = UsageRegistry::with_builtin_probes();
        assert!(registry.contains(PROVIDER_ID));
        assert_eq!(ClaudeProbe::new().id(), "claude");
    }
}
