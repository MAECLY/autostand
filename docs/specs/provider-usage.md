# Provider Usage Spec

This document specifies how autostand reads real subscription quota from the AI providers the user is already signed in to, normalises it into a single vocabulary, caches it, and uses it — both to inform the user and to pick a provider that can actually complete a render.

It supersedes the "usage is not programmatically available" stance recorded in
[`../llm-adapters/01-claude.md`](../llm-adapters/01-claude.md) (see [Policy change](#policy-change)).

The design is modelled on [OpenUsage](https://github.com/robinebers/openusage) (MIT), whose provider
pipeline — auth store → usage client → mapper → snapshot — is the reference for this work.

---

## Why

Today `Settings → Providers → Usage & availability` renders six rows and five of them read
**"Usage unavailable"**. Only `openai` reports real percentages, and it does so by spawning
`codex app-server --stdio` and speaking JSON-RPC with an 8-second timeout
(`commands/llm.rs:462-513`). Claude — the provider that actually renders most standups — reports
nothing at all.

Three defects compound this:

| # | Defect | Evidence |
| --- | --- | --- |
| 1 | `get_provider_health` delegates verbatim to `refresh_provider_health(None)`, so opening Settings probes all six providers **sequentially** (including the 8s process spawn) and fires low-usage notifications as a side effect of a render. | `commands/llm.rs:592-594` |
| 2 | The backend emits `provider-health-updated` and `tauri.ts` exports a typed `onProviderHealthUpdated` helper, but **no component subscribes**. Scheduler-driven refreshes never repaint Settings. | `commands/llm.rs:626-629`, `lib/tauri.ts:177-178` |
| 3 | `health.reason` renders only when `compact === false`, and the sole call site passes `compact`. The reason behind *Exhausted* / *Sign-in required* is therefore **never visible**. | `ProviderUsage.tsx:91-93`, `routes/settings.tsx:316-318` |

Two contract variants — `UsageSource::ResponseHeaders` and `UsageSource::ManagementApi` — are
declared in the IPC contract and never constructed (`commands/types.rs:484-496`).

---

## Decisions

These were settled with the user before design and are binding for this spec.

| Decision | Choice | Consequence |
| --- | --- | --- |
| **Credential source** | Read credentials that already exist on the machine, **read-only**. | autostand may read the Claude Code keychain item, `~/.claude/.credentials.json`, `~/.codex/auth.json`, and the equivalents for other providers. It **never writes, refreshes, or rotates** a third-party token. |
| **Provider scope** | Every provider OpenUsage covers that applies to autostand. | Claude, Codex, Cursor, Copilot, Devin, Grok, OpenCode, OpenRouter, Z.ai. See [Exclusions](#exclusions) for Antigravity. |
| **What quota drives** | Informational **and** functional. | Status-bar badge, a pre-flight check before compiling, and health-aware provider selection that skips exhausted providers while still respecting `provider_order`. |
| **HTTP identity** | Mirror the official client. | Requests carry the same `User-Agent` / beta headers the vendor's own CLI sends. See [Risk accepted](#risk-accepted). |

### Risk accepted

Every usage endpoint in this spec is internal and undocumented, and several requests imitate the
vendor's first-party client (`User-Agent: claude-code/<v>`, `anthropic-beta: oauth-2025-04-20`,
`originator: Codex Desktop`). The user chose this explicitly, accepting that a vendor may block or
change these endpoints without notice. Mitigations:

- Every mapper is pure and fixture-tested, so a payload change degrades a provider to `unknown`
  rather than producing wrong numbers.
- The existing rule stands: **never fabricate a percentage or a reset time.** A field the provider
  did not send is `None`, and the UI says "No data" — never `0%`.
- The `User-Agent` version string lives in one constant per provider so it can be bumped in one place.

### Read-only consequence

Because autostand never refreshes a third-party token, an expired access token surfaces as
`auth_required` ("Sign-in required") instead of being silently renewed. In practice this is mild:
both `claude` and `codex` refresh their own tokens whenever the user runs them, so a developer who
used the CLI today has a valid token on disk. The failure mode is a stale row after a long idle
period, cleared by running the CLI once.

This also removes the `refresh_token_reused` race OpenUsage documents against the Codex CLI
(`CodexAuthStore.swift:154-181`) — autostand cannot cause it, because it never calls the refresh
endpoint.

### Exclusions

Stated explicitly so scope is auditable, not silently narrowed:

- **Antigravity** is out of scope. Its OAuth path requires a Google `client_secret` that OpenUsage
  hardcodes in source. Redistributing a third party's client secret is not something autostand will
  ship. If Antigravity's local language-server path proves usable without that secret, it can be
  added later under this same contract.
- **Claude Desktop credentials** are out of scope for the first release. Reading them requires
  PBKDF2-HMAC-SHA1 (1003 iterations, salt `saltysalt`) plus AES-128-CBC over a Chromium cookie
  SQLite — six new crypto crates for a fallback that only matters when Claude Code is *not*
  installed. The Claude Code keychain and credentials file cover the primary case.
- **Spend tiles** (Today / Yesterday / Last 30 days in dollars) are out of scope here. They need a
  maintained model-pricing table; tracked separately.
- **The loopback HTTP API.** OpenUsage exposes `127.0.0.1:6736`. autostand's policy is no listening
  sockets (`../llm-adapters/06-built-in-local.md:51-58`). If headless consumption is needed later it
  ships as a CLI subcommand over the same cache, not a server.

---

## Architecture

### Where the contract lives

Usage moves out of `commands/llm.rs` — today a `match` on provider strings tangled with IPC,
notifications, and event emission (`commands/llm.rs:515-530`) — and into `autostand-adapters` as a
separate trait. It is deliberately **not** a method on `LlmAdapter`: only a subset of providers have
quota, and forcing all six adapters to implement a no-op would spread the contract for nothing.

```rust
// crates/autostand-adapters/src/usage/mod.rs

#[async_trait]
pub trait UsageProbe: Send + Sync {
    /// Stable provider id, matching `LlmAdapter::id()`.
    fn id(&self) -> &'static str;

    /// Cheap, LOCAL-ONLY check for whether credentials exist at all.
    /// Files and keychain only — never the network. Used to decide which
    /// providers are worth listing and probing.
    async fn has_local_credentials(&self) -> bool;

    /// Fetch and normalise. Never panics; a failure is a typed error, not an Err
    /// that blanks the row.
    async fn probe(&self, ctx: &ProbeContext) -> ProviderSnapshot;
}
```

Each implementation is three pieces in its own module, mirroring OpenUsage:

```
crates/autostand-adapters/src/usage/
  mod.rs            UsageProbe, ProbeContext, registry
  model.rs          ProviderSnapshot, UsageResource, ResourceKind, Pace
  parse.rs          clamp_percent, parse_reset_at, decode_json_with_hex_fallback
  http.rs           shared reqwest::Client (OnceLock), header capture
  creds/
    keychain.rs     read-only keychain access
    files.rs        read-only home-relative credential files
  claude/{auth.rs, client.rs, mapper.rs}
  codex/{auth.rs, client.rs, mapper.rs}
  cursor/…, copilot/…, devin/…, grok/…, opencode/…, openrouter/…, zai/…
```

**Mappers are pure functions over a deserialized payload.** They take bytes plus response headers
and return a `ProviderSnapshot`; they perform no I/O. This is what makes fixture testing possible and
is the single most important structural rule in this spec.

### Composition root

A `UsageRegistry` built once in `state.rs` holds the probes. `commands/llm.rs` keeps only IPC
plumbing.

---

## Data model

### Rust

`UsageWindow` today carries a percentage and nothing else, which cannot express credits, dollar
balances, or "N searches remaining". It is widened once — one IPC migration, not three — with every
new field `#[serde(default)]` so older cached snapshots still deserialize.

```rust
/// What a resource measures. `Consumption` fills a meter; `Balance` counts down.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResourceKind { Consumption, Balance }

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum UsageUnit { Percent, Usd, Credits, Requests, Tokens, Count }

pub struct UsageWindow {
    /// Stable, provider-defined id: `session`, `weekly`, `sonnet`, `credits`, …
    pub id: String,
    pub used_percent: Option<f64>,
    pub remaining_percent: Option<f64>,
    pub resets_at: Option<String>,

    // --- added by this spec, all #[serde(default)] ---
    #[serde(default)] pub kind: Option<ResourceKind>,
    #[serde(default)] pub unit: Option<UsageUnit>,
    /// Raw scalars, so the UI formats at the display edge instead of re-parsing strings.
    #[serde(default)] pub used: Option<f64>,
    #[serde(default)] pub limit: Option<f64>,
    #[serde(default)] pub available: Option<f64>,
    /// Window length. `parse_codex_window` already computes this and throws it away.
    #[serde(default)] pub period_duration_ms: Option<i64>,
    /// Human label when the provider names the window itself (e.g. a model-scoped limit).
    #[serde(default)] pub label: Option<String>,
    #[serde(default)] pub pace: Option<Pace>,
    /// The same projection as a countdown, so the pre-flight never re-derives it.
    #[serde(default)] pub runs_out_in_seconds: Option<f64>,
}

pub struct ProviderHealth {
    pub provider: String,
    pub availability: ProviderAvailability,
    pub source: UsageSource,
    #[serde(default)] pub windows: Vec<UsageWindow>,
    pub reason: Option<String>,
    pub checked_at: String,

    // --- added by this spec ---
    /// "Max 20x", "Pro 20x", "Team 5x" — context for interpreting a percentage.
    #[serde(default)] pub plan: Option<String>,
    /// True when this snapshot was served from cache after a failed refresh.
    #[serde(default)] pub stale: bool,
    /// Non-fatal notice shown beside the provider name ("Live usage rate limited — retry in ~4m").
    #[serde(default)] pub notice: Option<String>,
}
```

`ProviderAvailability` and `UsageSource` keep their variants. `ResponseHeaders` finally gets
constructed (Codex fills percentages from `x-codex-*-used-percent` when the body omits them), which
requires `http_get_json` / `http_post_json` to return headers instead of discarding them.

### TypeScript

`lib/types.ts` mirrors the above exactly. Every added field is optional, so no existing consumer
breaks.

### Percent and timestamp hygiene

One module, `usage::parse`, applied at the **single point of construction** — never scattered:

- `clamp_percent(v)` → `0.0..=100.0`; non-finite becomes `None`, not `0`.
- `parse_reset_at(v)` accepts RFC 3339 (fractional seconds normalised to 3 digits), an epoch number
  (`abs(n) < 1e10` ⇒ seconds, else milliseconds), or a relative `reset_after_seconds` resolved
  against `now`.
- `decode_json_with_hex_fallback(bytes)` — several vendors store credential JSON hex-encoded.

---

## Pace

Ported from OpenUsage's `Pace.evaluate` as a pure module in `autostand-core::pace` (~60 LOC, no
dependencies, trivially table-testable). It answers the question the user actually has before
compiling: *will this run out before the window resets?*

```rust
pub enum Pace { Ahead, OnTrack, Behind }

pub fn evaluate(used: f64, limit: f64, elapsed: Duration, period: Duration) -> Option<Pace> {
    // No projection until enough of the window has elapsed, or early noise
    // produces absurd extrapolations.
    let minimum = max(Duration::from_secs(60), period / 100);
    if elapsed < minimum { return None; }

    let projected = used / elapsed.as_secs_f64() * period.as_secs_f64();
    Some(match projected {
        p if p <= limit * 0.9 => Pace::Ahead,
        p if p <= limit       => Pace::OnTrack,
        _                     => Pace::Behind,
    })
}

pub fn seconds_to_run_out(used: f64, limit: f64, elapsed: Duration) -> Option<f64>;
```

Meter colour resolves by precedence: `no data → spent → pace → absolute bands (80% / 90%)`, fed to
the existing `Progress` component through `indicatorClassName` (already supported by the UI kit,
currently unused).

`UsageResource::derive_projection` computes both answers from one elapsed value and stores the
countdown in `runs_out_in_seconds`, so `pace` and "~35 min" can never contradict each other. The
countdown rides on `evaluate`'s minimum-elapsed guard: without it, one request in the first seconds
of a five-hour window would read as "runs out in ~40 seconds". A spent window reports a `pace` and no
countdown — exhaustion is a state, not a countdown.

---

## Provider contracts

Full per-provider request/response details live in [`../llm-adapters/`](../llm-adapters/), one page
per provider. The two that matter most are specified here; the rest follow the same shape.

### Claude

**Credentials (read-only, in order).** Keychain always beats file; the loop advances to the next
candidate only on expiry-class errors.

1. macOS keychain, service `"Claude Code" + <suffix> + "-credentials"`. When `CLAUDE_CONFIG_DIR` is
   set, try `<base>-<sha256(configDir NFC)[..8]>` first, then `<base>`. Per service: current-user
   lookup (`-a $USER`) first, then service-only (legacy).
2. `$CLAUDE_CONFIG_DIR/.credentials.json`, else `~/.claude/.credentials.json`. Shape:
   `{ "claudeAiOauth": { accessToken, refreshToken, expiresAt (epoch ms), subscriptionType, rateLimitTier, scopes[] } }`.
   Hex-encoded JSON accepted.
3. `CLAUDE_CODE_OAUTH_TOKEN` — **last** candidate only. A `claude setup-token` value can run
   inference but cannot read subscription limits, so it must never shadow a real login.

**Request.** `GET https://api.anthropic.com/api/oauth/usage`, timeout 10s:

```
Authorization: Bearer <accessToken>
Accept: application/json
Content-Type: application/json
anthropic-beta: oauth-2025-04-20
User-Agent: claude-code/<CLAUDE_UA_VERSION>
```

No `anthropic-version` header — the vendor's own client omits it here.

**Mapping.**

| Payload | Resource |
| --- | --- |
| `five_hour.{utilization, resets_at}` | `session`, percent, period 5h |
| `seven_day.{…}` | `weekly`, percent, period 7d |
| `seven_day_sonnet.{…}` | `sonnet`, percent, period 7d |
| `limits[] where kind == "weekly_scoped"` | one row per entry, labelled from `scope.model.display_name` |
| `extra_usage.{is_enabled, used_credits, monthly_limit}` (cents) | `extra_usage`; bounded → `Consumption`/`Usd`, unbounded → `Balance`/`Usd` |
| `subscriptionType` + `\d+x` from `rateLimitTier` | `plan` → `"Max 20x"` |

**Scope gate.** Reading usage requires the `user:profile` scope. If `scopes` is non-empty and lacks
it, the provider reports `availability: available` with
`notice: "Re-login for live usage"` rather than an error — inference still works, only the meters
are unavailable. Absent/empty `scopes` (older credentials) is treated as capable.

**429 cooldown.** Parse `Retry-After` (integer seconds or HTTP date). Set
`rate_limited_until = now + retry_after.unwrap_or(300s)`, serve the last good snapshot with
`stale: true` and a notice, and **do not call the endpoint at all** during the cooldown — including
on a manual refresh, which is exactly when a user hammering the button would make it worse. The
cache is keyed by `sha256(access_token)` so a different login starts with a clean cooldown.

### Codex

**Credentials (read-only).** `$CODEX_HOME/auth.json` if set, else `~/.config/codex/auth.json` then
`~/.codex/auth.json`; keychain service `"Codex Auth"` as fallback. Shape:
`{ tokens: { access_token, refresh_token, id_token, account_id }, last_refresh, OPENAI_API_KEY }`.

An auth file carrying **only** `OPENAI_API_KEY` yields the typed reason `usage_requires_cli_login`,
and the UI says so — an API key cannot see subscription quota, and reporting a generic
"Unavailable" for it is a support question waiting to happen.

Token expiry is detected by decoding the JWT `exp` claim. Under the read-only decision, a token
within 300s of expiry is reported as `auth_required` rather than refreshed.

**Request.** `GET https://chatgpt.com/backend-api/wham/usage`, timeout 10s:

```
Authorization: Bearer <access_token>
Accept: application/json
User-Agent: autostand/<version>
ChatGPT-Account-Id: <account_id>      # when present
```

This replaces the `codex app-server --stdio` spawn entirely: no child process, no 8-second timeout,
and it works whether or not the `codex` CLI is on `PATH`.

**Mapping.** Windows are classified by **duration**, not slot position: `limit_window_seconds == 18000`
→ `session`, `== 604800` → `weekly`. Positional `primary`/`secondary` is the fallback only when no
duration is recognisable — this matters because the vendor sometimes drops one limit and promotes the
weekly window into the primary slot. Response headers `x-codex-primary-used-percent`,
`x-codex-secondary-used-percent`, `x-codex-credits-balance` fill values the body omits
(`UsageSource::ResponseHeaders`).

`credits.balance` → `credits` (`Balance`/`Credits`). `plan_type` maps `prolite → "Pro 5x"`,
`pro → "Pro 20x"`, otherwise title-case over `_`.

**Not in scope:** claiming rate-limit reset credits. That is an irreversible account mutation and
belongs nowhere near a usage panel.

### Remaining providers

Cursor, Copilot, Devin, Grok, OpenCode, OpenRouter, Z.ai follow the same triad and land in phase 3.
OpenRouter and Z.ai have no local credential to reuse, so they read a user-supplied API key from the
existing autostand keychain — the one credential path that was already sanctioned.

---

## Refresh, cache, and backoff

### Split the commands

The root cause of defect 1 is that `get_provider_health` and `refresh_provider_health` are the same
function. They separate:

| Command | Behaviour |
| --- | --- |
| `get_provider_health` | **Pure cache read.** Never probes, never notifies, never blocks. Returns whatever is on disk, marked `stale` when older than the TTL. |
| `refresh_provider_health(provider?)` | The only prober. Concurrent across providers via `JoinSet`. Emits `provider-health-updated`. Evaluates notification thresholds. |

This changes documented IPC semantics, so `../tauri/02-ipc-contracts.md` is updated in the same
change.

### Cache

`state_dir()/provider-health.json`, written with the temp + fsync + rename pattern already used by
`notification-history.json`. Versioned schema key; a version bump discards rather than migrates.

Freshness follows OpenUsage's two-condition rule: a snapshot counts as *fresh enough to skip a probe*
only if it was written **during the current process run** *and* is within the TTL. A snapshot from a
previous launch still paints immediately, but always triggers one refresh — so a new app version
never shows the old version's numbers.

**Errors are never cached.** A failed probe leaves the last good snapshot in place, sets
`stale: true`, and attaches a reason. A row that has never had data reads "No data".

An account stamp (`sha256` of the credential) is stored alongside each snapshot; if the signed-in
account changed between launches the cached values are discarded instead of briefly showing the
previous account's quota under the new login.

### Cadence and backoff

- One interval constant is both the refresh period and the cache TTL, so they cannot drift apart.
  Default 5 minutes.
- Providers refresh **concurrently**; a provider taking ≥10s is logged as slow.
- An in-flight guard per provider prevents overlapping probes.
- After a failure, a 60s per-provider backoff. A manual refresh ignores backoff — but **not** an
  explicit `Retry-After` cooldown.
- Only providers that are `enabled` **and** pass `has_local_credentials()` are probed or listed. This
  alone removes the five "Usage unavailable" rows.

---

## Using the quota

### Live UI updates

A `useProviderHealthLive()` hook subscribes `onProviderHealthUpdated` and writes through
`queryClient.setQueryData(providerHealthKey, …)`, closing the orphaned event (defect 2). Per-row
refresh uses `refresh_provider_health({ provider })`, which the backend already accepts and the
component never used.

### Status bar

A compact badge showing the active provider's tightest window, so quota is visible where the user
decides to compile rather than only inside a Settings tab. "Active" is resolved exactly as
`render::provider_chain` resolves it — head of `provider_order`, else `preferred_provider` — so the
badge can never name one provider while the render uses another.

**No data renders nothing.** A provider with no snapshot, or with no window that reports a share,
gets no badge at all. A chip reading `0%` or `—` would be a claim about a provider nobody measured.

### Pre-flight

Before a compile, if the selected provider's tightest window is at or below the configured low
threshold, the UI states the fact and offers the fallback:

> Claude — 12% of the 5 h window left, projected to run out in ~35 min. Use Codex instead?

The projection clause is **dropped, not softened**, when there is none: `runs_out_in_seconds` rides
on `pace`'s minimum-elapsed guard, so a window too young to project says only the percentage.

The dialog never blocks. "Compile anyway" is the primary action and holds initial focus, so anyone
who already knows their quota dismisses it with Enter. Choosing the alternative moves that provider
to the head of `provider_order` (the rest of the order is preserved) and **waits for the save**
before compiling — the render reads the chain from the config store, so firing both at once could
still render on the provider the user just declined. The offer is withheld when no other configured
provider is in better shape.

### Health-aware fallback

`provider_order` remains the user's preference and is still honoured. Selection changes only in that
providers whose latest snapshot is `exhausted`, `rate_limited`, or `auth_required` are **skipped**
rather than attempted-and-failed, when `fallback_enabled` is on. A provider with `unknown` usage is
never skipped — absence of data is not evidence of exhaustion.

Three guards keep that invariant honest, all in `render::health_skip_reason`:

| Guard | Why |
| --- | --- |
| `unknown`, `low`, `unavailable` and `model_unavailable` never skip | `unknown` is where every provider sits until the first refresh; skipping on it would leave a working machine with no provider. `low` is what the pre-flight warns about, not a refusal. |
| A snapshot older than **6 h** stops being evidence | Five hours is the shortest quota window any supported provider defines, so an older `exhausted` reading says nothing about the window this render draws from. |
| Nothing is skipped while `fallback_enabled` is off | The chain is one provider long; skipping it would replace a real attempt with silence. |

The skip is recorded in `provider_attempts` as `status: "skipped"` with reason `usage_exhausted`,
`usage_rate_limited` or `usage_auth_required`, so the audit sidecar explains the rotation. A skipped
attempt is **not** fed back into the failure-inferred cache: a skip is a consequence of an earlier
observation, not a new one, and recording it would let `exhausted` decay into `unavailable` on every
render that passed the provider over.

### One threshold

`notifications.low_usage_threshold_percent` is the single source for the badge, the pre-flight, the
`ProviderSnapshot::graded` downgrade to `low`/`exhausted`, and the low-usage notification, so they
cannot disagree. Nothing hardcodes a percentage: the `20.0` that used to live in `parse_codex_health`
went out with the `codex app-server` spawn, and the frontend reads the config value rather than a
constant of its own.

---

## Errors

Each provider defines a `thiserror` enum, and one module maps every variant to
`(ProviderAvailability, reason_code)` **exhaustively** — no wildcard arm — so adding a variant is a
compile error rather than a silent fallthrough to `unknown`. This replaces today's substring
allow-list over stderr and response bodies.

Reason codes are stable, lowercase, secret-free identifiers: `not_logged_in`, `session_expired`,
`missing_profile_scope`, `usage_requires_cli_login`, `rate_limited`, `network`, `unsupported_payload`.
The UI owns the human string.

`../specs/audit.md` already forbids stderr and API bodies in attempt telemetry. That holds: no
response body, header, or token is ever logged, at any level.

---

## Privacy and security

### Policy change

`../llm-adapters/01-claude.md:35` records `unknown` as a deliberate decision. This spec reverses it,
and that file plus `../architecture/05-security.md` are updated in the same change to state:

- autostand reads third-party credential files and keychain items **read-only**, solely to query that
  vendor's own usage endpoint.
- It never writes, refreshes, rotates, or deletes a third-party credential.
- Tokens are never logged, never persisted by autostand, never sent anywhere except that vendor's own
  endpoint, and never included in audit records. Only a `sha256` fingerprint is retained, for cache
  invalidation.
- Usage reads are opt-in per provider, and the panel names the file or keychain item each reading came
  from.

### Keychain prompts

macOS may prompt before a non-owning app reads another app's keychain item. OpenUsage confines that
interaction to manual refresh so background passes never raise a dialog; autostand does the same via
a `ProbeContext { is_manual: bool }` flag. **The exact behaviour of the `keyring` crate from a Tauri
binary — signed and unsigned — is unverified and must be measured before this ships.** If it prompts
on background refresh, the fallback is the credentials file only.

### Cross-platform

`tauri.conf.json` declares `"targets": "all"`. Only the **file-based** credential paths are portable.
Keychain reads are macOS-only; Windows and Linux degrade to file paths and report
`not_logged_in` when there is no file. This is a deliberate, documented degradation, not a bug.

---

## Testing

Ordered so that mappers are never written by guessing at payload shapes.

1. **Capture fixtures first.** Real responses from `/api/oauth/usage` and `/wham/usage` on the
   developer's own account, redacted and committed under
   `crates/autostand-adapters/tests/fixtures/usage/`. OpenUsage does not ship complete payloads for
   every provider, so its mappers are evidence of field names, not a substitute for a real response.
2. **Mapper table tests** — pure input → `ProviderSnapshot`, including: missing optional fields,
   percentages out of range, epoch-seconds vs epoch-milliseconds resets, the duration-vs-positional
   Codex fallback, and header-filled percentages.
3. **`autostand-core::pace`** table tests, including the minimum-elapsed guard.
4. **Cache tests** — TTL, current-run freshness rule, errors not cached, account-stamp invalidation,
   atomic write under a simulated crash.
5. **Frontend** — `ProviderUsage.tsx` has zero tests today. Add `makeProviderHealth` /
   `makeUsageWindow` factories to `src/test/mocks.ts`, cover the availability→variant map, window
   labels, reset parsing, pending/error/empty states, and populate the E2E mock (it returns `[]`
   today, so the section is empty in E2E).
6. **Fix `provider_preset_live.rs`**, which does not compile under `--features live-e2e` (E0063,
   missing `local_runtime_policy`).

No live network test runs in CI.

---

## Phasing

Each phase ends with a granular conventional commit and a push.

| Phase | Content |
| --- | --- |
| **0** | Fix the three defects: split `get_provider_health` from `refresh_provider_health`, subscribe `onProviderHealthUpdated`, surface `reason`. Filter to enabled providers. Concurrent probing. No new credential reads — this phase is a strict improvement on its own. |
| **1** | Widen the DTOs, add `usage::parse`, `autostand-core::pace`, the shared HTTP client with header capture, the disk cache, and backoff. Still no new credential reads. |
| **2** | `UsageProbe` trait + Claude and Codex probes. Replaces the `codex app-server` spawn. Fixtures and mapper tests. Docs and policy updates land here. |
| **3** | Remaining providers: Cursor, Copilot, Devin, Grok, OpenCode, OpenRouter, Z.ai. |
| **4** | Functional use: status-bar badge, pre-flight, health-aware fallback, unified threshold. |

Phase 0 alone fixes a real performance and correctness problem and can ship independently of every
decision about third-party credentials.
