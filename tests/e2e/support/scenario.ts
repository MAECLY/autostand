/**
 * Seed data for the mocked Tauri backend.
 *
 * Every fixture is typed against the app's own DTO module, so a change to the
 * frozen IPC contract (`docs/tauri/02-ipc-contracts.md`) breaks these factories
 * instead of silently producing payloads the UI cannot read. The import is
 * type-only and therefore erased before the file reaches Node.
 */

import type {
  AppConfig,
  AppError,
  AuditData,
  AuditSidecar,
  CloudFolder,
  CompileResult,
  DataSourceConfig,
  Dependency,
  GatherPreview,
  LlmProviderConfig,
  LocalModelInfo,
  PathValidation,
  PipelineStatus,
  ProviderHealth,
  RepoInfo,
  SchedulerStatus,
  SettingsPaths,
  StandupFileContent,
  TestProviderResult,
} from "../../../apps/autostand-app/src/lib/types";

/**
 * The browser clock every spec runs against. Combined with `timezoneId: "UTC"`
 * in the Playwright config this pins `todayIso()` to {@link TODAY}, so date
 * fixtures do not rot and relative timestamps ("about 5 hours ago") are stable.
 */
export const FIXED_NOW = "2026-08-03T12:00:00.000Z";

/** Work day the fixed clock resolves to — a Monday. */
export const TODAY = "2026-08-03";

/** How `formatIsoDate` renders {@link TODAY}. */
export const TODAY_LABEL = "Aug 3, 2026";

/**
 * The file {@link TODAY}'s work is filed in under the default policy.
 *
 * Monday's work goes to Tuesday's standup, so the dashboard is looking at
 * `2026-08-04.md` while the calendar says the 3rd. Keeping the two dates apart
 * in the fixtures is the point: a suite that used one value for both could not
 * tell the dashboard reading the right file from it reading the wrong one.
 */
export const FILING_DATE = "2026-08-04";

/** How `formatIsoDate` renders {@link FILING_DATE}. */
export const FILING_DATE_LABEL = "Aug 4, 2026";

/** Slug of the machine under test — its AUTO block is the highlighted one. */
export const HOST = "mbp-miguel";

/** A second machine writing into the same file, to prove multi-host rendering. */
export const OTHER_HOST = "linux-lab";

export const DAILIES_DIR = "/Users/tester/Sync/Github_Dailies";
export const STATE_DIR = "/Users/tester/.autostand";

export function sidecarPath(date: string, host: string): string {
  return `${STATE_DIR}/audit/${date}.${host}.json`;
}

// ── Backend state ─────────────────────────────────────────────────────────

/**
 * Everything the fake backend owns. Keys are replaced wholesale by
 * `app.patchState`, so each one has to be independently meaningful.
 */
export interface BackendState {
  config: AppConfig;
  hostSlug: string;
  dataSources: DataSourceConfig[];
  providers: LlmProviderConfig[];
  /** `test_llm_provider` answers, keyed by provider id. */
  providerTests: Record<string, TestProviderResult>;
  /** `list_provider_models` answers, keyed by provider id. A miss is `[]`. */
  providerModels: Record<string, string[]>;
  /** `get_provider_health` / `refresh_provider_health` answers, in strip order. */
  providerHealth: ProviderHealth[];
  /** `read_standup_file` answers, keyed by filing date. A miss is `not_found`. */
  standups: Record<string, StandupFileContent>;
  /** `list_audit_sidecars` answers, keyed by filing date. A miss is an empty list. */
  sidecars: Record<string, AuditSidecar[]>;
  /** `read_audit_sidecar` answers, keyed by sidecar path. */
  auditData: Record<string, AuditData>;
  pipelineStatus: PipelineStatus;
  schedulerStatus: SchedulerStatus;
  gatherPreview: GatherPreview;
  repos: RepoInfo[];
  settingsPaths: SettingsPaths;
  pathValidations: PathValidation[];
  compileResult: CompileResult;
  cloudFolders: CloudFolder[];
  /** `get_dependency_status` answers; the command filters them by group. */
  dependencies: Dependency[];
  /** `list_local_models` answers, in catalog order. */
  localModels: LocalModelInfo[];
}

export interface Scenario {
  state: BackendState;
  /**
   * Commands that hang instead of answering, until the spec calls
   * `app.settle`/`app.fail`. This is how a spec holds the UI in its
   * "request in flight" state long enough to assert on it.
   */
  defer: string[];
  /** Commands that reject with the given `AppError` instead of dispatching. */
  errors: Record<string, AppError>;
}

// ── Factories ─────────────────────────────────────────────────────────────

export function makeAppConfig(): AppConfig {
  return {
    github_dir: "/Users/tester/Github",
    dailies_dir: DAILIES_DIR,
    standup_authors: ["Tester"],
    git_refs: "--all",
    jira_base: "https://example.atlassian.net/browse",
    host_slug_override: null,
    render_mode: "Auto",
    llm: {
      preferred_provider: "claude",
      fallback_enabled: true,
      provider_order: ["claude", "ollama"],
      fallback_policy: { retry_rate_limits: true, max_retry_after_secs: 30 },
      local_runtime_policy: "on_demand",
      providers: [
        {
          id: "claude",
          enabled: true,
          mode: "CliFirst",
          model: "claude-sonnet-4",
          cli_path: "/usr/local/bin/claude",
          api_key_ref: null,
          api_base_url: null,
          timeout_secs: 180,
        },
        {
          id: "ollama",
          enabled: false,
          mode: "ApiOnly",
          model: "llama3.1",
          cli_path: null,
          api_key_ref: null,
          api_base_url: "http://localhost:11434",
          timeout_secs: 300,
        },
      ],
    },
    data_sources: {
      local_git: true,
      github: false,
      claude_code: true,
      remember: false,
      opencode: false,
      codex: false,
      gemini_cli: false,
      grok_cli: false,
    },
    scheduler: { enabled: true, cron: "0 9 * * 1-5", self_heal: true },
    review: {
      reviewer: "tester",
      pr_org: "example",
      max_prs: 20,
      comment_len: 280,
      include_self_reviews: false,
    },
    scrub: { alias_scrub: true, alias_scrub_min: 4, meta_extra: null },
    format: {
      preset: "classic-scrum",
      verbosity: "standard",
      include_pr_review: true,
      include_confidence: false,
      include_risks: false,
      conventional: false,
    },
    // `SyncTab` reads `sync.cloud_root` without a fallback, so the Sync tab
    // cannot render at all when the fixture omits it.
    sync: { cloud_root: null, repo_enabled: false },
    // `CompileButton` reads both of these without a fallback, so the dashboard
    // cannot render at all when the fixture omits either.
    notifications: {
      enabled: true,
      low_usage: true,
      low_usage_threshold_percent: 20,
      provider_exhausted: true,
      provider_fallback: true,
      local_model_downloads: true,
      standup_complete: true,
      standup_failed: true,
    },
    regeneration: { replace_immediately: false },
    dates: { archive_mode: "next_business_day" },
  };
}

export function makeDataSources(): DataSourceConfig[] {
  return [
    {
      id: "local-git",
      label: "Local git",
      enabled: true,
      description: "Commits under the configured GitHub directory. Authoritative.",
    },
    {
      id: "github",
      label: "GitHub",
      enabled: false,
      description: "Pull requests and reviews via the gh CLI.",
    },
    {
      id: "claude-code",
      label: "Claude Code",
      enabled: true,
      description: "Session transcripts written by Claude Code.",
    },
    {
      id: "remember-plugin",
      label: "Remember plugin",
      enabled: false,
      description: "Daily notes captured by the remember plugin.",
    },
    {
      id: "opencode",
      label: "opencode",
      enabled: false,
      description: "Session transcripts written by opencode.",
    },
    {
      id: "codex",
      label: "Codex",
      enabled: false,
      description: "Session transcripts written by Codex.",
    },
    {
      id: "gemini-cli",
      label: "Gemini CLI",
      enabled: false,
      description: "Session transcripts written by Gemini CLI.",
    },
    {
      id: "grok-cli",
      label: "Grok CLI",
      enabled: false,
      description: "Session transcripts written by Grok CLI.",
    },
  ];
}

export function makeProviders(): LlmProviderConfig[] {
  return [
    {
      id: "claude",
      label: "Claude",
      enabled: true,
      mode: "CliFirst",
      model: "claude-sonnet-4",
      cli: {
        found: true,
        path: "/usr/local/bin/claude",
        version: "claude 1.2.3",
      },
      api_key: { set: false, mode: "none" },
    },
    {
      id: "ollama",
      label: "Ollama",
      enabled: false,
      mode: "ApiOnly",
      model: "llama3.1",
      cli: { found: false, path: "", version: "" },
      api_key: { set: false, mode: "none" },
    },
  ];
}

/**
 * The standup filed on {@link TODAY} — that is, Friday's work.
 *
 * This is the *previous* file from the dashboard's point of view: History and
 * Audit browse it, the dashboard does not.
 */
export function makeStandupFile(
  overrides: Partial<StandupFileContent> = {},
): StandupFileContent {
  return {
    date: TODAY,
    title: "Daily Standup — August 03, 2026",
    subtitle: "_Work completed Fri Jul 31 – Sun Aug 02, 2026._",
    auto_blocks: [
      { host: HOST, body: "- FIF-136 wired the compile pipeline end to end" },
      { host: OTHER_HOST, body: "- FIF-141 drafted the discovery KB" },
    ],
    manual_region: "- Pairing session with the platform team",
    ...overrides,
  };
}

/**
 * The standup the dashboard is looking at: {@link FILING_DATE}, holding the work
 * done on {@link TODAY}.
 */
export function makeFilingDateStandupFile(
  overrides: Partial<StandupFileContent> = {},
): StandupFileContent {
  return makeStandupFile({
    date: FILING_DATE,
    title: "Daily Standup — August 04, 2026",
    subtitle: "_Work completed Monday, August 03, 2026._",
    ...overrides,
  });
}

/**
 * The result of the last compile.
 *
 * `date` is a **filing** date — that is what `CompileResult.date` means on the
 * wire — so it is {@link FILING_DATE}, not the calendar day. Getting this wrong
 * is not cosmetic: `apply_regeneration` echoes it back and the frontend
 * invalidates the standup query for exactly that date, so a fixture that
 * reported the calendar day would leave the dashboard showing a stale body.
 */
export function makeCompileResult(
  overrides: Partial<CompileResult> = {},
): CompileResult {
  return {
    date: FILING_DATE,
    host: HOST,
    status: "ok",
    render_used: "llm",
    fellback: false,
    audit_path: sidecarPath(FILING_DATE, HOST),
    file_path: `${DAILIES_DIR}/${FILING_DATE}.md`,
    accumulated_count: 0,
    message: "3 bullets across 2 repos",
    ...overrides,
  };
}

export function makeIdlePipelineStatus(): PipelineStatus {
  return {
    state: "idle",
    current_date: null,
    current_host: null,
    step: null,
    percent: 0,
    last_run_at: null,
    last_result: null,
    error: null,
  };
}

export function makeAuditData(overrides: Partial<AuditData> = {}): AuditData {
  return {
    file: `${DAILIES_DIR}/${TODAY}.md`,
    host: HOST,
    rendered_at: "2026-08-03T07:15:00Z",
    window: { range_start: "2026-08-01", range_end: "2026-08-02" },
    facts: [
      {
        repo: "autostand",
        ticket: "FIF-136",
        title: "wire the compile pipeline",
        commits: [
          {
            sha: "0f1e2d3a",
            subject: "feat(core): implement pipeline",
            date: "2026-08-02T18:04:00Z",
            files: ["crates/autostand-core/src/pipeline.rs"],
          },
        ],
      },
    ],
    notes: [
      {
        source: "/Users/tester/Sync/Github_Context/FIF-136.md",
        date: "2026-08-02",
        clauses: ["pipeline wired end to end"],
      },
    ],
    github: null,
    conv: null,
    prrev: null,
    claude_files: [],
    opencode_sessions: [],
    codex_sessions: [],
    gemini_sessions: [],
    grok_sessions: [],
    // A forbidden ticket is exactly what makes a code-change bullet a phantom.
    forbidden_tickets: ["FIF-133"],
    covered_tickets: ["FIF-136"],
    skew: [],
    ticket_days: { "FIF-136": ["2026-08-02"] },
    archive_mode: "next_business_day",
    render_mode: "auto",
    render_used: "llm",
    provider: "claude",
    model: "claude-sonnet-4",
    fellback: false,
    provider_attempts: [],
    hash: "sha256:deadbeef",
    accumulated_count: 0,
    ...overrides,
  };
}

export function makeSidecars(): AuditSidecar[] {
  return [
    {
      path: sidecarPath(TODAY, HOST),
      date: TODAY,
      host: HOST,
      rendered_at: "2026-08-03T07:15:00Z",
      render_used: "llm",
      provider: "claude",
      model: "claude-sonnet-4",
      fellback: false,
    },
    {
      path: sidecarPath(TODAY, OTHER_HOST),
      date: TODAY,
      host: OTHER_HOST,
      rendered_at: "2026-08-03T07:20:00Z",
      render_used: "det",
      provider: null,
      model: null,
      fellback: false,
    },
  ];
}

function makeSettingsPaths(): SettingsPaths {
  return {
    github_dir: "/Users/tester/Github",
    dailies_dir: DAILIES_DIR,
    claude_dir: "/Users/tester/.claude",
    codex_dir: "/Users/tester/.codex",
    gemini_dir: "/Users/tester/.gemini",
    opencode_dir: "/Users/tester/.opencode",
    state_dir: STATE_DIR,
    config_dir: "/Users/tester/.config/autostand",
    audit_dir: `${STATE_DIR}/audit`,
  };
}

export function makeCloudFolders(): CloudFolder[] {
  return [
    {
      id: "icloud-drive",
      label: "iCloud Drive",
      path: "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs",
      dailies_path:
        "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs/autostand",
      exists: true,
      provider: "iCloud",
    },
    {
      id: "onedrive",
      label: "OneDrive",
      path: "/Users/tester/OneDrive",
      dailies_path: "/Users/tester/OneDrive/autostand",
      exists: false,
      provider: "OneDrive",
    },
    {
      id: "syncthing",
      label: "Syncthing",
      path: "/Users/tester/Sync",
      dailies_path: "/Users/tester/Sync/autostand",
      exists: false,
      provider: "Syncthing",
    },
  ];
}

/**
 * A machine where Repo Sync is fully set up and Local AI is not: the checklist
 * has to render a satisfied group and an unmet one side by side, including the
 * three remediation kinds.
 */
export function makeDependencies(): Dependency[] {
  return [
    {
      id: "repo-sync.git",
      group: "repo_sync",
      label: "Git",
      description: "Commits and pushes the standup history from the sync folder.",
      state: "ok",
      detail: "/usr/bin/git",
      remediation: null,
    },
    {
      id: "repo-sync.gh",
      group: "repo_sync",
      label: "GitHub CLI",
      description: "Creates the private repository and verifies it stayed private.",
      state: "ok",
      detail: "/opt/homebrew/bin/gh",
      remediation: null,
    },
    {
      id: "repo-sync.gh-auth",
      group: "repo_sync",
      label: "GitHub sign-in",
      description: "An authenticated github.com account for the GitHub CLI.",
      state: "ok",
      detail: null,
      remediation: null,
    },
    {
      id: "local-ai.sidecar",
      group: "local_ai",
      label: "Local inference helper",
      description:
        "The autostand-local-llm process that keeps models out of the app process.",
      state: "ok",
      detail: "/Applications/Autostand.app/Contents/MacOS/autostand-local-llm",
      remediation: null,
    },
    {
      id: "local-ai.runtime",
      group: "local_ai",
      label: "llama.cpp runtime",
      description:
        "Runs GGUF models on this device (llama-completion, or the older llama-cli).",
      state: "missing",
      detail: null,
      remediation: {
        kind: "terminal_command",
        label: "Install with Homebrew",
        command: "brew install llama.cpp",
        url: "https://github.com/ggml-org/llama.cpp",
        runnable: true,
        note: "Already have a build? Point AUTOSTAND_LLAMA_CLI at its executable.",
      },
    },
    {
      id: "local-ai.model",
      group: "local_ai",
      label: "Downloaded model",
      description: "A verified GGUF model selected for on-device rendering.",
      state: "missing",
      detail: "No model has been downloaded yet.",
      remediation: {
        kind: "in_app_action",
        label: "Download a model from the list below.",
        command: null,
        url: null,
        runnable: false,
        note: null,
      },
    },
  ];
}

function makeGatherPreview(): GatherPreview {
  return {
    date: TODAY,
    host: HOST,
    window: { range_start: "2026-08-01", range_end: "2026-08-02" },
    facts: [],
    notes: [],
    github: null,
    conv: null,
    prrev: null,
    claude_files: [],
    opencode_sessions: [],
    codex_sessions: [],
    gemini_sessions: [],
    grok_sessions: [],
    forbidden_tickets: [],
    covered_tickets: [],
    skew: [],
  };
}

/**
 * A signed-in, already-compiled machine: today's standup is on disk, the
 * pipeline is idle, and every settings surface has something to show. Specs
 * mutate the returned object before handing it to `app.start`.
 */
/**
 * Provider usage with something in it.
 *
 * Deliberately covers all three resource shapes the panel renders — a bounded
 * percentage, a countdown with a projection, and an unbounded credit balance —
 * because an empty panel is the one state that says nothing about the feature.
 */
export function makeProviderHealth(): ProviderHealth[] {
  return [
    {
      provider: "claude",
      availability: "available",
      source: "provider_reported",
      plan: "Max 20x",
      stale: false,
      notice: null,
      reason: null,
      checked_at: `${TODAY}T08:55:00Z`,
      windows: [
        {
          id: "session",
          label: "Session",
          kind: "consumption",
          unit: "percent",
          used: 34,
          limit: 100,
          available: null,
          used_percent: 34,
          remaining_percent: 66,
          period_duration_ms: 5 * 60 * 60 * 1000,
          resets_at: `${TODAY}T15:20:00Z`,
          pace: "ahead",
        },
        {
          id: "weekly",
          label: "Weekly",
          kind: "consumption",
          unit: "percent",
          used: 71,
          limit: 100,
          available: null,
          used_percent: 71,
          remaining_percent: 29,
          period_duration_ms: 7 * 24 * 60 * 60 * 1000,
          resets_at: "2026-08-08T00:00:00Z",
          pace: "on_track",
        },
      ],
    },
    {
      provider: "openai",
      availability: "low",
      source: "response_headers",
      plan: "Pro 20x",
      stale: false,
      notice: null,
      reason: null,
      checked_at: `${TODAY}T08:55:00Z`,
      windows: [
        {
          id: "session",
          label: "Session",
          kind: "consumption",
          unit: "percent",
          used: 88,
          limit: 100,
          available: null,
          used_percent: 88,
          remaining_percent: 12,
          period_duration_ms: 5 * 60 * 60 * 1000,
          resets_at: `${TODAY}T13:45:00Z`,
          pace: "behind",
        },
        {
          id: "credits",
          label: "Credits",
          kind: "balance",
          unit: "credits",
          used: null,
          limit: null,
          available: 821,
          used_percent: null,
          remaining_percent: null,
          period_duration_ms: null,
          resets_at: null,
          pace: null,
        },
      ],
    },
  ];
}

/**
 * The shipped GGUF catalog, byte counts and all, with the 2B downloaded and
 * selected: an empty catalog renders a panel that says nothing about what the
 * feature does.
 */
export function makeLocalModels(): LocalModelInfo[] {
  const base = {
    format: "GGUF",
    context_length: 32_768,
    downloaded_bytes: 0,
    runtime_cache_bytes: 0,
    error: null,
  } as const;
  return [
    {
      ...base,
      id: "gemma3:1b",
      display_name: "Gemma 3 1B (Fast)",
      tier: "extra_small",
      quality: "fast",
      size_bytes: 1_069_306_624,
      status: "not_downloaded",
      selected: false,
      license: "Gemma Terms of Use",
      license_url: "https://ai.google.dev/gemma/terms",
      terms_required: true,
    },
    {
      ...base,
      id: "qwen3.5:2b",
      display_name: "Qwen 3.5 2B (Balanced)",
      tier: "small",
      quality: "balanced",
      size_bytes: 1_280_835_840,
      status: "available",
      selected: true,
      license: "Apache-2.0",
      license_url: "https://www.apache.org/licenses/LICENSE-2.0",
      terms_required: false,
      downloaded_bytes: 1_280_835_840,
      runtime_cache_bytes: 268_435_456,
    },
    {
      ...base,
      id: "gemma3:4b",
      display_name: "Gemma 3 4B (Balanced)",
      tier: "medium",
      quality: "balanced",
      size_bytes: 2_489_758_112,
      status: "not_downloaded",
      selected: false,
      license: "Gemma Terms of Use",
      license_url: "https://ai.google.dev/gemma/terms",
      terms_required: true,
    },
    {
      ...base,
      id: "qwen3.5:4b",
      display_name: "Qwen 3.5 4B (High Quality)",
      tier: "large",
      quality: "high_quality",
      size_bytes: 2_740_937_888,
      status: "not_downloaded",
      selected: false,
      license: "Apache-2.0",
      license_url: "https://www.apache.org/licenses/LICENSE-2.0",
      terms_required: false,
    },
  ];
}

export function makeScenario(): Scenario {
  return {
    state: {
      config: makeAppConfig(),
      hostSlug: HOST,
      dataSources: makeDataSources(),
      providers: makeProviders(),
      providerTests: {
        claude: { ok: true, message: "claude-sonnet-4 responded", latency_ms: 42 },
        ollama: { ok: false, message: "connection refused", latency_ms: 0 },
      },
      providerModels: {
        claude: ["claude-sonnet-4", "claude-opus-4"],
        ollama: ["llama3.1", "llama3.2:latest"],
      },
      providerHealth: makeProviderHealth(),
      // Two files, because the two dates are different things: Monday's file
      // holds the weekend's work and is what History browses, while the
      // dashboard is already filling Tuesday's.
      standups: {
        [TODAY]: makeStandupFile(),
        [FILING_DATE]: makeFilingDateStandupFile(),
      },
      sidecars: { [TODAY]: makeSidecars() },
      auditData: {
        [sidecarPath(TODAY, HOST)]: makeAuditData(),
        [sidecarPath(TODAY, OTHER_HOST)]: makeAuditData({
          host: OTHER_HOST,
          rendered_at: "2026-08-03T07:20:00Z",
          render_used: "det",
          provider: null,
          model: null,
          facts: [],
          notes: [],
          covered_tickets: [],
          forbidden_tickets: [],
          hash: "sha256:cafebabe",
        }),
      },
      pipelineStatus: makeIdlePipelineStatus(),
      schedulerStatus: {
        enabled: true,
        source: "launchd",
        cron: "0 9 * * 1-5",
        next_run_at: "2026-08-04T09:00:00Z",
        last_run_at: "2026-08-03T09:00:00Z",
        last_trigger: "scheduled",
      },
      gatherPreview: makeGatherPreview(),
      repos: [
        {
          path: "/Users/tester/Github/autostand",
          name: "autostand",
          remote: "git@github.com:tester/autostand.git",
          last_commit_at: "2026-08-02T18:04:00Z",
        },
      ],
      settingsPaths: makeSettingsPaths(),
      pathValidations: [
        {
          path: "/Users/tester/Github",
          label: "github_dir",
          exists: true,
          readable: true,
          message: null,
        },
        {
          path: DAILIES_DIR,
          label: "dailies_dir",
          exists: true,
          readable: true,
          message: null,
        },
      ],
      compileResult: makeCompileResult(),
      cloudFolders: makeCloudFolders(),
      dependencies: makeDependencies(),
      localModels: makeLocalModels(),
    },
    defer: [],
    errors: {},
  };
}
