//! LLM render orchestration + output validation.
//!
//! See steps (m) `render_llm` and (n) `validate_render` of `docs/specs/pipeline.md`.
//!
//! This module is the async half of the render: `autostand_core::pipeline::compile_file`
//! is pure and sync, so it takes an *already produced* LLM body. Everything needed to
//! produce that body — prompt assembly, provider selection, API-key resolution, mode
//! orchestration and output validation — lives here.
//!
//! Three invariants shape the code:
//!
//! 1. **Fallback is explicit.** [`render_llm`] preserves its `Option` compatibility API,
//!    while [`render_llm_outcome_logged`] retains every safe failure so `Auto` can fall back
//!    deterministically and strict `Llm` mode can report an actionable error.
//! 2. **Anti-recursion.** A rendering CLI session must not be picked up as a data source on
//!    the next run, so every CLI subprocess carries `AUTOSTAND_RENDER=1` (set by
//!    `autostand_adapters::llm::helpers::run_cli`), and [`render_llm`] refuses to render at
//!    all when it finds that variable already set in its own environment.
//! 3. **Keys never leak.** API keys come from the OS keychain (`autostand.<provider>`) or the
//!    provider's env var, are never written to config, and adapter errors are logged as a
//!    short kind label instead of their `Display` string (a provider that puts the key in a
//!    query string could otherwise echo it back inside a transport error).

use std::future::Future;
use std::path::PathBuf;
use std::time::Duration;

use autostand_adapters::llm::helpers;
use autostand_adapters::llm::traits::{
    LlmAdapter, LlmError, ProviderConfig as AdapterConfig, ProviderMode as AdapterMode,
    RenderModeUsed, RenderOutput,
};
use autostand_core::provenance::extract_tickets;

use crate::commands::types::{
    AppConfig, ProviderConfig, ProviderFallbackPolicy, ProviderMode, RenderMode,
    StandupFormatConfig,
};

// ── Canonical prompt ──────────────────────────────────────────────────────

/// The canonical render prompt, verbatim from `docs/llm-adapters/render-prompt.md`.
///
/// It is provider-agnostic: no "you are Claude/GPT" framing, no provider-specific
/// instructions. Each adapter decides where the text goes (Anthropic `system`, `OpenAI`
/// `messages[0]`, Gemini `systemInstruction`, or prepended to the CLI prompt).
///
/// The `{JIRA_BASE}` token is substituted by [`system_prompt_for`]; when no Jira base is
/// configured the literal token is left in place (the spec surfaces a config warning rather
/// than aborting the render).
const RENDER_PROMPT: &str = r#"You are a daily standup compiler. Given structured activity data, produce a clean Markdown standup.

## Source hierarchy (most authoritative first)
1. GIT FACTS — committed work. Authoritative for what was committed and when.
2. GITHUB — PRs opened/merged, reviews given. Authoritative for PR activity.
3. EDITED FILES (Claude Code / OpenCode / Codex / Gemini CLI / Grok CLI) — non-commit file work attributed to repos. Use repo basenames.
4. NOTES (.remember) — narrative non-commit work. LAST RESORT after scrubbing. Never claim committed work.

## Rules
- Past tense, concrete, English.
- The OUTPUT block below is the sole authority for headings and section order. Do not use a legacy repo-section layout when a preset is present.
- Place repo names, Jira keys, titles and PR-review facts inside the required preset bullets; the Jira key is the only link.
- When a required forward-looking or evaluative section has no supported fact, write exactly `- None`; never ask a question, explain a conflict, or refuse the format.
- NEVER claim work was committed/pushed/merged if it's only in notes.
- NEVER include secrets, API keys, tokens, passwords.
- NEVER attribute to AI. Write as if the human did the work.
- NEVER say "no work done" if FACTS or NOTES have content.
- Accumulate: if a previous render had bullets not covered by the new data, they will be re-injected — do not duplicate them.
- Jira base URL: {JIRA_BASE}"#;

/// Return the canonical render prompt with `{JIRA_BASE}` still unresolved.
///
/// Callers that have a Jira base URL should use [`system_prompt_for`] instead; this accessor
/// exists so the raw text can be shown in the Debug UI and asserted against in tests.
pub fn system_prompt() -> &'static str {
    RENDER_PROMPT
}

/// Return the render prompt with `{JIRA_BASE}` substituted.
///
/// An empty `jira_base` leaves the literal token in place, which is what the spec asks for:
/// an unconfigured Jira is a config warning, not a render failure.
pub fn system_prompt_for(jira_base: &str) -> String {
    let base = jira_base.trim();
    if base.is_empty() {
        return RENDER_PROMPT.to_string();
    }
    RENDER_PROMPT.replace("{JIRA_BASE}", base)
}

// ── Prompt construction ───────────────────────────────────────────────────

/// Everything the user-role prompt is built from.
///
/// All fields borrow: the caller owns the gathered, scrubbed and redacted strings. Fields
/// left empty (`""` / `None`) are omitted from the prompt entirely, so a run with no GitHub
/// enrichment produces a prompt with no `## GITHUB` heading at all — an empty section reads
/// to the model as "there was no activity", which is a different claim from "not gathered".
#[derive(Debug, Clone, Copy, Default)]
pub struct PromptInputs<'a> {
    /// GIT FACTS block from `local-git` — the authoritative record of committed work.
    pub facts: &'a str,
    /// GitHub enrichment (PRs opened/merged), already redacted.
    pub github: Option<&'a str>,
    /// Narrative notes, already scrubbed (anti-backdating) and redacted.
    pub notes: &'a str,
    /// AI-session digest (Claude Code / `OpenCode` / Codex / Gemini / Grok edited files).
    pub conv: Option<&'a str>,
    /// PR-review enrichment feeding the trailing `**PR Review**` section.
    pub prrev: Option<&'a str>,
    /// Jira base URL, e.g. `https://org.atlassian.net/browse`.
    pub jira_base: &'a str,
    /// Filing date `F` (`YYYY-MM-DD`) — the day the standup is filed for.
    pub file_date: &'a str,
    /// Inclusive start of the work window (`YYYY-MM-DD`).
    pub range_start: &'a str,
    /// Inclusive end of the work window (`YYYY-MM-DD`).
    pub range_end: &'a str,
    /// Standup title, e.g. `Daily Standup — August 03, 2026`.
    pub title: &'a str,
    /// Standup subtitle, e.g. `_Work completed August 01–02, 2026._`.
    pub subtitle: &'a str,
    /// This host's previous AUTO body, if any.
    ///
    /// Shown to the model so it does not restate bullets `accumulate` will re-inject anyway.
    pub prev_auto: Option<&'a str>,
    /// Standup format configuration (presets mold the `## OUTPUT` section).
    ///
    /// When `None`, the default fixed output block is used (Det mode or legacy).
    pub format: Option<&'a StandupFormatConfig>,
}

/// Heading used for the previous-render section.
const PREV_RENDER_HEADING: &str = "## PREVIOUS RENDER (already reported — do not duplicate)";

/// Build the user-role prompt from the gathered inputs.
///
/// Deterministic by construction: no timestamps, no iteration over hash maps, no randomness —
/// the same inputs always yield byte-identical output, which is what makes the pipeline's
/// dirty-check hash meaningful.
///
/// Sections appear in the source-hierarchy order the system prompt declares (GIT FACTS →
/// GITHUB → PR REVIEWS → EDITED FILES → NOTES), and empty ones are skipped.
pub fn build_prompt(inputs: &PromptInputs<'_>) -> String {
    let mut out = String::with_capacity(1024);
    out.push_str("# Standup render request\n\n");

    push_context_line(&mut out, "Filing date", inputs.file_date);
    if !inputs.range_start.trim().is_empty() && !inputs.range_end.trim().is_empty() {
        out.push_str("Work window: ");
        out.push_str(inputs.range_start.trim());
        out.push_str(" .. ");
        out.push_str(inputs.range_end.trim());
        out.push('\n');
    }
    push_context_line(&mut out, "Title", inputs.title);
    push_context_line(&mut out, "Subtitle", inputs.subtitle);
    push_context_line(&mut out, "Jira base", inputs.jira_base);

    push_section(&mut out, "## GIT FACTS", Some(inputs.facts));
    push_section(&mut out, "## GITHUB", inputs.github);
    push_section(&mut out, "## PR REVIEWS", inputs.prrev);
    push_section(&mut out, "## EDITED FILES", inputs.conv);
    push_section(&mut out, "## NOTES", Some(inputs.notes));
    push_section(&mut out, PREV_RENDER_HEADING, inputs.prev_auto);

    if let Some(format) = inputs.format {
        out.push_str(&crate::format_presets::output_section(format));
    } else {
        out.push_str("\n## OUTPUT\n");
        out.push_str(
            "Return only the standup Markdown body: section headers and `- ` bullets. \
             No preamble, no closing commentary, no code fences.\n",
        );
    }
    out
}

/// Append `Label: value` when `value` is non-blank.
fn push_context_line(out: &mut String, label: &str, value: &str) {
    let value = value.trim();
    if value.is_empty() {
        return;
    }
    out.push_str(label);
    out.push_str(": ");
    out.push_str(value);
    out.push('\n');
}

/// Append a `## HEADING` + body block when the body is non-blank.
fn push_section(out: &mut String, heading: &str, body: Option<&str>) {
    let Some(body) = body.map(str::trim).filter(|b| !b.is_empty()) else {
        return;
    };
    out.push('\n');
    out.push_str(heading);
    out.push('\n');
    out.push_str(body);
    out.push('\n');
}

// ── Provider selection ────────────────────────────────────────────────────

/// Env var that overrides `config.llm.preferred_provider`.
const PROVIDER_ENV: &str = "AUTOSTAND_LLM_PROVIDER";

/// Env var that overrides the preferred provider's access mode.
const MODE_ENV: &str = "AUTOSTAND_LLM_MODE";

/// Anti-recursion guard env var (set on every spawned render CLI).
const RENDER_ENV: &str = "AUTOSTAND_RENDER";

/// Default per-render timeout in seconds (`docs/llm-adapters/00-providers-overview.md`).
const DEFAULT_TIMEOUT_SECS: u64 = 180;

/// Ollama's default timeout: local first-inference has to load the model into VRAM.
const OLLAMA_TIMEOUT_SECS: u64 = 300;

/// Instantiate the adapter for a provider id, or `None` if the id is unknown.
///
/// Resolution goes through `autostand_adapters::llm::registry()` rather than naming the
/// concrete structs, so a provider added to the registry is reachable here for free.
pub fn adapter_for(provider_id: &str) -> Option<Box<dyn LlmAdapter>> {
    autostand_adapters::llm::registry()
        .into_iter()
        .find(|adapter| adapter.id() == provider_id)
}

/// Env vars consulted (after the keychain) for a provider's API key.
fn env_vars_for(provider_id: &str) -> &'static [&'static str] {
    match provider_id {
        "claude" => &["ANTHROPIC_API_KEY"],
        "openai" => &["OPENAI_API_KEY"],
        "gemini" => &["GEMINI_API_KEY", "GOOGLE_API_KEY"],
        "grok" => &["XAI_API_KEY", "GROK_API_KEY"],
        // Ollama is local-only; a key is only meaningful for a remote gateway.
        _ => &[],
    }
}

/// Default model for a provider when config leaves `model` blank.
fn default_model_for(provider_id: &str) -> &'static str {
    match provider_id {
        "claude" => "sonnet",
        "ollama" => "llama3.2",
        "gemini" => "gemini-2.5-flash",
        "grok" => "grok-4.5",
        // Codex accounts do not all expose the same model ids. Leaving OpenAI
        // blank lets the CLI use the compatible model selected by the user's
        // Codex configuration. Built-in local uses the same blank value to
        // resolve the model-manager selection; unknown providers stay blank.
        // The HTTP adapter owns its separate API default.
        _ => "",
    }
}

/// Default timeout for a provider when config leaves `timeout_secs` at 0.
fn default_timeout_for(provider_id: &str) -> u64 {
    if matches!(provider_id, "ollama" | "builtin-local") {
        OLLAMA_TIMEOUT_SECS
    } else {
        DEFAULT_TIMEOUT_SECS
    }
}

/// Map the IPC-facing provider mode onto the adapter's.
fn adapter_mode(mode: ProviderMode) -> AdapterMode {
    match mode {
        ProviderMode::CliFirst => AdapterMode::CliFirst,
        ProviderMode::ApiFallback => AdapterMode::ApiFallback,
        ProviderMode::CliOnly => AdapterMode::CliOnly,
        ProviderMode::ApiOnly => AdapterMode::ApiOnly,
    }
}

/// Parse the `AUTOSTAND_LLM_MODE` override (case-insensitive).
fn parse_mode(raw: &str) -> Option<AdapterMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "clifirst" => Some(AdapterMode::CliFirst),
        "apifallback" => Some(AdapterMode::ApiFallback),
        "clionly" => Some(AdapterMode::CliOnly),
        "apionly" => Some(AdapterMode::ApiOnly),
        _ => None,
    }
}

/// Translate an IPC [`ProviderConfig`] into the adapter-facing config.
///
/// `api_key` is passed in rather than read here so the caller can decide *whether* to touch
/// the keychain at all: a `CliOnly` provider must never unlock it.
///
/// Blank `model` / zero `timeout_secs` fall back to the documented per-provider defaults —
/// the Settings UI stores `""` for "not chosen yet", and sending that straight to a CLI's
/// `--model` flag would fail every render.
pub fn provider_config(cfg: &ProviderConfig, api_key: Option<String>) -> AdapterConfig {
    let is_builtin_local = cfg.id == "builtin-local";
    let model = if cfg.model.trim().is_empty() {
        default_model_for(&cfg.id).to_string()
    } else {
        cfg.model.trim().to_string()
    };
    let timeout_secs = if cfg.timeout_secs == 0 {
        default_timeout_for(&cfg.id)
    } else {
        cfg.timeout_secs
    };
    AdapterConfig {
        mode: if is_builtin_local {
            AdapterMode::CliOnly
        } else {
            adapter_mode(cfg.mode)
        },
        model,
        cli_path: cfg
            .cli_path
            .as_ref()
            .map(|p| p.trim())
            .filter(|p| !p.is_empty())
            .map(PathBuf::from),
        api_key: if is_builtin_local { None } else { api_key },
        api_base_url: if is_builtin_local {
            None
        } else {
            cfg.api_base_url
                .as_ref()
                .map(|u| u.trim().to_string())
                .filter(|u| !u.is_empty())
        },
        timeout_secs,
    }
}

/// Resolve the provider failover chain, preserving legacy configurations.
///
/// An environment override is an explicit single-provider request and therefore
/// never silently fans out to configured alternatives. Without it, an explicit
/// `provider_order` wins; legacy configs start with `preferred_provider` and
/// then append configured providers in storage order. Duplicate/blank ids are
/// removed without reordering the remaining entries.
fn provider_chain(config: &AppConfig, env_override: Option<&str>) -> Vec<String> {
    if let Some(provider) = env_override.map(str::trim).filter(|id| !id.is_empty()) {
        return vec![provider.to_string()];
    }

    let mut candidates = if config.llm.provider_order.is_empty() {
        let mut legacy = Vec::with_capacity(config.llm.providers.len() + 1);
        legacy.push(config.llm.preferred_provider.clone());
        legacy.extend(
            config
                .llm
                .providers
                .iter()
                .map(|provider| provider.id.clone()),
        );
        legacy
    } else {
        config.llm.provider_order.clone()
    };
    candidates.retain(|id| !id.trim().is_empty());

    let mut ordered = Vec::with_capacity(candidates.len());
    for id in candidates {
        let id = id.trim().to_string();
        if !ordered.contains(&id) {
            ordered.push(id);
        }
    }
    if !config.llm.fallback_enabled {
        ordered.truncate(1);
    }
    ordered
}

/// Synthesize the provider entry for `provider_id`, defaulting when config has none.
///
/// A provider named by `AUTOSTAND_LLM_PROVIDER` typically has no entry in `llm.providers`;
/// defaulting (rather than bailing) is what makes the env override usable on its own.
fn provider_entry(config: &AppConfig, provider_id: &str) -> ProviderConfig {
    config
        .llm
        .providers
        .iter()
        .find(|p| p.id == provider_id)
        .cloned()
        .unwrap_or_else(|| ProviderConfig {
            id: provider_id.to_string(),
            enabled: true,
            mode: ProviderMode::CliFirst,
            model: String::new(),
            cli_path: None,
            api_key_ref: None,
            api_base_url: None,
            timeout_secs: 0,
        })
}

/// True when this process is itself a render subprocess.
///
/// Pure so it can be tested without mutating the process environment.
fn is_render_subprocess(env_value: Option<&str>) -> bool {
    env_value.is_some_and(|v| !v.trim().is_empty() && v.trim() != "0")
}

/// The ordered single-channel attempts for a configured mode.
///
/// Each returned mode is deliberately `CliOnly` or `ApiOnly`: handing the adapter a
/// single-channel mode is what *guarantees* the contract rather than trusting each of the
/// five adapters to interpret `CliFirst`/`ApiFallback` the same way.
///
/// - `CliOnly` → CLI, never the API.
/// - `ApiOnly` → API, never a subprocess.
/// - `CliFirst` → CLI, then the API if the CLI is missing **or fails**.
/// - `ApiFallback` → the API only when the CLI is unavailable.
fn attempt_plan(mode: AdapterMode, cli_available: bool) -> &'static [AdapterMode] {
    match mode {
        AdapterMode::CliOnly => &[AdapterMode::CliOnly],
        AdapterMode::ApiOnly => &[AdapterMode::ApiOnly],
        AdapterMode::CliFirst => &[AdapterMode::CliOnly, AdapterMode::ApiOnly],
        AdapterMode::ApiFallback => {
            if cli_available {
                &[AdapterMode::CliOnly]
            } else {
                &[AdapterMode::ApiOnly]
            }
        }
    }
}

/// Short, key-safe label for an adapter error.
///
/// WHY not `Display`: `LlmError::ApiError` carries a transport-error string, and a provider
/// that authenticates via a URL query parameter (Gemini) can surface that URL — key included —
/// inside it. Logging a fixed label makes leaking a secret into the run log impossible.
fn error_kind(err: &LlmError) -> &'static str {
    match err {
        LlmError::Timeout { .. } => "timeout",
        LlmError::CliNotFound { .. } => "cli_not_found",
        LlmError::CliExitError { stderr, .. } => safe_provider_error(stderr, "cli_exit_error"),
        LlmError::ApiError { status: 402, body } => safe_provider_error(body, "payment_required"),
        LlmError::ApiError {
            status: 401 | 403, ..
        }
        | LlmError::AuthError => "auth_error",
        LlmError::ApiError { body, .. } => safe_provider_error(body, "api_error"),
        LlmError::ParseError { .. } => "parse_error",
        LlmError::RateLimit { .. } => "rate_limit",
    }
}

/// Recognise a small allow-list of actionable provider failures.
///
/// Raw CLI/API bodies can contain prompts, URLs, or credentials, so they must
/// never enter the pipeline log. These stable labels reveal only the category
/// of several common failures observed in the supported CLIs.
fn safe_provider_error(message: &str, fallback: &'static str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("usage balance exhausted")
        || lower.contains("quota exhausted")
        || lower.contains("quota exceeded")
    {
        "usage_balance_exhausted"
    } else if lower.contains("payment required") {
        "payment_required"
    } else if lower.contains("not logged in") || lower.contains("please run /login") {
        "not_logged_in"
    } else if lower.contains("model is not supported")
        || (lower.contains("model") && lower.contains("not supported"))
        || lower.contains("unsupported_model")
    {
        "unsupported_model"
    } else if lower.contains("model_not_installed") || lower.contains("invalid_model") {
        "model_not_installed"
    } else if lower.contains("runtime_missing") {
        "runtime_missing"
    } else {
        fallback
    }
}

// ── Render orchestration ──────────────────────────────────────────────────

/// A validated-shape LLM body plus the provenance the audit sidecar records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedBody {
    /// The rendered AUTO block body (trimmed, code fences stripped).
    pub body: String,
    /// Provider id that produced it (`claude`, `ollama`, …).
    pub provider: String,
    /// Model id the provider reported using.
    pub model: String,
    /// True when the HTTP API produced it, false when a local CLI did.
    pub used_api: bool,
}

/// Transport used by one provider attempt.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAttemptChannel {
    Cli,
    Api,
}

/// Result of one secret-free provider/transport attempt.
#[derive(Debug, Clone, Copy, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProviderAttemptStatus {
    Succeeded,
    Failed,
    Empty,
    Skipped,
}

/// Structured render telemetry safe for logs, events and audit sidecars.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct ProviderAttempt {
    pub provider: String,
    pub channel: Option<ProviderAttemptChannel>,
    pub model: String,
    pub status: ProviderAttemptStatus,
    /// Stable classifier only; raw provider output must never be stored here.
    pub reason: Option<String>,
    pub latency_ms: Option<u64>,
}

/// Detailed result of a provider chain. The compatibility APIs project this to
/// `Option<RenderedBody>`, while the pipeline keeps the attempts for auditing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LlmRenderOutcome {
    pub rendered: Option<RenderedBody>,
    pub attempts: Vec<ProviderAttempt>,
}

impl LlmRenderOutcome {
    /// A stable, secret-free summary suitable for `AppError::Llm`.
    pub fn failure_summary(&self) -> String {
        let failures: Vec<String> = self
            .attempts
            .iter()
            .filter_map(|attempt| {
                attempt
                    .reason
                    .as_ref()
                    .map(|reason| format!("{}:{reason}", attempt.provider))
            })
            .collect();
        if failures.is_empty() {
            "no enabled LLM provider was available".to_string()
        } else {
            format!("all LLM providers failed ({})", failures.join(", "))
        }
    }
}

#[derive(Debug)]
struct BackendRenderOutcome {
    output: Option<RenderOutput>,
    attempts: Vec<ProviderAttempt>,
}

/// The slice of `LlmAdapter` the orchestrator actually needs.
///
/// WHY a second trait: `LlmAdapter` is an `#[async_trait]` trait, so writing a test double
/// for it would need the `async-trait` macro, which this crate does not depend on. This local
/// trait uses return-position `impl Future`, so the in-test fake is a plain struct and the
/// mode orchestration can be tested without spawning a process or touching the network.
//
// `manual_async_fn`: the `+ Send` bound is load-bearing — `render_llm` is awaited from a
// Tauri command on a multi-threaded runtime — and stable Rust cannot spell that bound on an
// `async fn` in a trait (it needs return-type notation).
#[allow(clippy::manual_async_fn)]
trait RenderBackend {
    /// Whether a local CLI binary is available for this provider.
    fn cli_available(&self) -> impl Future<Output = bool> + Send;

    /// Perform one single-channel render attempt.
    fn render(
        &self,
        prompt: &str,
        system: &str,
        config: &AdapterConfig,
    ) -> impl Future<Output = Result<RenderOutput, LlmError>> + Send;
}

/// [`RenderBackend`] over a real adapter.
struct AdapterBackend<'a>(&'a dyn LlmAdapter);

#[allow(clippy::manual_async_fn)] // see the note on `RenderBackend`
impl RenderBackend for AdapterBackend<'_> {
    fn cli_available(&self) -> impl Future<Output = bool> + Send {
        async move { self.0.detect_cli().await.is_some() }
    }

    fn render(
        &self,
        prompt: &str,
        system: &str,
        config: &AdapterConfig,
    ) -> impl Future<Output = Result<RenderOutput, LlmError>> + Send {
        async move { self.0.render(prompt, system, config).await }
    }
}

/// Run one provider's transport plan, returning its structured attempts.
async fn render_via_backend<B: RenderBackend>(
    backend: &B,
    provider: &str,
    prompt: &str,
    system: &str,
    config: &AdapterConfig,
    policy: ProviderFallbackPolicy,
    mut log: impl FnMut(&str),
) -> BackendRenderOutcome {
    // Only `ApiFallback` branches on CLI availability; probing for the other modes would
    // spawn a pointless `--version` subprocess on every render.
    let cli_available = if config.mode == AdapterMode::ApiFallback {
        backend.cli_available().await
    } else {
        false
    };

    let mut attempts = Vec::new();
    for &step in attempt_plan(config.mode, cli_available) {
        let channel = match step {
            AdapterMode::CliOnly | AdapterMode::CliFirst => "CLI",
            AdapterMode::ApiOnly | AdapterMode::ApiFallback => "API",
        };
        let channel_kind = match step {
            AdapterMode::CliOnly | AdapterMode::CliFirst => ProviderAttemptChannel::Cli,
            AdapterMode::ApiOnly | AdapterMode::ApiFallback => ProviderAttemptChannel::Api,
        };
        log(&format!(
            "trying {channel} — model {} (timeout {}s)",
            if config.model.is_empty() {
                "(default)"
            } else {
                config.model.as_str()
            },
            config.timeout_secs.max(1)
        ));
        let mut attempt = config.clone();
        attempt.mode = step;
        let mut retried_rate_limit = false;
        loop {
            match backend.render(prompt, system, &attempt).await {
                Ok(output) if output.body.trim().is_empty() => {
                    tracing::warn!(mode = ?step, "LLM returned an empty body");
                    log(&format!("{channel} returned an empty body"));
                    attempts.push(ProviderAttempt {
                        provider: provider.to_string(),
                        channel: Some(channel_kind),
                        model: config.model.clone(),
                        status: ProviderAttemptStatus::Empty,
                        reason: Some("empty_body".to_string()),
                        latency_ms: Some(output.latency_ms),
                    });
                    break;
                }
                Ok(output) => {
                    log(&format!(
                        "{channel} ok — {} ({} ms)",
                        output.model, output.latency_ms
                    ));
                    attempts.push(ProviderAttempt {
                        provider: provider.to_string(),
                        channel: Some(channel_kind),
                        model: output.model.clone(),
                        status: ProviderAttemptStatus::Succeeded,
                        reason: None,
                        latency_ms: Some(output.latency_ms),
                    });
                    return BackendRenderOutcome {
                        output: Some(output),
                        attempts,
                    };
                }
                Err(err) => {
                    let kind = error_kind(&err);
                    tracing::warn!(mode = ?step, kind, "LLM render attempt failed");
                    log(&format!("{channel} failed — {kind}"));
                    attempts.push(ProviderAttempt {
                        provider: provider.to_string(),
                        channel: Some(channel_kind),
                        model: config.model.clone(),
                        status: ProviderAttemptStatus::Failed,
                        reason: Some(kind.to_string()),
                        latency_ms: None,
                    });
                    let retry_after = match err {
                        LlmError::RateLimit { retry_after_secs } => retry_after_secs,
                        _ => None,
                    };
                    if !retried_rate_limit
                        && policy.retry_rate_limits
                        && retry_after.is_some_and(|secs| secs <= policy.max_retry_after_secs)
                    {
                        let delay = retry_after.unwrap_or_default();
                        retried_rate_limit = true;
                        log(&format!("{channel} retrying after {delay}s"));
                        tokio::time::sleep(Duration::from_secs(delay)).await;
                        continue;
                    }
                    break;
                }
            }
        }
    }
    BackendRenderOutcome {
        output: None,
        attempts,
    }
}

/// Strip a wrapping Markdown code fence, if the model added one.
///
/// Models regularly wrap the whole answer in a triple-backtick fence despite being told not
/// to; the fence markers would end up verbatim inside the standup file, so they are peeled
/// off here rather than failing validation over formatting.
fn strip_code_fence(body: &str) -> &str {
    let trimmed = body.trim();
    if !trimmed.starts_with("```") || !trimmed.ends_with("```") || trimmed.len() < 7 {
        return trimmed;
    }
    let Some((_, rest)) = trimmed.split_once('\n') else {
        return trimmed;
    };
    match rest.rfind("```") {
        Some(end) => rest[..end].trim(),
        None => trimmed,
    }
}

/// Produce the LLM standup body, or `None` when no provider in the chain succeeds.
///
/// Step (m) of `docs/specs/pipeline.md`. `None` is returned — always after a
/// `tracing::warn!` — when: the render mode is `Det`, no provider is configured, the provider
/// id is unknown, the provider is disabled, this process is itself a render subprocess, or
/// every attempt in the provider chain failed. The caller applies the selected
/// `Auto` versus strict `Llm` policy.
pub async fn render_llm(inputs: &PromptInputs<'_>, config: &AppConfig) -> Option<RenderedBody> {
    render_llm_logged(inputs, config, |_| {}).await
}

/// [`render_llm`] that reports each CLI/API attempt so the pipeline log can
/// show *why* a render is still sitting at 72 %.
#[tracing::instrument(skip_all, fields(provider, mode))]
pub async fn render_llm_logged(
    inputs: &PromptInputs<'_>,
    config: &AppConfig,
    log: impl FnMut(&str),
) -> Option<RenderedBody> {
    render_llm_outcome_logged(inputs, config, log)
        .await
        .rendered
}

/// Render through the configured provider chain and retain every safe attempt.
#[tracing::instrument(skip_all, fields(provider, mode))]
#[allow(clippy::too_many_lines)]
pub async fn render_llm_outcome_logged(
    inputs: &PromptInputs<'_>,
    config: &AppConfig,
    log: impl FnMut(&str),
) -> LlmRenderOutcome {
    render_llm_outcome_inner(inputs, config, None, log).await
}

/// Pipeline variant that rejects a provider's invalid body and continues the
/// failover chain. This keeps provenance validation inside the same sequence as
/// transport failures instead of accepting the first syntactically successful response.
pub async fn render_llm_outcome_validated_logged(
    inputs: &PromptInputs<'_>,
    config: &AppConfig,
    range_tickets: &[String],
    forbidden_tickets: &[String],
    log: impl FnMut(&str),
) -> LlmRenderOutcome {
    render_llm_outcome_inner(
        inputs,
        config,
        Some((range_tickets, forbidden_tickets)),
        log,
    )
    .await
}

#[allow(clippy::too_many_lines)]
async fn render_llm_outcome_inner(
    inputs: &PromptInputs<'_>,
    config: &AppConfig,
    validation: Option<(&[String], &[String])>,
    mut log: impl FnMut(&str),
) -> LlmRenderOutcome {
    if config.render_mode == RenderMode::Det {
        tracing::info!("render_mode=Det: skipping the LLM");
        log("render mode = Det — skipping LLM");
        return LlmRenderOutcome::default();
    }
    if is_render_subprocess(std::env::var(RENDER_ENV).ok().as_deref()) {
        tracing::warn!("{RENDER_ENV} is set: refusing to render from inside a render subprocess");
        log("refusing to render: AUTOSTAND_RENDER is set");
        return LlmRenderOutcome::default();
    }

    let env_provider = std::env::var(PROVIDER_ENV).ok();
    let providers = provider_chain(config, env_provider.as_deref());
    if providers.is_empty() {
        tracing::warn!("no LLM provider configured; using the deterministic renderer");
        log("no preferred provider configured");
        return LlmRenderOutcome::default();
    }

    let system = system_prompt_for(inputs.jira_base);
    let prompt = build_prompt(inputs);
    let mut attempts = Vec::new();

    for provider_id in providers {
        tracing::Span::current().record("provider", provider_id.as_str());
        let entry = provider_entry(config, &provider_id);
        if !entry.enabled {
            tracing::warn!(provider = %provider_id, "provider is disabled");
            log(&format!("provider {provider_id} skipped — disabled"));
            attempts.push(ProviderAttempt {
                provider: provider_id,
                channel: None,
                model: entry.model,
                status: ProviderAttemptStatus::Skipped,
                reason: Some("disabled".to_string()),
                latency_ms: None,
            });
            continue;
        }
        let Some(adapter) = adapter_for(&provider_id) else {
            tracing::warn!(provider = %provider_id, "unknown LLM provider id");
            log(&format!(
                "provider {provider_id} skipped — unknown provider"
            ));
            attempts.push(ProviderAttempt {
                provider: provider_id,
                channel: None,
                model: entry.model,
                status: ProviderAttemptStatus::Skipped,
                reason: Some("unknown_provider".to_string()),
                latency_ms: None,
            });
            continue;
        };

        let mode = if provider_id == "builtin-local" {
            AdapterMode::CliOnly
        } else {
            std::env::var(MODE_ENV)
                .ok()
                .and_then(|raw| parse_mode(&raw))
                .unwrap_or_else(|| adapter_mode(entry.mode))
        };
        tracing::Span::current().record("mode", tracing::field::debug(mode));
        let api_key = if mode == AdapterMode::CliOnly {
            None
        } else {
            helpers::load_api_key(&provider_id, env_vars_for(&provider_id))
        };
        let mut adapter_cfg = provider_config(&entry, api_key);
        adapter_cfg.mode = mode;
        log(&format!(
            "prompt ready — {} chars, provider {provider_id}, mode {mode:?}",
            prompt.len()
        ));

        let backend = AdapterBackend(adapter.as_ref());
        let provider_outcome = render_via_backend(
            &backend,
            &provider_id,
            &prompt,
            &system,
            &adapter_cfg,
            config.llm.fallback_policy,
            &mut log,
        )
        .await;
        attempts.extend(provider_outcome.attempts);
        if let Some(output) = provider_outcome.output {
            let body = strip_code_fence(&output.body).to_string();
            if let Some((range_tickets, forbidden_tickets)) = validation {
                if let Err(failure) = validate_render(&body, range_tickets, forbidden_tickets) {
                    let code = format!("validation_{}", failure.code());
                    tracing::warn!(provider = %provider_id, code, "LLM render failed validation");
                    log(&format!("provider {provider_id} rejected — {code}"));
                    if let Some(attempt) = attempts.last_mut() {
                        attempt.status = ProviderAttemptStatus::Failed;
                        attempt.reason = Some(code);
                    }
                    continue;
                }
            }
            tracing::info!(
                provider = %provider_id,
                model = %output.model,
                latency_ms = output.latency_ms,
                "LLM render succeeded"
            );
            return LlmRenderOutcome {
                rendered: Some(RenderedBody {
                    body,
                    provider: provider_id,
                    model: output.model,
                    used_api: output.mode_used == RenderModeUsed::Api,
                }),
                attempts,
            };
        }
        log(&format!("provider {provider_id} exhausted — trying next"));
    }

    LlmRenderOutcome {
        rendered: None,
        attempts,
    }
}

// ── Validation ────────────────────────────────────────────────────────────

/// Upper bound on a rendered body, in characters.
///
/// A two-business-day standup is a few kilobytes at most. A body past this size means the
/// model echoed its context (transcripts, file dumps) instead of writing a standup, so the
/// deterministic render is strictly better.
const MAX_RENDER_CHARS: usize = 20_000;

/// Phrases that assert nothing happened.
///
/// Kept literal (not the bare substring `no work`) so `no workaround was found` is not
/// mistaken for a hallucination.
const NO_WORK_PHRASES: &[&str] = &[
    "no work done",
    "no work was done",
    "nothing to report",
    "no activity to report",
];

/// Why an LLM body was rejected. Recorded in the audit sidecar by the caller.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ValidationFailure {
    /// The body was empty or whitespace-only.
    #[error("empty render body")]
    Empty,
    /// The body was implausibly long for a standup.
    #[error("render body too long ({chars} chars, max {max})")]
    TooLong {
        /// Length of the offending body, in characters.
        chars: usize,
        /// The limit it exceeded.
        max: usize,
    },
    /// The body had no `- ` bullet at all, so it is not a standup.
    #[error("render body has no bullet lines")]
    NoBullets,
    /// The body claimed nothing happened while the window had facts.
    #[error("render claims no work was done, but the window has facts")]
    NoWorkClaim,
    /// The body named a `FORBIDDEN` (cross-day) ticket.
    #[error("render names FORBIDDEN ticket {ticket}")]
    ForbiddenTicket {
        /// The offending ticket key.
        ticket: String,
    },
    /// The body named a ticket that is not in the allowed set.
    #[error("render invents ticket {ticket}, which is not in the window")]
    InventedTicket {
        /// The offending ticket key.
        ticket: String,
    },
}

impl ValidationFailure {
    /// Stable machine-readable label for the audit sidecar / run log.
    ///
    /// The `Display` string carries the offending ticket, which makes it useless as a
    /// grouping key; this one is fixed per variant.
    pub fn code(&self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::TooLong { .. } => "too_long",
            Self::NoBullets => "no_bullets",
            Self::NoWorkClaim => "no_work_claim",
            Self::ForbiddenTicket { .. } => "forbidden_ticket",
            Self::InventedTicket { .. } => "invented_ticket",
        }
    }
}

/// Validate an LLM body against the window's provenance — step (n) of the pipeline spec.
///
/// `range_tickets` is the allow-list of ticket keys the body may mention. The pipeline passes
/// the in-window GIT FACTS tickets; a caller that also wants GitHub or notes tickets accepted
/// passes the union. `forbidden_tickets` is checked first so a cross-day ticket is reported as
/// `ForbiddenTicket` rather than the less specific `InventedTicket`.
///
/// The no-work check only fires when `range_tickets` is non-empty: with no facts in the
/// window, "nothing to report" is the truth, not a hallucination.
pub fn validate_render(
    body: &str,
    range_tickets: &[String],
    forbidden_tickets: &[String],
) -> Result<(), ValidationFailure> {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return Err(ValidationFailure::Empty);
    }

    let chars = trimmed.chars().count();
    if chars > MAX_RENDER_CHARS {
        return Err(ValidationFailure::TooLong {
            chars,
            max: MAX_RENDER_CHARS,
        });
    }

    if !trimmed
        .lines()
        .any(|line| line.trim_start().starts_with("- "))
    {
        return Err(ValidationFailure::NoBullets);
    }

    if !range_tickets.is_empty() {
        let lower = trimmed.to_lowercase();
        if NO_WORK_PHRASES.iter().any(|p| lower.contains(p)) {
            return Err(ValidationFailure::NoWorkClaim);
        }
    }

    for ticket in extract_tickets(trimmed) {
        if forbidden_tickets.contains(&ticket) {
            return Err(ValidationFailure::ForbiddenTicket { ticket });
        }
        if !range_tickets.contains(&ticket) {
            return Err(ValidationFailure::InventedTicket { ticket });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        adapter_for, attempt_plan, build_prompt, error_kind, is_render_subprocess, parse_mode,
        provider_chain, provider_config, render_via_backend, strip_code_fence, system_prompt,
        system_prompt_for, validate_render, AdapterConfig, AdapterMode, LlmError, PromptInputs,
        ProviderAttemptStatus, ProviderConfig, ProviderMode, RenderBackend, RenderModeUsed,
        RenderOutput, MAX_RENDER_CHARS, PROVIDER_ENV,
    };
    use crate::commands::types::{
        AppConfig, LlmConfig, ProviderFallbackPolicy, StandupFormatConfig,
    };
    use std::future::Future;
    use std::sync::Mutex;

    /// Serializes the few tests that touch process-wide env vars.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn full_inputs() -> PromptInputs<'static> {
        PromptInputs {
            facts: "### repo: autostand / tickets: FIF-133 / commits (1):\n- (FIF-133) Implemented core model",
            github: Some("autostand #12 — \"Add pipeline\" — MERGED"),
            notes: "- Drafted the render spec",
            conv: Some("autostand: src/render.rs, src/gather.rs"),
            prrev: Some("autostand #9 — \"Fix lock\" (by someone) — APPROVED"),
            jira_base: "https://jira.example.com/browse",
            file_date: "2026-08-03",
            range_start: "2026-08-01",
            range_end: "2026-08-02",
            title: "Daily Standup — August 03, 2026",
            subtitle: "_Work completed August 01–02, 2026._",
            prev_auto: Some("- Refactored the queue processor"),
            format: None,
        }
    }

    // ── prompt ────────────────────────────────────────────────────────────

    #[test]
    fn build_prompt_is_deterministic() {
        let inputs = full_inputs();
        assert_eq!(build_prompt(&inputs), build_prompt(&inputs));
    }

    #[test]
    fn build_prompt_includes_every_non_empty_section() {
        let prompt = build_prompt(&full_inputs());
        for needle in [
            "Filing date: 2026-08-03",
            "Work window: 2026-08-01 .. 2026-08-02",
            "Title: Daily Standup",
            "Subtitle: _Work completed",
            "Jira base: https://jira.example.com/browse",
            "## GIT FACTS",
            "## GITHUB",
            "## PR REVIEWS",
            "## EDITED FILES",
            "## NOTES",
            "## PREVIOUS RENDER",
            "## OUTPUT",
        ] {
            assert!(
                prompt.contains(needle),
                "prompt is missing {needle}:\n{prompt}"
            );
        }
        assert!(prompt.contains("Implemented core model"));
        assert!(prompt.contains("Drafted the render spec"));
        assert!(prompt.contains("Refactored the queue processor"));
    }

    #[test]
    fn build_prompt_orders_sections_by_source_hierarchy() {
        let prompt = build_prompt(&full_inputs());
        let at = |needle: &str| prompt.find(needle).expect("section present");
        assert!(at("## GIT FACTS") < at("## GITHUB"));
        assert!(at("## GITHUB") < at("## PR REVIEWS"));
        assert!(at("## PR REVIEWS") < at("## EDITED FILES"));
        assert!(at("## EDITED FILES") < at("## NOTES"));
        assert!(at("## NOTES") < at("## OUTPUT"));
    }

    #[test]
    fn build_prompt_embeds_every_preset_marker() {
        for preset in crate::format_presets::all_presets() {
            let format = StandupFormatConfig {
                preset,
                ..StandupFormatConfig::default()
            };
            let prompt = build_prompt(&PromptInputs {
                format: Some(&format),
                ..full_inputs()
            });
            for marker in crate::format_presets::preset_section_markers(preset) {
                assert!(
                    prompt.contains(marker),
                    "{preset:?} prompt is missing {marker}"
                );
            }
        }
    }

    #[test]
    fn build_prompt_omits_empty_and_blank_sections() {
        let inputs = PromptInputs {
            facts: "### repo: autostand",
            github: None,
            notes: "",
            conv: Some("   \n  "),
            prrev: None,
            jira_base: "",
            file_date: "2026-08-03",
            ..PromptInputs::default()
        };
        let prompt = build_prompt(&inputs);
        assert!(prompt.contains("## GIT FACTS"));
        for absent in [
            "## GITHUB",
            "## NOTES",
            "## EDITED FILES",
            "## PR REVIEWS",
            "## PREVIOUS RENDER",
            "Jira base:",
            "Work window:",
            "Title:",
        ] {
            assert!(
                !prompt.contains(absent),
                "prompt should omit {absent}:\n{prompt}"
            );
        }
    }

    #[test]
    fn system_prompt_is_the_canonical_text() {
        let prompt = system_prompt();
        assert!(prompt.starts_with("You are a daily standup compiler."));
        for rule in [
            "## Source hierarchy (most authoritative first)",
            "1. GIT FACTS",
            "4. NOTES (.remember)",
            "- Past tense, concrete, English.",
            "NEVER claim work was committed/pushed/merged if it's only in notes.",
            "NEVER include secrets, API keys, tokens, passwords.",
            "NEVER attribute to AI.",
            "NEVER say \"no work done\" if FACTS or NOTES have content.",
            "{JIRA_BASE}",
        ] {
            assert!(prompt.contains(rule), "canonical prompt is missing: {rule}");
        }
    }

    #[test]
    fn system_prompt_for_substitutes_jira_base() {
        let resolved = system_prompt_for("https://jira.example.com/browse");
        assert!(resolved.ends_with("Jira base URL: https://jira.example.com/browse"));
        assert!(!resolved.contains("{JIRA_BASE}"));
    }

    #[test]
    fn system_prompt_for_keeps_the_token_when_jira_is_unconfigured() {
        assert!(system_prompt_for("  ").contains("{JIRA_BASE}"));
    }

    #[test]
    fn strip_code_fence_unwraps_a_fenced_body() {
        assert_eq!(
            strip_code_fence("```markdown\n**repo**\n- did a thing\n```"),
            "**repo**\n- did a thing"
        );
        assert_eq!(
            strip_code_fence("  **repo**\n- did a thing  "),
            "**repo**\n- did a thing"
        );
        assert_eq!(strip_code_fence("```"), "```");
    }

    // ── validation ────────────────────────────────────────────────────────

    fn clean_body() -> String {
        "**autostand — [FIF-133](https://jira.example.com/browse/FIF-133) — Core model**\n\
         - Implemented the standup domain model\n"
            .to_string()
    }

    #[test]
    fn validate_accepts_a_clean_body() {
        let range = vec!["FIF-133".to_string()];
        assert!(validate_render(&clean_body(), &range, &[]).is_ok());
    }

    #[test]
    fn validate_rejects_an_empty_body() {
        let err = validate_render("   \n\t ", &[], &[]).expect_err("empty is rejected");
        assert_eq!(err.code(), "empty");
    }

    #[test]
    fn validate_rejects_an_oversized_body() {
        let body = format!("- {}\n", "x".repeat(MAX_RENDER_CHARS + 10));
        let err = validate_render(&body, &[], &[]).expect_err("oversized is rejected");
        assert_eq!(err.code(), "too_long");
    }

    #[test]
    fn validate_rejects_a_body_without_bullets() {
        let err = validate_render("**autostand — Core model**", &[], &[])
            .expect_err("prose without bullets is rejected");
        assert_eq!(err.code(), "no_bullets");
    }

    #[test]
    fn validate_rejects_an_invented_ticket() {
        let range = vec!["FIF-133".to_string()];
        let body = "**autostand**\n- Implemented FIF-999 end to end\n";
        let err = validate_render(body, &range, &[]).expect_err("invented ticket is rejected");
        assert_eq!(err.code(), "invented_ticket");
        assert!(err.to_string().contains("FIF-999"));
    }

    #[test]
    fn validate_rejects_a_forbidden_ticket_with_the_specific_reason() {
        // FIF-140 is also absent from `range_tickets`; FORBIDDEN must win the report.
        let range = vec!["FIF-133".to_string()];
        let forbidden = vec!["FIF-140".to_string()];
        let body = "**autostand**\n- Shipped FIF-140 yesterday\n";
        let err = validate_render(body, &range, &forbidden).expect_err("forbidden is rejected");
        assert_eq!(err.code(), "forbidden_ticket");
        assert!(err.to_string().contains("FIF-140"));
    }

    #[test]
    fn validate_rejects_the_no_work_hallucination_only_when_facts_exist() {
        let body = "**General**\n- No work done in this window\n";
        let range = vec!["FIF-133".to_string()];
        assert_eq!(
            validate_render(body, &range, &[])
                .expect_err("hallucination is rejected")
                .code(),
            "no_work_claim"
        );
        assert!(
            validate_render(body, &[], &[]).is_ok(),
            "no facts ⇒ truthful"
        );
    }

    #[test]
    fn validate_ignores_technical_identifiers_that_look_like_tickets() {
        // `extract_tickets` denies UTF/SHA-style prefixes; validation must not flag them.
        let body = "**autostand**\n- Normalized UTF-8 handling and SHA-256 digests\n";
        assert!(validate_render(body, &[], &[]).is_ok());
    }

    #[test]
    fn validate_accepts_a_ticket_named_only_in_the_jira_link() {
        let range = vec!["FIF-133".to_string()];
        let body = "**autostand — [FIF-133](https://j.example/browse/FIF-133) — x**\n- Did it\n";
        assert!(validate_render(body, &range, &[]).is_ok());
    }

    // ── provider selection ────────────────────────────────────────────────

    fn cfg_with(preferred: &str, providers: Vec<ProviderConfig>) -> AppConfig {
        AppConfig {
            llm: LlmConfig {
                preferred_provider: preferred.to_string(),
                providers,
                ..LlmConfig::default()
            },
            ..AppConfig::default()
        }
    }

    fn provider(id: &str, mode: ProviderMode) -> ProviderConfig {
        ProviderConfig {
            id: id.to_string(),
            enabled: true,
            mode,
            model: String::new(),
            cli_path: None,
            api_key_ref: None,
            api_base_url: None,
            timeout_secs: 0,
        }
    }

    #[test]
    fn adapter_for_resolves_all_providers_and_rejects_others() {
        for id in [
            "builtin-local",
            "claude",
            "ollama",
            "openai",
            "gemini",
            "grok",
        ] {
            let adapter = adapter_for(id).expect("provider is registered");
            assert_eq!(adapter.id(), id);
        }
        assert!(adapter_for("anthropic").is_none());
        assert!(adapter_for("").is_none());
    }

    #[test]
    fn provider_chain_migrates_legacy_order_and_deduplicates() {
        let config = cfg_with(
            "grok",
            vec![
                provider("claude", ProviderMode::CliOnly),
                provider("grok", ProviderMode::CliOnly),
                provider("openai", ProviderMode::CliOnly),
            ],
        );
        assert_eq!(provider_chain(&config, None), ["grok", "claude", "openai"]);
    }

    #[test]
    fn explicit_provider_order_and_fallback_switch_are_respected() {
        let mut config = cfg_with("grok", vec![]);
        config.llm.provider_order = vec![" openai ".into(), "claude".into(), "openai".into()];
        assert_eq!(provider_chain(&config, None), ["openai", "claude"]);
        config.llm.fallback_enabled = false;
        assert_eq!(provider_chain(&config, None), ["openai"]);
        assert_eq!(provider_chain(&config, Some("grok")), ["grok"]);
    }

    #[test]
    fn legacy_llm_json_enables_safe_fallback_defaults() {
        let config: LlmConfig = serde_json::from_value(serde_json::json!({
            "preferred_provider": "grok",
            "providers": []
        }))
        .expect("legacy LlmConfig");
        assert!(config.fallback_enabled);
        assert!(config.provider_order.is_empty());
        assert!(config.fallback_policy.retry_rate_limits);
        assert_eq!(config.fallback_policy.max_retry_after_secs, 30);
    }

    #[test]
    fn provider_env_var_is_read_from_the_process_environment() {
        let _guard = ENV_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        std::env::set_var(PROVIDER_ENV, "grok");
        let read = std::env::var(PROVIDER_ENV).ok();
        let resolved = provider_chain(&cfg_with("claude", vec![]), read.as_deref());
        std::env::remove_var(PROVIDER_ENV);
        assert_eq!(resolved, ["grok"]);
    }

    #[test]
    fn provider_config_fills_in_documented_defaults() {
        let cfg = provider_config(&provider("ollama", ProviderMode::CliOnly), None);
        assert_eq!(cfg.model, "llama3.2");
        assert_eq!(cfg.timeout_secs, 300);
        assert_eq!(cfg.mode, AdapterMode::CliOnly);
        assert!(cfg.cli_path.is_none());
        assert!(cfg.api_key.is_none());

        let claude = provider_config(&provider("claude", ProviderMode::ApiOnly), None);
        assert_eq!(claude.model, "sonnet");
        assert_eq!(claude.timeout_secs, 180);

        let codex = provider_config(&provider("openai", ProviderMode::CliFirst), None);
        assert_eq!(codex.model, "");
        assert_eq!(codex.timeout_secs, 180);

        let local = provider_config(&provider("builtin-local", ProviderMode::ApiOnly), None);
        assert_eq!(local.model, "");
        assert_eq!(local.mode, AdapterMode::CliOnly);
        assert_eq!(local.timeout_secs, 300);
        assert!(local.api_key.is_none());
    }

    #[test]
    fn provider_config_keeps_explicit_values_and_the_supplied_key() {
        let mut p = provider("claude", ProviderMode::CliFirst);
        p.model = " opus ".to_string();
        p.timeout_secs = 42;
        p.cli_path = Some("/opt/bin/claude".to_string());
        p.api_base_url = Some("  ".to_string());
        let cfg = provider_config(&p, Some("sk-not-a-real-key".to_string()));
        assert_eq!(cfg.model, "opus");
        assert_eq!(cfg.timeout_secs, 42);
        assert_eq!(
            cfg.cli_path.as_deref(),
            Some(std::path::Path::new("/opt/bin/claude"))
        );
        assert!(
            cfg.api_base_url.is_none(),
            "a blank base URL is not a base URL"
        );
        assert_eq!(cfg.api_key.as_deref(), Some("sk-not-a-real-key"));
    }

    #[test]
    fn parse_mode_accepts_the_documented_spellings() {
        assert_eq!(parse_mode("CliFirst"), Some(AdapterMode::CliFirst));
        assert_eq!(parse_mode(" apionly "), Some(AdapterMode::ApiOnly));
        assert_eq!(parse_mode("CLIONLY"), Some(AdapterMode::CliOnly));
        assert_eq!(parse_mode("ApiFallback"), Some(AdapterMode::ApiFallback));
        assert_eq!(parse_mode("nonsense"), None);
    }

    #[test]
    fn render_subprocess_guard_trips_only_on_a_meaningful_value() {
        assert!(is_render_subprocess(Some("1")));
        assert!(is_render_subprocess(Some("true")));
        assert!(!is_render_subprocess(Some("0")));
        assert!(!is_render_subprocess(Some(" ")));
        assert!(!is_render_subprocess(None));
    }

    #[test]
    fn error_kinds_are_fixed_labels() {
        assert_eq!(error_kind(&LlmError::Timeout { secs: 1 }), "timeout");
        assert_eq!(error_kind(&LlmError::AuthError), "auth_error");
        assert_eq!(
            error_kind(&LlmError::RateLimit {
                retry_after_secs: None
            }),
            "rate_limit"
        );
        assert_eq!(
            error_kind(&LlmError::CliExitError {
                code: 1,
                stderr: "402 Payment Required: Grok Build usage balance exhausted".into(),
            }),
            "usage_balance_exhausted"
        );
        assert_eq!(
            error_kind(&LlmError::CliExitError {
                code: 1,
                stderr: "Not logged in · Please run /login".into(),
            }),
            "not_logged_in"
        );
        assert_eq!(
            error_kind(&LlmError::CliExitError {
                code: -1,
                stderr: "model_not_installed".into(),
            }),
            "model_not_installed"
        );
        assert_eq!(
            error_kind(&LlmError::CliExitError {
                code: -1,
                stderr: "runtime_missing".into(),
            }),
            "runtime_missing"
        );
        assert_eq!(
            error_kind(&LlmError::CliExitError {
                code: 1,
                stderr: "secret-shaped unknown failure sk-test-must-not-leak".into(),
            }),
            "cli_exit_error"
        );
    }

    // ── mode orchestration (fake backend, no process, no network) ─────────

    #[test]
    fn attempt_plan_honours_every_mode() {
        assert_eq!(
            attempt_plan(AdapterMode::CliOnly, true),
            [AdapterMode::CliOnly]
        );
        assert_eq!(
            attempt_plan(AdapterMode::ApiOnly, true),
            [AdapterMode::ApiOnly]
        );
        assert_eq!(
            attempt_plan(AdapterMode::CliFirst, false),
            [AdapterMode::CliOnly, AdapterMode::ApiOnly]
        );
        assert_eq!(
            attempt_plan(AdapterMode::ApiFallback, true),
            [AdapterMode::CliOnly],
            "the CLI is available, so the API must not be used"
        );
        assert_eq!(
            attempt_plan(AdapterMode::ApiFallback, false),
            [AdapterMode::ApiOnly]
        );
    }

    /// Records every attempted mode and answers from a scripted outcome per channel.
    struct FakeBackend {
        cli_present: bool,
        cli_ok: bool,
        api_ok: bool,
        attempts: Mutex<Vec<AdapterMode>>,
    }

    impl FakeBackend {
        fn new(cli_present: bool, cli_ok: bool, api_ok: bool) -> Self {
            Self {
                cli_present,
                cli_ok,
                api_ok,
                attempts: Mutex::new(Vec::new()),
            }
        }

        fn attempts(&self) -> Vec<AdapterMode> {
            self.attempts
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone()
        }
    }

    #[allow(clippy::manual_async_fn)] // see the note on `RenderBackend`
    impl RenderBackend for FakeBackend {
        fn cli_available(&self) -> impl Future<Output = bool> + Send {
            async move { self.cli_present }
        }

        fn render(
            &self,
            _prompt: &str,
            _system: &str,
            config: &AdapterConfig,
        ) -> impl Future<Output = Result<RenderOutput, LlmError>> + Send {
            let mode = config.mode;
            let model = config.model.clone();
            async move {
                self.attempts
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .push(mode);
                let (ok, used) = if mode == AdapterMode::CliOnly {
                    (self.cli_ok, RenderModeUsed::Cli)
                } else {
                    (self.api_ok, RenderModeUsed::Api)
                };
                if ok {
                    Ok(RenderOutput {
                        body: "**autostand**\n- Did the thing".to_string(),
                        mode_used: used,
                        model,
                        latency_ms: 1,
                    })
                } else {
                    Err(LlmError::CliNotFound { searched: vec![] })
                }
            }
        }
    }

    fn adapter_cfg(mode: AdapterMode) -> AdapterConfig {
        AdapterConfig {
            mode,
            model: "test-model".to_string(),
            cli_path: None,
            api_key: None,
            api_base_url: None,
            timeout_secs: 5,
        }
    }

    async fn run(backend: &FakeBackend, mode: AdapterMode) -> Option<RenderOutput> {
        render_via_backend(
            backend,
            "test",
            "prompt",
            "system",
            &adapter_cfg(mode),
            ProviderFallbackPolicy::default(),
            |_| {},
        )
        .await
        .output
    }

    #[tokio::test]
    async fn cli_only_never_touches_the_api() {
        let backend = FakeBackend::new(false, false, true);
        assert!(run(&backend, AdapterMode::CliOnly).await.is_none());
        assert_eq!(backend.attempts(), [AdapterMode::CliOnly]);
    }

    #[tokio::test]
    async fn api_only_never_shells_out() {
        let backend = FakeBackend::new(true, true, true);
        let out = run(&backend, AdapterMode::ApiOnly)
            .await
            .expect("api render");
        assert_eq!(out.mode_used, RenderModeUsed::Api);
        assert_eq!(backend.attempts(), [AdapterMode::ApiOnly]);
    }

    #[tokio::test]
    async fn cli_first_falls_back_to_the_api_when_the_cli_fails() {
        let backend = FakeBackend::new(true, false, true);
        let out = run(&backend, AdapterMode::CliFirst)
            .await
            .expect("api fallback");
        assert_eq!(out.mode_used, RenderModeUsed::Api);
        assert_eq!(
            backend.attempts(),
            [AdapterMode::CliOnly, AdapterMode::ApiOnly]
        );
    }

    #[tokio::test]
    async fn cli_first_stops_at_the_cli_when_it_succeeds() {
        let backend = FakeBackend::new(true, true, true);
        let out = run(&backend, AdapterMode::CliFirst)
            .await
            .expect("cli render");
        assert_eq!(out.mode_used, RenderModeUsed::Cli);
        assert_eq!(backend.attempts(), [AdapterMode::CliOnly]);
    }

    #[tokio::test]
    async fn api_fallback_uses_the_api_only_when_the_cli_is_unavailable() {
        let with_cli = FakeBackend::new(true, true, true);
        let out = run(&with_cli, AdapterMode::ApiFallback)
            .await
            .expect("cli render");
        assert_eq!(out.mode_used, RenderModeUsed::Cli);
        assert_eq!(with_cli.attempts(), [AdapterMode::CliOnly]);

        let without_cli = FakeBackend::new(false, false, true);
        let out = run(&without_cli, AdapterMode::ApiFallback)
            .await
            .expect("api render");
        assert_eq!(out.mode_used, RenderModeUsed::Api);
        assert_eq!(without_cli.attempts(), [AdapterMode::ApiOnly]);
    }

    #[tokio::test]
    async fn every_channel_failing_yields_none_so_the_caller_falls_back() {
        let backend = FakeBackend::new(false, false, false);
        assert!(run(&backend, AdapterMode::CliFirst).await.is_none());
        assert_eq!(
            backend.attempts(),
            [AdapterMode::CliOnly, AdapterMode::ApiOnly]
        );
    }

    #[tokio::test]
    async fn exhausted_provider_plan_retains_safe_attempt_telemetry() {
        let backend = FakeBackend::new(false, false, false);
        let outcome = render_via_backend(
            &backend,
            "grok",
            "prompt",
            "system",
            &adapter_cfg(AdapterMode::CliFirst),
            ProviderFallbackPolicy::default(),
            |_| {},
        )
        .await;
        assert!(outcome.output.is_none());
        assert_eq!(outcome.attempts.len(), 2);
        assert!(outcome
            .attempts
            .iter()
            .all(|attempt| attempt.provider == "grok"
                && attempt.status == ProviderAttemptStatus::Failed
                && attempt.reason.as_deref() == Some("cli_not_found")));
    }
}
