//! Provider usage probes: read the subscription quota the user is already
//! signed in to, and normalise it into one vocabulary.
//!
//! See `docs/specs/provider-usage.md` for the full contract.
//!
//! # Shape of a provider
//!
//! Each provider is three pieces in its own module, mirroring `OpenUsage`:
//!
//! ```text
//! usage/<provider>/
//!   auth.rs     read-only credential discovery (files + keychain)
//!   client.rs   one request, returning status + headers + body
//!   mapper.rs   PURE: (payload, headers, now) -> ProviderSnapshot
//! ```
//!
//! **Mappers perform no I/O and read no clock.** Every timestamp is passed in.
//! That single rule is what makes a provider fixture-testable, and it is the
//! most important structural constraint in this module.
//!
//! # Rules that are not negotiable
//!
//! - **Read-only credentials.** A third-party token is never written,
//!   refreshed, rotated or deleted. An expired token is reported as
//!   `auth_required`, not renewed.
//! - **Never fabricate.** A field the provider did not send is `None`. There is
//!   no `unwrap_or(0.0)`; the UI says "No data".
//! - **Nothing secret escapes.** No token, header, URL or response body reaches
//!   a log, an error or a DTO. Failures carry a [`ReasonCode`] from a fixed
//!   vocabulary and nothing else.
//!
//! # Adding a provider
//!
//! Implement [`UsageProbe`], then add one line to
//! [`UsageRegistry::with_builtin_probes`]. Nothing else changes: the IPC layer
//! consults the registry by id and falls back to its previous behaviour for any
//! provider without a probe.

pub mod claude;
pub mod codex;
pub mod copilot;
pub mod creds;
pub mod cursor;
pub mod devin;
pub mod grok;
pub mod http;
pub mod model;
pub mod opencode;
pub mod openrouter;
pub mod parse;
pub mod zai;

use async_trait::async_trait;
use chrono::Utc;

pub use creds::keychain::{KeychainAccess, KeychainOutcome};
pub use creds::CredentialSource;
pub use model::{
    Availability, ProbeFailure, ProviderSnapshot, ReasonCode, ResourceKind, UsageError,
    UsageResource, UsageSourceKind, UsageUnit,
};

/// What the caller knows about *why* this refresh is happening.
///
/// Currently one fact, and it is load-bearing: macOS may prompt before a
/// non-owning app reads another app's keychain item, so a secret read is
/// confined to a refresh the user explicitly asked for. A five-minute
/// background pass that raised a system dialog would be unusable.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ProbeContext {
    /// True when the user pressed refresh; false for a scheduled pass.
    pub is_manual: bool,
}

impl ProbeContext {
    /// The user asked for this refresh.
    #[must_use]
    pub fn manual() -> Self {
        Self { is_manual: true }
    }

    /// A scheduled or startup pass. Keychain reads stay deferred.
    #[must_use]
    pub fn background() -> Self {
        Self { is_manual: false }
    }

    /// Whether this pass may read a secret from the keychain.
    #[must_use]
    pub fn keychain_access(self) -> KeychainAccess {
        if self.is_manual {
            KeychainAccess::Allowed
        } else {
            KeychainAccess::Deferred
        }
    }
}

/// One provider's usage contract.
///
/// Deliberately *not* a method on `LlmAdapter`: only a subset of providers
/// expose quota, and forcing every adapter to implement a no-op would spread the
/// contract for nothing.
#[async_trait]
pub trait UsageProbe: Send + Sync {
    /// Stable provider id, matching `LlmAdapter::id()`.
    fn id(&self) -> &'static str;

    /// Cheap, **local-only** check for whether credentials exist at all.
    ///
    /// Files and keychain existence probes only — never the network, and never
    /// a secret read that could raise a prompt. Used to decide which providers
    /// are worth listing and probing, which is what removes the rows that read
    /// "Usage unavailable" for a provider the user never signed in to.
    async fn has_local_credentials(&self) -> bool;

    /// Fetch and normalise.
    ///
    /// Never panics and never returns `Err`: a failure is a classified
    /// [`ProviderSnapshot`], because an error that blanks the row loses the
    /// reason the user needs.
    async fn probe(&self, ctx: &ProbeContext) -> ProviderSnapshot;
}

/// The probes available to this build, keyed by provider id.
///
/// Built once at the composition root and consulted by id; the IPC layer keeps
/// only plumbing.
#[derive(Default)]
pub struct UsageRegistry {
    probes: Vec<Box<dyn UsageProbe>>,
}

impl UsageRegistry {
    /// An empty registry. Every provider falls back to the caller's previous
    /// behaviour.
    #[must_use]
    pub fn new() -> Self {
        Self { probes: Vec::new() }
    }

    /// The probes this build ships.
    ///
    /// **The extension point: one line per provider.** Adding Claude is
    /// `registry.register(Box::new(claude::ClaudeProbe::new()));` and nothing
    /// else — no match arm, no IPC change, no DTO change.
    #[must_use]
    pub fn with_builtin_probes() -> Self {
        #[allow(unused_mut)]
        let mut registry = Self::new();
        registry.register(Box::new(claude::ClaudeProbe::new()));
        registry.register(Box::new(codex::CodexProbe::new()));
        registry.register(Box::new(cursor::CursorProbe::new()));
        registry.register(Box::new(copilot::CopilotProbe::new()));
        registry.register(Box::new(devin::DevinProbe::new()));
        registry.register(Box::new(grok::GrokProbe::new()));
        registry.register(Box::new(opencode::OpenCodeProbe::new()));
        registry.register(Box::new(openrouter::OpenRouterProbe::new()));
        registry.register(Box::new(zai::ZaiProbe::new()));
        registry
    }

    /// Add a probe. A second registration for the same id replaces the first,
    /// so a caller can override a built-in without rebuilding the list.
    pub fn register(&mut self, probe: Box<dyn UsageProbe>) -> &mut Self {
        let id = probe.id();
        self.probes.retain(|existing| existing.id() != id);
        self.probes.push(probe);
        self
    }

    /// Builder form of [`Self::register`].
    #[must_use]
    pub fn with(mut self, probe: Box<dyn UsageProbe>) -> Self {
        self.register(probe);
        self
    }

    /// The probe for `id`, if this build has one.
    #[must_use]
    pub fn get(&self, id: &str) -> Option<&dyn UsageProbe> {
        self.probes
            .iter()
            .find(|probe| probe.id() == id)
            .map(AsRef::as_ref)
    }

    /// Whether a probe exists for `id`.
    #[must_use]
    pub fn contains(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    /// Registered provider ids, in registration order.
    #[must_use]
    pub fn ids(&self) -> Vec<&'static str> {
        self.probes.iter().map(|probe| probe.id()).collect()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.probes.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.probes.len()
    }

    /// Probe one provider.
    ///
    /// `None` means this build has no probe for `id` — the caller keeps its
    /// existing behaviour, which is what lets probes land one provider at a
    /// time without a flag day.
    ///
    /// A provider with no local credentials is short-circuited to
    /// `auth_required` without a request: there is nothing to authenticate
    /// with, and asking anyway would be a guaranteed 401 per refresh.
    pub async fn snapshot(&self, id: &str, ctx: &ProbeContext) -> Option<ProviderSnapshot> {
        let probe = self.get(id)?;
        if !probe.has_local_credentials().await {
            // The registry is the I/O boundary, so reading the clock here is
            // fine — unlike inside a mapper, which must stay pure.
            return Some(ProviderSnapshot::from_failure(
                probe.id(),
                &UsageError::NotLoggedIn,
                Utc::now(),
            ));
        }
        Some(probe.probe(ctx).await)
    }

    /// Registered providers that have credentials on this machine.
    ///
    /// The listing filter: a provider the user never signed in to is not worth
    /// a row.
    pub async fn credentialed_ids(&self) -> Vec<&'static str> {
        let mut found = Vec::new();
        for probe in &self.probes {
            if probe.has_local_credentials().await {
                found.push(probe.id());
            }
        }
        found
    }
}

impl std::fmt::Debug for UsageRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UsageRegistry")
            .field("providers", &self.ids())
            .finish()
    }
}

/// A probe with no provider behind it, for wiring tests and as the shape a real
/// probe fills in.
///
/// Carries a canned snapshot and records nothing; it never touches the network,
/// the filesystem or the keychain.
pub struct StubProbe {
    id: &'static str,
    has_credentials: bool,
    snapshot: Option<ProviderSnapshot>,
}

impl StubProbe {
    /// A stub that reports credentials and returns `usage_not_supported`.
    #[must_use]
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            has_credentials: true,
            snapshot: None,
        }
    }

    /// Whether [`UsageProbe::has_local_credentials`] answers true.
    #[must_use]
    pub fn with_credentials(mut self, has_credentials: bool) -> Self {
        self.has_credentials = has_credentials;
        self
    }

    /// The snapshot [`UsageProbe::probe`] returns.
    #[must_use]
    pub fn with_snapshot(mut self, snapshot: ProviderSnapshot) -> Self {
        self.snapshot = Some(snapshot);
        self
    }
}

#[async_trait]
impl UsageProbe for StubProbe {
    fn id(&self) -> &'static str {
        self.id
    }

    async fn has_local_credentials(&self) -> bool {
        self.has_credentials
    }

    async fn probe(&self, _ctx: &ProbeContext) -> ProviderSnapshot {
        self.snapshot.clone().unwrap_or_else(|| {
            ProviderSnapshot::unknown(self.id, ReasonCode::UsageNotSupported, Utc::now())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Availability, KeychainAccess, ProbeContext, ProviderSnapshot, ReasonCode, StubProbe,
        UsageProbe, UsageRegistry, UsageResource, UsageSourceKind,
    };
    use chrono::{TimeZone, Utc};

    fn snapshot(provider: &str) -> ProviderSnapshot {
        ProviderSnapshot::ok(
            provider,
            UsageSourceKind::ProviderReported,
            vec![UsageResource::percent("session", Some(25.0))],
            Utc.with_ymd_and_hms(2026, 8, 13, 12, 0, 0).unwrap(),
        )
    }

    #[test]
    fn a_manual_refresh_may_read_the_keychain_and_a_background_one_may_not() {
        assert_eq!(
            ProbeContext::manual().keychain_access(),
            KeychainAccess::Allowed
        );
        assert_eq!(
            ProbeContext::background().keychain_access(),
            KeychainAccess::Deferred
        );
        // The default must be the safe side: no unprompted dialog on startup.
        assert_eq!(ProbeContext::default(), ProbeContext::background());
    }

    #[test]
    fn the_builtin_registry_registers_each_id_once() {
        // Registration is one line per provider and keyed by id, so a duplicate
        // would silently replace a shipped probe rather than fail to build.
        let ids = UsageRegistry::with_builtin_probes().ids();
        let mut unique = ids.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(ids.len(), unique.len(), "{ids:?}");
    }

    #[test]
    fn registration_is_keyed_by_id_and_last_wins() {
        let registry = UsageRegistry::new()
            .with(Box::new(StubProbe::new("claude")))
            .with(Box::new(StubProbe::new("codex")))
            .with(Box::new(StubProbe::new("claude").with_credentials(false)));
        assert_eq!(registry.len(), 2);
        assert_eq!(registry.ids(), vec!["codex", "claude"]);
        assert!(registry.contains("claude"));
        assert!(!registry.contains("grok"));
    }

    #[test]
    fn debug_lists_ids_only() {
        let registry = UsageRegistry::new().with(Box::new(StubProbe::new("claude")));
        assert_eq!(
            format!("{registry:?}"),
            r#"UsageRegistry { providers: ["claude"] }"#
        );
    }

    #[tokio::test]
    async fn an_unregistered_provider_yields_none_so_the_caller_keeps_its_behaviour() {
        let registry = UsageRegistry::new();
        assert!(registry
            .snapshot("claude", &ProbeContext::manual())
            .await
            .is_none());
    }

    #[tokio::test]
    async fn a_registered_probe_answers() {
        let registry = UsageRegistry::new().with(Box::new(
            StubProbe::new("claude").with_snapshot(snapshot("claude")),
        ));
        let result = registry
            .snapshot("claude", &ProbeContext::manual())
            .await
            .unwrap();
        assert_eq!(result.availability, Availability::Available);
        assert_eq!(result.resources.len(), 1);
    }

    #[tokio::test]
    async fn a_provider_without_credentials_is_not_asked_over_the_network() {
        // A guaranteed 401 every five minutes is worse than saying "sign in".
        let registry = UsageRegistry::new().with(Box::new(
            StubProbe::new("claude")
                .with_credentials(false)
                .with_snapshot(snapshot("claude")),
        ));
        let result = registry
            .snapshot("claude", &ProbeContext::background())
            .await
            .unwrap();
        assert_eq!(result.availability, Availability::AuthRequired);
        assert_eq!(result.reason, Some(ReasonCode::NotLoggedIn));
        assert!(result.resources.is_empty());
    }

    #[tokio::test]
    async fn listing_skips_providers_with_no_local_credentials() {
        let registry = UsageRegistry::new()
            .with(Box::new(StubProbe::new("claude")))
            .with(Box::new(StubProbe::new("codex").with_credentials(false)));
        assert_eq!(registry.credentialed_ids().await, vec!["claude"]);
    }

    #[tokio::test]
    async fn a_stub_without_a_canned_snapshot_states_it_is_unsupported() {
        let probe = StubProbe::new("grok");
        let result = probe.probe(&ProbeContext::manual()).await;
        assert_eq!(result.availability, Availability::Unknown);
        assert_eq!(result.reason, Some(ReasonCode::UsageNotSupported));
    }
}
