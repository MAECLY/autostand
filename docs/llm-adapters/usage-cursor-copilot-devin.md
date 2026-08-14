# Usage probes — Cursor, Copilot, Devin

Phase 3b of [`../specs/provider-usage.md`](../specs/provider-usage.md). These three providers are
**usage-only**: autostand reads their subscription quota but never renders a standup with them, so
they implement `UsageProbe` and not `LlmAdapter`.

Every rule from the spec applies unchanged: credentials are read-only, mappers are pure functions of
`(payload, now)`, a field the vendor did not send is `None`, and no token, header, URL or response
body reaches a log, an error or a DTO.

---

## Cursor — `crates/autostand-adapters/src/usage/cursor/`

**Credentials (read-only).** The editor's own key/value store, then the keychain.

1. `state.vscdb` → `ItemTable['cursorAuth/accessToken']`. Opened through
   `usage::creds::vscdb` with `SQLITE_OPEN_READ_ONLY`, with the key bound as a parameter.
   `OpenUsage` shells out to `/usr/bin/sqlite3 -readonly`; this crate already links `rusqlite`, so
   the read happens in-process — no subprocess, no shell quoting.
   Paths: `~/Library/Application Support/Cursor/User/globalStorage/` (macOS),
   `%APPDATA%\Cursor\User\globalStorage\` (Windows), `$XDG_CONFIG_HOME/Cursor/User/globalStorage/`
   (Linux).
2. macOS keychain, service `cursor-access-token`. Read only on a **manual** refresh
   (`ProbeContext::is_manual`).

The editor store wins, with one exception carried over from `OpenUsage`: when it reports
`cursorAuth/stripeMembershipType == "free"` **and** the keychain token's JWT `sub` names a different
account, the keychain token wins — the editor is signed in to a second, unpaid account.

**Read-only consequence.** `OpenUsage` exchanges Cursor's refresh token and writes the rotated
access token back into Cursor's own database. autostand does neither: it never reads the refresh
token at all, and a token whose `exp` has passed is reported as `session_expired`.

**Requests.**

| Endpoint | Purpose | Required |
| --- | --- | --- |
| `POST api2.cursor.sh/aiserver.v1.DashboardService/GetCurrentPeriodUsage` | plan usage | yes |
| `POST …/GetPlanInfo` | plan label, and the fallback decision | no |
| `POST …/GetCreditGrantsBalance` | prepaid grants | no |
| `GET cursor.com/api/auth/stripe` | prepaid balance | no |
| `GET cursor.com/api/usage?user=<id>` | request-metered plans | no |

The RPCs carry `Authorization: Bearer <token>` and `Connect-Protocol-Version: 1`; the `cursor.com`
endpoints authenticate with the `WorkosCursorSessionToken=<account>%3A%3A<token>` cookie. The
account id comes from the JWT `sub` and is rejected unless it is alphanumeric plus `-_.`, so nothing
read from a token can reshape a request line. Every optional endpoint failing costs one resource,
never the snapshot.

**Resources.** `totalUsage`, `autoUsage`, `apiUsage`, `onDemand`, `credits`, `requests`.

`totalUsage`'s **unit varies with the account shape** — percent on an individual plan, USD on a team
or pooled plan, requests on a request-metered plan — which is why `UsageResource` carries an explicit
`UsageUnit` rather than assuming a percentage. Money arrives in cents and is converted once, at
construction. A bounded meter (dollars of a cap, requests of an allowance) also carries the derived
`used_percent`, so it can take part in low-quota grading; that is arithmetic on two reported figures,
not an invented percentage. `onDemand` without a stated cap is a spent total with **no** limit.

**Not ported, deliberately.**

- **Per-model / long-context thresholds.** Cursor publishes per-model consumption only as a
  row-aggregated CSV export from the dashboard — there is no structured endpoint. The meters here are
  therefore plan-wide. This is a limit of what Cursor exposes, inherited from `OpenUsage`, not a
  simplification made here. The CSV is also the source of the dollar spend tiles, which the spec puts
  out of scope.
- **The enterprise `usage-summary` mapper.** `OpenUsage` combines `cursor.com/api/usage-summary` with
  the request endpoint for enterprise dashboards. Those accounts here fall back to the request-metered
  mapping alone; if it yields nothing they report `unsupported_payload` rather than a guessed meter.

## Copilot — `crates/autostand-adapters/src/usage/copilot/`

**Credentials (read-only), in order.** Prompt-free files first, keychain last.

1. `~/.config/github-copilot/apps.json`, then `hosts.json` — what the VS Code / JetBrains / Neovim
   plugins write. Only `github.com` and `github.com:<appId>` entries are used: an Enterprise token
   must never be sent to `api.github.com`.
2. `~/.config/gh/hosts.yml` → `oauth_token`, scoped to the `github.com` block. `$GH_CONFIG_DIR` and
   the Windows `%APPDATA%\GitHub CLI\` layout are honoured.
3. macOS keychain, service `gh:github.com`, `go-keyring-base64:` wrapped. Manual refresh only.

**No refresh path, by design.** These tokens belong to other tools; rotating one would sign the user
out of them. A rejected token moves the probe to the next source — a small, deliberate improvement on
`OpenUsage`, which stops at the first source — and when none is left the provider reports
`session_expired`.

**Requests.** `GET api.github.com/copilot_internal/user`, mirroring the official client
(`Authorization: token …` — not `Bearer` — plus `Editor-Version`, `Editor-Plugin-Version`,
`User-Agent: GitHubCopilotChat/…`, `X-Github-Api-Version`). For an org-managed seat only:
`GET /user/orgs?per_page=100` and `GET /orgs/{org}/settings/billing/usage/summary`.

**Resources.** `premiumCredits`, `extraUsage`, `chat`, `completions` from the seat;
`orgCredits`, `orgSpend` from org billing (`UsageSource::ManagementApi`).

Suppressed rather than drawn as a misleading `0%`: an `unlimited` bucket, the `-1`
entitlement/remaining sentinel, and a zero-entitlement placeholder. `extraUsage` appears only once
overage spend is enabled, where a zero is a measured zero. `orgCredits`/`orgSpend` are consumed
totals with no limit — GitHub exposes no allotment, so no percentage is fabricated.

A Copilot Business seat managed by an organization reports a placeholder for every bucket. That is a
legitimate empty state: the plan is shown with no meters, and organization billing is consulted
instead. A 403 there is the expected answer for a plain member and keeps the plan-only card. The
organization that answered is remembered **in process** (a slug, never a token), so a steady-state
refresh makes one billing call; discovery is capped at 10 organizations.

## Devin — `crates/autostand-adapters/src/usage/devin/`

**Credentials (read-only), in order.**

1. `~/.local/share/devin/credentials.toml` (`$XDG_DATA_HOME` honoured) → `windsurf_api_key`, with an
   optional `api_server_url`. The host is accepted **only** when it is `https`: the API key travels
   in the request body, so a plaintext host would put it on the wire in the clear.
2. The Devin app's `state.vscdb` → `ItemTable['windsurfAuthStatus']`, whose JSON carries `apiKey`.

Both are tried; identical logins are de-duplicated so the second never spends the user's rate limit.

**Request.** `POST {api_server_url}/exa.seat_management_pb.SeatManagementService/GetUserStatus`
(default host `https://server.codeium.com`), a Connect-RPC call whose `metadata` carries the API key
and the Devin editor identity.

**Resources.** `daily` and `weekly` (percent, one-day and seven-day windows) and
`extraUsageBalance` (a USD balance from `overageBalanceMicros`).

Devin reports quota as percent **remaining**; every meter flips it to percent used. A plan with
`hideDailyQuota` drops the `daily` row, and its hidden daily figure fills `weekly` only when Devin
sent no weekly figure of its own. A present balance of zero stays `0`; an absent one is "No data".

---

## Reason-code gap

`ReasonCode` has no variant for "the account has no active subscription" (Cursor's `enabled: false`)
or for "the payload states no allowance". Both currently classify as
`(Unknown, unsupported_payload)` — honest in that nothing was measured, but less specific than the
fact warrants. Adding a variant touches `usage::model`, the IPC contract and the UI copy together, so
it is tracked separately rather than smuggled in here.
