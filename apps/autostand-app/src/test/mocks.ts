/**
 * Shared Tauri test doubles and fixture factories.
 *
 * The IPC boundary is the only thing the frontend cannot exercise for real, so
 * every test replaces `@tauri-apps/api` with the two spies below. `vi.mock` is
 * hoisted above imports, which is why the module factories are exposed as
 * functions — a test file can do:
 *
 *   vi.mock("@tauri-apps/api/core", async () =>
 *     (await import("@/test/mocks")).tauriCoreMock());
 *
 * without tripping over the hoisting rules.
 */

import { vi } from "vitest";

import type {
  AppConfig,
  AuditData,
  AuditSidecar,
  CloudFolder,
  CompileResult,
  DataSourceConfig,
  DataSourceConfigs,
  PipelineStatus,
  ProviderConfig,
  StandupFileContent,
} from "@/lib/types";

// ── invoke ────────────────────────────────────────────────────────────────

/** Stand-in for `invoke`. Resolves `undefined` until a test configures it. */
export const mockInvoke = vi.fn();

export type InvokeHandler = (args: Record<string, unknown>) => unknown;

/**
 * Route `invoke` by command name. An unlisted command rejects loudly rather
 * than resolving `undefined`, so a typo in a wrapper fails the test instead of
 * silently producing empty data.
 */
export function mockInvokeCommands(
  handlers: Readonly<Record<string, InvokeHandler>>,
): void {
  mockInvoke.mockImplementation(
    (command: string, args: Record<string, unknown> = {}) => {
      const handler = handlers[command];
      if (!handler) {
        return Promise.reject(
          new Error(`invoke called with unstubbed command: ${command}`),
        );
      }
      return Promise.resolve(handler(args));
    },
  );
}

/** Command names `invoke` was called with, in order. */
export function invokedCommands(): string[] {
  return mockInvoke.mock.calls.map((call) => call[0] as string);
}

/** How many times a given command was invoked. */
export function invokeCount(command: string): number {
  return invokedCommands().filter((name) => name === command).length;
}

// ── listen ────────────────────────────────────────────────────────────────

export interface MockTauriEvent {
  event: string;
  id: number;
  payload: unknown;
}

type EventHandler = (event: MockTauriEvent) => void;

const listeners = new Map<string, Set<EventHandler>>();
let nextEventId = 0;

function listenImpl(
  name: string,
  handler: EventHandler,
): Promise<() => void> {
  const handlers = listeners.get(name) ?? new Set<EventHandler>();
  handlers.add(handler);
  listeners.set(name, handlers);
  // Registration is synchronous so a test can emit right after the effect runs,
  // but the returned promise still models the real async `listen`.
  return Promise.resolve(() => {
    handlers.delete(handler);
  });
}

/** Stand-in for `listen`. Backed by an in-memory event bus. */
export const mockListen = vi.fn(listenImpl);

/** Push a payload to every handler currently subscribed to `name`. */
export function emitTauriEvent(name: string, payload: unknown): void {
  const handlers = listeners.get(name);
  if (!handlers) return;
  nextEventId += 1;
  // Copy: a handler may unsubscribe itself while we iterate.
  for (const handler of [...handlers]) {
    handler({ event: name, id: nextEventId, payload });
  }
}

/** Live subscriber count — the leak check for unmount cleanup. */
export function tauriListenerCount(name: string): number {
  return listeners.get(name)?.size ?? 0;
}

/** Every event name that still has at least one subscriber. */
export function subscribedEventNames(): string[] {
  return [...listeners.entries()]
    .filter(([, handlers]) => handlers.size > 0)
    .map(([name]) => name)
    .sort();
}

// ── module factories + reset ──────────────────────────────────────────────

export function tauriCoreMock() {
  return { invoke: mockInvoke };
}

export function tauriEventMock() {
  return { listen: mockListen };
}

/** Call from `beforeEach`: no state leaks between tests, no live subscribers. */
export function resetTauriMocks(): void {
  listeners.clear();
  nextEventId = 0;
  mockInvoke.mockReset();
  mockInvoke.mockResolvedValue(undefined);
  mockListen.mockReset();
  mockListen.mockImplementation(listenImpl);
}

// ── fixtures ──────────────────────────────────────────────────────────────

export const FIXTURE_DATE = "2026-08-03";
export const FIXTURE_HOST = "mbp-miguel";
export const FIXTURE_OTHER_HOST = "linux-lab";

export function makeProviderConfig(
  overrides: Partial<ProviderConfig> = {},
): ProviderConfig {
  return {
    id: "claude",
    enabled: true,
    mode: "CliFirst",
    model: "claude-sonnet-4",
    cli_path: "/usr/local/bin/claude",
    api_key_ref: null,
    api_base_url: null,
    timeout_secs: 120,
    ...overrides,
  };
}

export function makeDataSourceConfigs(
  overrides: Partial<DataSourceConfigs> = {},
): DataSourceConfigs {
  return {
    local_git: true,
    github: true,
    claude_code: true,
    remember: false,
    opencode: false,
    codex: false,
    gemini_cli: false,
    grok_cli: false,
    ...overrides,
  };
}

export function makeAppConfig(overrides: Partial<AppConfig> = {}): AppConfig {
  return {
    github_dir: "/Users/tester/Github",
    dailies_dir: "/Users/tester/Sync/Github_Dailies",
    standup_authors: ["Tester"],
    git_refs: "--all",
    jira_base: "https://example.atlassian.net/browse",
    host_slug_override: null,
    render_mode: "Auto",
    llm: {
      preferred_provider: "claude",
      providers: [makeProviderConfig()],
      fallback_enabled: true,
      provider_order: ["claude"],
      fallback_policy: {
        retry_rate_limits: true,
        max_retry_after_secs: 30,
      },
      local_runtime_policy: "on_demand",
    },
    data_sources: makeDataSourceConfigs(),
    scheduler: { enabled: true, cron: "0 9 * * 1-5", self_heal: true },
    review: {
      reviewer: "tester",
      pr_org: "example",
      max_prs: 20,
      comment_len: 280,
      include_self_reviews: false,
    },
    scrub: { alias_scrub: true, alias_scrub_min: 4, meta_extra: null },
    notifications: {
      enabled: false,
      low_usage: true,
      low_usage_threshold_percent: 20,
      provider_exhausted: true,
      provider_fallback: true,
      local_model_downloads: true,
      standup_complete: false,
      standup_failed: true,
    },
    sync: { cloud_root: null, repo_enabled: false },
    regeneration: { replace_immediately: false },
    format: {
      preset: "classic-scrum",
      verbosity: "standard",
      include_pr_review: true,
      include_confidence: false,
      include_risks: false,
      conventional: false,
    },
    ...overrides,
  };
}

export function makeDataSource(
  overrides: Partial<DataSourceConfig> = {},
): DataSourceConfig {
  return {
    id: "github",
    label: "GitHub",
    enabled: false,
    description: "Pull requests and reviews via the gh CLI.",
    ...overrides,
  };
}

export function makeCompileResult(
  overrides: Partial<CompileResult> = {},
): CompileResult {
  return {
    date: FIXTURE_DATE,
    host: FIXTURE_HOST,
    status: "ok",
    render_used: "llm",
    fellback: false,
    audit_path: `/Users/tester/.autostand/audit/${FIXTURE_DATE}.${FIXTURE_HOST}.json`,
    file_path: `/Users/tester/Sync/Github_Dailies/${FIXTURE_DATE}.md`,
    accumulated_count: 0,
    message: "3 bullets across 2 repos",
    ...overrides,
  };
}

export function makeStandupFileContent(
  overrides: Partial<StandupFileContent> = {},
): StandupFileContent {
  return {
    date: FIXTURE_DATE,
    title: "Daily Standup — August 03, 2026",
    subtitle: "_Work completed August 01–02, 2026._",
    auto_blocks: [
      { host: FIXTURE_HOST, body: "- FIF-136 wired the pipeline" },
      { host: FIXTURE_OTHER_HOST, body: "- FIF-141 drafted the discovery KB" },
    ],
    manual_region: "- Pairing session with the platform team",
    ...overrides,
  };
}

export function makePipelineStatus(
  overrides: Partial<PipelineStatus> = {},
): PipelineStatus {
  return {
    state: "idle",
    current_date: null,
    current_host: null,
    step: null,
    percent: 0,
    last_run_at: null,
    last_result: null,
    error: null,
    ...overrides,
  };
}

export function makeAuditData(overrides: Partial<AuditData> = {}): AuditData {
  return {
    file: `/Users/tester/Sync/Github_Dailies/${FIXTURE_DATE}.md`,
    host: FIXTURE_HOST,
    rendered_at: "2026-08-03T07:15:00Z",
    window: { range_start: "2026-08-01", range_end: "2026-08-02" },
    facts: [
      {
        repo: "autostand",
        ticket: "FIF-136",
        title: "wire the pipeline",
        commits: [
          {
            sha: "0f1e2d3",
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
    forbidden_tickets: [],
    covered_tickets: ["FIF-136"],
    skew: [],
    ticket_days: { "FIF-136": ["2026-08-02"] },
    render_mode: "auto",
    render_used: "llm",
    provider: "claude",
    model: "claude-sonnet-4",
    provider_attempts: [],
    fellback: false,
    hash: "sha256:deadbeef",
    accumulated_count: 0,
    ...overrides,
  };
}

export function makeAuditSidecar(
  overrides: Partial<AuditSidecar> = {},
): AuditSidecar {
  return {
    path: `/Users/tester/.local/state/autostand/audit/${FIXTURE_DATE}-${FIXTURE_HOST}.json`,
    date: FIXTURE_DATE,
    host: FIXTURE_HOST,
    rendered_at: "2026-08-03T07:15:00Z",
    render_used: "llm",
    provider: "claude",
    model: "claude-sonnet-4",
    fellback: false,
    ...overrides,
  };
}

export function makeCloudFolder(
  overrides: Partial<CloudFolder> = {},
): CloudFolder {
  return {
    id: "icloud-drive",
    label: "iCloud Drive",
    path: "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs",
    dailies_path:
      "/Users/tester/Library/Mobile Documents/com~apple~CloudDocs/autostand",
    exists: true,
    provider: "iCloud",
    ...overrides,
  };
}
