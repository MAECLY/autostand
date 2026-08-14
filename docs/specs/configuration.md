# Configuration Spec

This document specifies how `autostand` stores, validates, and overrides configuration: storage backend, the `AppConfig` schema (Rust + TypeScript), environment-variable overrides, defaults, the Settings UI, and keychain API-key handling.

---

## Config storage

`autostand` writes config as JSON via the Tauri Store plugin (`tauri-plugin-store`). The Store plugin writes to the platform config dir:

| Platform | Path |
| --- | --- |
| macOS | `~/Library/Application Support/autostand/config.json` |
| Linux | `~/.config/autostand/config.json` |
| Windows | `%APPDATA%\autostand\config.json` |

The store is loaded once at app startup into `AppState` and re-serialized on every `set_config` call. Reads are in-memory after load.

### Env var overrides (headless / CLI mode)

When the scheduler runs `autostand --run` headlessly (e.g. via launchd), the same config file is read, but every field can be overridden by an environment variable. Env vars take **precedence** over the JSON file. This preserves backward compatibility with the prior App Script, which was entirely env-driven.

API keys are **never** in the config JSON or env vars. They live in the OS keychain (see [Keychain API keys](#keychain-api-keys)).

---

## `AppConfig` schema

### Rust

```rust
// crates/autostand-core/src/config.rs
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub github_dir: PathBuf,            // default ~/Documents/Github
    pub dailies_dir: PathBuf,            // default <repo>/dailies
    pub standup_authors: Vec<String>,    // git author emails/usernames to match as "me"
    pub git_refs: String,                // default "--all"
    pub jira_base: String,               // default "https://fiftyflowers.atlassian.net/browse"
    pub host_slug_override: Option<String>,
    pub render_mode: RenderMode,         // Auto | Llm | Det
    pub llm: LlmConfig,
    pub data_sources: DataSourceConfigs,
    pub scheduler: SchedulerConfig,
    pub review: ReviewConfig,
    pub scrub: ScrubConfig,
    pub format: StandupFormatConfig,
    #[serde(default)]
    pub notifications: NotificationConfig,
    #[serde(default)]
    pub dates: DatesConfig,             // which file a day's work is archived under
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum RenderMode { Auto, Llm, Det }

/// Filing-date policy. `#[serde(default)]` on both the field and the section:
/// a `config.json` written before this block existed must keep loading, and
/// must load into the App Script's rule rather than silently moving the user's
/// standups to a different file.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct DatesConfig {
    #[serde(default)]
    pub archive_mode: ArchiveMode,      // next_business_day | same_day
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveMode {
    #[default]
    NextBusinessDay,                    // work on D → next_business_day(D).md
    SameDay,                            // work on D → D.md (weekend rolls to Monday)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmConfig {
    pub preferred_provider: String,     // cloud/CLI provider id or "builtin-local"
    pub providers: Vec<ProviderConfig>,
    #[serde(default = "default_fallback_enabled")]
    pub fallback_enabled: bool,         // default true
    #[serde(default)]
    pub provider_order: Vec<String>,    // explicit priority, first provider is preferred
    #[serde(default)]
    pub fallback_policy: ProviderFallbackPolicy,
    #[serde(default)]
    pub local_runtime_policy: LocalRuntimePolicy, // on_demand | keep_ready
}

pub struct ProviderFallbackPolicy {
    pub retry_rate_limits: bool,        // default true
    pub max_retry_after_secs: u64,      // default 30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub id: String,                      // "claude" etc.
    pub enabled: bool,
    pub mode: ProviderMode,              // CliFirst | ApiFallback | CliOnly | ApiOnly
    pub model: String,                   // e.g. "sonnet", "gpt-5-codex", "grok-4.5"
    pub cli_path: Option<PathBuf>,       // override binary path
    pub api_key_ref: Option<String>,     // keychain reference (not the key itself)
    pub api_base_url: Option<String>,    // for self-hosted Ollama, etc.
    pub timeout_secs: u64,               // default 180
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub enum ProviderMode { CliFirst, ApiFallback, CliOnly, ApiOnly }

pub struct NotificationConfig {
    pub enabled: bool,                  // default false; OS permission is separate
    pub low_usage: bool,                // default true
    pub low_usage_threshold_percent: u8,// default 20; normalized to 0..=100
    pub provider_exhausted: bool,       // default true
    pub provider_fallback: bool,        // default true
    pub local_model_downloads: bool,    // default true
    pub standup_complete: bool,         // default false
    pub standup_failed: bool,           // default true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceConfigs {
    pub local_git: bool,                 // always true (authoritative)
    pub github: bool,
    pub claude_code: bool,
    pub remember: bool,
    pub opencode: bool,
    pub codex: bool,
    pub gemini_cli: bool,
    pub grok_cli: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchedulerConfig {
    pub enabled: bool,
    pub cron: String,                    // default "0 7-19 * * 1-5" (hourly 07-19 weekdays)
    pub self_heal: bool,                 // recompile prev business day
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewConfig {
    pub reviewer: String,                // GitHub login for "me"
    pub pr_org: String,                  // GitHub org to search
    pub max_prs: u32,                     // default 10
    pub comment_len: u32,                 // default 220
    pub include_self_reviews: bool,      // default false
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubConfig {
    pub alias_scrub: bool,               // default false
    pub alias_scrub_min: u32,             // default 2
    pub meta_extra: Option<String>,       // regex pipe-separated to extend meta-work filter
}
```

### TypeScript (mirror, lives in `src/lib/types.ts`)

```ts
export interface AppConfig {
  github_dir: string;
  dailies_dir: string;
  standup_authors: string[];
  git_refs: string;
  jira_base: string;
  host_slug_override: string | null;
  render_mode: "Auto" | "Llm" | "Det";
  llm: LlmConfig;
  data_sources: DataSourceConfigs;
  scheduler: SchedulerConfig;
  review: ReviewConfig;
  scrub: ScrubConfig;
  format: StandupFormatConfig;
  notifications: NotificationConfig;
  dates: DatesConfig;
}

export interface DatesConfig {
  archive_mode: "next_business_day" | "same_day";
}

export interface LlmConfig {
  preferred_provider: string;
  providers: ProviderConfig[];
  fallback_enabled: boolean;
  provider_order: string[];
  fallback_policy: {
    retry_rate_limits: boolean;
    max_retry_after_secs: number;
  };
  local_runtime_policy: "on_demand" | "keep_ready";
}

export interface NotificationConfig {
  enabled: boolean;
  low_usage: boolean;
  low_usage_threshold_percent: number;
  provider_exhausted: boolean;
  provider_fallback: boolean;
  local_model_downloads: boolean;
  standup_complete: boolean;
  standup_failed: boolean;
}

export interface ProviderConfig {
  id: string;
  enabled: boolean;
  mode: "CliFirst" | "ApiFallback" | "CliOnly" | "ApiOnly";
  model: string;
  cli_path: string | null;
  api_key_ref: string | null;
  api_base_url: string | null;
  timeout_secs: number;
}

export interface DataSourceConfigs {
  local_git: boolean;
  github: boolean;
  claude_code: boolean;
  remember: boolean;
  opencode: boolean;
  codex: boolean;
  gemini_cli: boolean;
  grok_cli: boolean;
}

export interface SchedulerConfig {
  enabled: boolean;
  cron: string;
  self_heal: boolean;
}

export interface ReviewConfig {
  reviewer: string;
  pr_org: string;
  max_prs: number;
  comment_len: number;
  include_self_reviews: boolean;
}

export interface ScrubConfig {
  alias_scrub: boolean;
  alias_scrub_min: number;
  meta_extra: string | null;
}
```

---

## Env var overrides

| Env var | Field | Type | Backward compat? |
| --- | --- | --- | --- |
| `GITHUB_DIR` | `github_dir` | path | Yes (App Script) |
| `DAILIES_DIR` | `dailies_dir` | path | Yes |
| `STANDUP_AUTHORS` | `standup_authors` | pipe-separated list | Yes |
| `STANDUP_GIT_REFS` | `git_refs` | string | Yes |
| `JIRA_BASE` | `jira_base` | URL | Yes |
| `STANDUP_HOST` | `host_slug_override` | string | Yes |
| `STANDUP_RENDER` | `render_mode` | `Auto`\|`Llm`\|`Det` | Yes |
| `STANDUP_MODEL` | `llm.preferred_provider`'s `model` | string | Yes |
| `STANDUP_REVIEWER` | `review.reviewer` | GitHub login | Yes |
| `STANDUP_PR_ORG` | `review.pr_org` | GitHub org | Yes |
| `STANDUP_GH_MAX_PRS` | `review.max_prs` | u32 | Yes |
| `STANDUP_GH_COMMENT_LEN` | `review.comment_len` | u32 | Yes |
| `STANDUP_PR_REVIEW_INCLUDE_SELF` | `review.include_self_reviews` | bool | Yes |
| `STANDUP_ALIAS_SCRUB` | `scrub.alias_scrub` | bool | Yes |
| `STANDUP_ALIAS_SCRUB_MIN` | `scrub.alias_scrub_min` | u32 | Yes |
| `STANDUP_META_EXTRA` | `scrub.meta_extra` | pipe-separated regex | Yes |
| `AUTOSTAND_RENDER` | (anti-recursion guard) | set to `1` when the LLM render subshell runs to prevent reentry | **New** |
| `AUTOSTAND_LLM_PROVIDER` | `llm.preferred_provider` | string | **New** |
| `AUTOSTAND_LLM_MODE` | `provider.mode` for the preferred provider | `CliFirst`\|`ApiFallback`\|`CliOnly`\|`ApiOnly` | **New** |

### Resolution precedence

1. Env var (highest)
2. Config JSON
3. Built-in default (lowest)

### `AUTOSTAND_RENDER` anti-recursion guard

When the LLM render step spawns a subprocess (e.g. `claude` CLI) to render the standup, `autostand` sets `AUTOSTAND_RENDER=1` in the subprocess env. The pipeline checks this at `trigger()` entry:

```rust
if std::env::var_os("AUTOSTAND_RENDER").is_some() {
    anyhow::bail!("refusing to run inside an autostand render subprocess");
}
```

This prevents a render CLI from re-invoking `autostand` and recursing.

---

## Defaults

| Field | Default | Description |
| --- | --- | --- |
| `github_dir` | `~/Documents/Github` | Where `discover_repos` scans |
| `dailies_dir` | `<github_dir>/dailies` (resolved to a repo's dailies) | Standup output dir |
| `standup_authors` | `[]` → this machine's `git config` identity | git author emails/usernames matching "me". Edited in Settings → Paths → Commit authors |
| `git_refs` | `--all` (blank counts as `--all`) | `git log` ref selector. Edited in Settings → Paths → Commit authors → Advanced |
| `jira_base` | `https://fiftyflowers.atlassian.net/browse` | Jira URL prefix for ticket links |
| `host_slug_override` | `None` | Manual host slug override (else detected) |
| `render_mode` | `Auto` | `Auto` = CLI-first with API fallback; `Llm` = force LLM; `Det` = deterministic only |
| `llm.preferred_provider` | `claude` | Default provider id |
| `llm.fallback_enabled` | `true` | Continue through the configured provider priority after a provider fails |
| `llm.provider_order` | `[]` | Empty preserves legacy order: preferred provider, then stored providers |
| `llm.fallback_policy.retry_rate_limits` | `true` | Retry one rate-limited transport when it reports a bounded reset |
| `llm.fallback_policy.max_retry_after_secs` | `30` | Maximum reported delay to wait before advancing |
| `llm.local_runtime_policy` | `on_demand` | Built-in local runtime lifecycle; `keep_ready` reuses a model-scoped llama.cpp prompt/KV cache for manual and scheduled renders |
| `provider.enabled` | `true` for `claude`, `false` for others | Whether a provider appears in the rotation |
| `provider.mode` | `CliFirst` | Try CLI first, fall back to API |
| `provider.model` | `sonnet` (claude), blank/account default (Codex CLI), `gpt-5` (OpenAI API), `grok-4.5` (grok), `gemini-2.5-pro` (gemini), `llama3.3` (ollama) | Model identifier |
| `notifications.enabled` | `false` | Master opt-in; native permission alone does not enable alerts |
| `notifications.low_usage` | `true` | Alert only when an exact remaining percentage is available |
| `notifications.low_usage_threshold_percent` | `20` | Remaining percentage considered low |
| `notifications.provider_exhausted` | `true` | Alert on quota or billing exhaustion |
| `notifications.provider_fallback` | `true` | Alert when another provider wins the render |
| `notifications.local_model_downloads` | `true` | Alert when a model download completes or fails |
| `notifications.standup_complete` | `false` | Avoid a routine daily success alert |
| `notifications.standup_failed` | `true` | Alert when a compile fails |
| `provider.cli_path` | `None` (auto-detect) | Override binary path |
| `provider.api_key_ref` | `None` | Keychain reference name (not the key) |
| `provider.api_base_url` | `None` | Custom API base (e.g. self-hosted Ollama) |
| `provider.timeout_secs` | `180` | Per-request timeout |
| `data_sources.local_git` | `true` (always, cannot be disabled) | Authoritative source |
| `data_sources.github` | `true` | GitHub PR/review enrichment |
| `data_sources.claude_code` | `true` | Claude Code conversation digest |
| `data_sources.remember` | `true` | Remember notes |
| `data_sources.opencode` | `false` | opencode sessions |
| `data_sources.codex` | `false` | Codex sessions |
| `data_sources.gemini_cli` | `false` | Gemini CLI sessions |
| `data_sources.grok_cli` | `false` | Grok CLI sessions |
| `scheduler.enabled` | `true` | Whether the system scheduler is installed |
| `scheduler.cron` | `0 7-19 * * 1-5` | Hourly 07:00–19:00 weekdays |
| `scheduler.self_heal` | `true` | Recompile F_PREV if AUTO empty |
| `review.reviewer` | `""` (must be set) | GitHub login for "me" |
| `review.pr_org` | `""` (must be set) | GitHub org to search |
| `review.max_prs` | `10` | Max PRs to fetch per run |
| `review.comment_len` | `220` | Max comment preview length |
| `review.include_self_reviews` | `false` | Include PRs where I reviewed my own code |
| `scrub.alias_scrub` | `false` | Re-attach alias tags instead of dropping |
| `scrub.alias_scrub_min` | `2` | Min token overlap for alias match |
| `scrub.meta_extra` | `None` | Pipe-separated regex to extend meta-work filter |
| `dates.archive_mode` | `next_business_day` | Which file a day's work is archived under. Edited in Settings → Paths → Filing date |

---

## Filing date

`dates.archive_mode` decides the **file name** a compile writes, which is a
different question from `dailies_dir` (the directory) and from
`scheduler.cron` (when the run fires).

| Value | Consequence | Weekend |
| --- | --- | --- |
| `next_business_day` (default) | Today's work is filed for tomorrow's standup — Thursday's work appears in Friday's file. Reproduces the App Script (`compile.sh:534`). | Friday, Saturday and Sunday all accumulate into Monday's file |
| `same_day` | Today's work is filed for today's standup — Thursday's work stays in Thursday's file. | Saturday and Sunday accumulate into Monday's file |

Neither value can produce a standup named after a weekend day, so weekend work
always lands on Monday. Both preserve the window contract in
`docs/specs/pipeline.md` § (a): a file's range starts the day after the previous
file's range ended, so no day is dropped or reported twice.

A `config.json` written before this block existed has no `dates` key and loads
as `next_business_day` — the only policy those installs could have been running.

**Where the UI puts it:** Settings → **Paths**, above the directory fields.
Paths is the tab that answers "where does my standup end up", and the directory
and the file name are the two halves of that answer. It is deliberately *not* on
Standup Format, whose banner says presets "only affect the LLM render path" —
the filing date applies to the deterministic renderer too.

**Where to check what a run actually used:** the Terminal panel's step-(a) line
(`filing <date>.md — window <start> → <end>`, detail `archive_mode=…`) and the
`archive_mode` field of the audit sidecar.

---

## Provider failover configuration

`llm.provider_order` is the authoritative ordered list when it is non-empty. Blank and duplicate ids are removed while preserving the first occurrence. Providers with `enabled = false` are recorded as skipped and are not invoked. Turning off `fallback_enabled` truncates the resolved chain to its first provider.

For existing config files without the new fields, serde defaults enable fallback and derive a legacy-compatible order from `preferred_provider` followed by `providers` in storage order. Setting `AUTOSTAND_LLM_PROVIDER` is an explicit single-provider override: it does not fan out to the saved rotation.

The Settings Providers tab exposes the master fallback switch, provider enable switches, preferred-provider selection, and up/down priority controls. Making a provider preferred also moves it to the beginning of `provider_order`.

## Config UI

The Settings page (`routes/settings.tsx`) exposes these tabs (see `docs/tauri/04-frontend-stack.md`):

- Providers — connection settings, ordered failover, and provider usage.
- Data Sources — enablement for the eight read-only activity sources.
- Standup Format — preset and output options.
- Paths — the filing-date policy (`dates.archive_mode`) first, then the GitHub and dailies directories, commit authors, and the discovered-repo table.
- Sync — Cloud Sync creates `<provider-root>/autostand`; optional Repo Sync versions that same directory in a private GitHub repository when `git`, `gh`, and GitHub authentication are available. A shared requirements checklist reports each of those three and offers the one next step that satisfies it: a command printed in full before it may run, or the official install guide when no package id can be vouched for on this platform.
- Scheduler — a human schedule builder (time, days, once/hourly) with cron kept under Advanced, plus self-heal controls.
- Notifications — OS permission, master opt-in, thresholds, and alert categories.
- Local AI — curated model downloads, selection, and on-demand/reusable-cache runtime policy. Selecting a model also enables and prefers `builtin-local` in Providers.

### Native notifications

Notification delivery has two transports with one shared preference and deduplication policy:

- When the Tauri runtime is active, Autostand uses `tauri-plugin-notification`; OS permission is requested only after the user presses **Allow notifications**.
- Scheduled `--compile` processes use the platform-native service directly: `osascript` on macOS, `notify-send` on Linux, and Windows toast APIs through a fixed PowerShell script. Dynamic Windows title/body values are base64 environment data rather than executable script text.

The master switch defaults off. Permission by itself never enables delivery. Category defaults are low usage, exhaustion, failover, local-model completion/failure, and standup failure on; routine standup success off. Notifications are deduplicated by transition key for six hours and history older than seven days is pruned from `state/notification-history.json`.

Titles and bodies are single-line, bounded, content-free summaries. They must never contain standup text, prompts, model responses, raw provider failures, paths, API keys, or CLI credentials. Notification delivery errors are warnings and never change a compile result.

### Live validation

- `validate_paths()` runs after every path edit. Each path shows a green/red badge with the missing-path message.
- `detect_cli(provider)` runs after a `cli_path` change. Shows the resolved path + `--version` output.
- `get_api_key_status(provider)` runs after the API key dialog closes.
- `get_provider_health()` loads current usage state; `refresh_provider_health(provider?)` performs an explicit refresh.
- The "Test provider" button calls `test_llm_provider` and displays `{ ok, message, latency_ms }` inline.

### Save semantics

`set_config` is called on every field change (debounced 500ms). There is no separate "Save" button — the UI is always in sync with the JSON file. A "Revert" button restores the last loaded config from disk.

---

## Keychain API keys

API keys live in the OS keychain, **never** in the config JSON or env vars.

| Platform | Backend | Entry name |
| --- | --- | --- |
| macOS | Keychain (via `keyring` crate) | `autostand.<provider>` |
| Linux | Secret Service (`gnome-keyring` / `kwallet`) | `autostand.<provider>` |
| Windows | Credential Manager | `autostand.<provider>` |

### Storage

```rust
// crates/autostand-app/src/secrets.rs (excerpt)
use keyring::Entry;

pub fn store_api_key(provider: &str, key: &str) -> Result<()> {
    Entry::new("autostand", provider)?.set_password(key)?;
    Ok(())
}

pub fn get_api_key(provider: &str) -> Result<Option<String>> {
    match Entry::new("autostand", provider)?.get_password() {
        Ok(k) => Ok(Some(k)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

pub fn get_api_key_status(provider: &str) -> ApiKeyStatus {
    if get_api_key(provider).ok().flatten().is_some() {
        ApiKeyStatus { set: true, mode: ApiKeyMode::Keychain }
    } else if std::env::var(format!("{}_API_KEY", provider.to_uppercase())).is_ok() {
        ApiKeyStatus { set: true, mode: ApiKeyMode::Env }
    } else {
        ApiKeyStatus { set: false, mode: ApiKeyMode::None }
    }
}
```

### Config JSON never contains raw keys

`ProviderConfig.api_key_ref` is the **keychain entry name** (e.g. `Some("claude")`), not the key itself. The config JSON is safe to share, commit, or print. When the LLM adapter needs the key, it calls `get_api_key(provider)` at call time.

### Env fallback

For headless/CLI mode where the keychain may be unavailable (e.g. systemd unit without D-Bus), the adapter falls back to `<PROVIDER>_API_KEY` env vars (`CLAUDE_API_KEY`, `OPENAI_API_KEY`, `GEMINI_API_KEY`, `GROK_API_KEY`). `get_api_key_status` reports `mode: "env"` in this case. The preferred path is always the keychain.
