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
  GatherPreview,
  LlmProviderConfig,
  PathValidation,
  PipelineStatus,
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

/** Filing date the fixed clock resolves to. */
export const TODAY = "2026-08-03";

/** How `formatIsoDate` renders {@link TODAY}. */
export const TODAY_LABEL = "Aug 3, 2026";

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

export function makeStandupFile(
  overrides: Partial<StandupFileContent> = {},
): StandupFileContent {
  return {
    date: TODAY,
    title: "Daily Standup — August 03, 2026",
    subtitle: "_Work completed August 01–02, 2026._",
    auto_blocks: [
      { host: HOST, body: "- FIF-136 wired the compile pipeline end to end" },
      { host: OTHER_HOST, body: "- FIF-141 drafted the discovery KB" },
    ],
    manual_region: "- Pairing session with the platform team",
    ...overrides,
  };
}

export function makeCompileResult(
  overrides: Partial<CompileResult> = {},
): CompileResult {
  return {
    date: TODAY,
    host: HOST,
    status: "ok",
    render_used: "llm",
    fellback: false,
    audit_path: sidecarPath(TODAY, HOST),
    file_path: `${DAILIES_DIR}/${TODAY}.md`,
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
    render_mode: "auto",
    render_used: "llm",
    provider: "claude",
    model: "claude-sonnet-4",
    fellback: false,
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
    },
    {
      path: sidecarPath(TODAY, OTHER_HOST),
      date: TODAY,
      host: OTHER_HOST,
      rendered_at: "2026-08-03T07:20:00Z",
      render_used: "det",
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
      exists: true,
      provider: "iCloud",
    },
    {
      id: "onedrive",
      label: "OneDrive",
      path: "/Users/tester/OneDrive",
      exists: false,
      provider: "OneDrive",
    },
    {
      id: "syncthing",
      label: "Syncthing",
      path: "/Users/tester/Sync",
      exists: false,
      provider: "Syncthing",
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
      standups: { [TODAY]: makeStandupFile() },
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
    },
    defer: [],
    errors: {},
  };
}
