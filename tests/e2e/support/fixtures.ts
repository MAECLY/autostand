/**
 * The `app` fixture: boots the real React app in Chromium with the Tauri IPC
 * boundary replaced by a scripted backend.
 *
 * Three init scripts run, in order, before any application code:
 *
 *   1. a two-line CommonJS shim, so the mocks build has an `exports` object;
 *   2. `@tauri-apps/api/mocks` itself, straight out of the app's node_modules
 *      — the genuine `mockIPC`, not a re-implementation;
 *   3. `installMockBackend`, which hands `mockIPC` the command dispatcher and
 *      publishes `window.__E2E__` for the helpers below.
 *
 * Everything after that is the production bundle: TanStack Router, TanStack
 * Query, sonner, the design system. Only `invoke` and `listen` are fake.
 */

import { test as base, expect } from "@playwright/test";
import type { Locator, Page } from "@playwright/test";

import type {
  AppError,
  CompileResult,
  PipelineErrorEvent,
  PipelineProgressEvent,
  PipelineStartedEvent,
} from "../../../apps/autostand-app/src/lib/types";
import { installMockBackend, MOCKS_GLOBAL } from "./mock-backend";
import type { E2EBridge } from "./mock-backend";
import { FIXED_NOW, makeScenario } from "./scenario";
import type { BackendState, Scenario } from "./scenario";

declare const __dirname: string;
declare const require: {
  resolve(id: string, options?: { paths?: string[] }): string;
};

declare global {
  interface Window {
    /** Published by `installMockBackend`; only exists under this suite. */
    __E2E__: E2EBridge;
  }
}

/**
 * The CommonJS build of the mocks module, resolved through the app package so
 * it always matches the `@tauri-apps/api` version the app is built against.
 */
const MOCKS_CJS_PATH = require.resolve("@tauri-apps/api/mocks", {
  paths: [`${__dirname}/../../../apps/autostand-app`],
});

/**
 * `mocks.cjs` ends in `exports.mockIPC = mockIPC`, which a classic script has
 * nowhere to put. Give it somewhere; `installMockBackend` removes it again
 * before any page script can mistake the document for a CommonJS module.
 */
const CJS_SHIM = `
globalThis.${MOCKS_GLOBAL} = {};
Object.defineProperty(globalThis, "exports", {
  value: globalThis.${MOCKS_GLOBAL},
  configurable: true,
  writable: true,
});
`;

/** Routes the sidebar exposes, in nav order. */
export const ROUTES = [
  { path: "/", link: "Dashboard", title: "Dashboard" },
  { path: "/history", link: "History", title: "History" },
  { path: "/audit", link: "Audit", title: "Audit" },
  { path: "/debug", link: "Debug", title: "Debug" },
  { path: "/settings", link: "Settings", title: "Settings" },
] as const;

/** How long a deferred command may stay un-issued before we call it a bug. */
const PENDING_TIMEOUT_MS = 10_000;

/**
 * sonner's live region. Toast copy is deliberately close to the copy on the
 * page behind it, so notification assertions scope themselves here.
 */
export function toasts(page: Page): Locator {
  return page.getByRole("region", { name: /Notifications/ });
}

/** Per-test overrides for {@link AppHarness.start}. */
export interface StartOptions {
  /**
   * Wall clock the page runs against, ISO-8601. Defaults to {@link FIXED_NOW}.
   *
   * Only a spec whose subject *is* the calendar should touch this — the filing
   * date is one, because the rule it follows only differs on a weekend.
   */
  now?: string;
}

export interface AppHarness {
  /** Install the mock backend and open `path`. Call once per test. */
  start: (
    scenario?: Scenario,
    path?: string,
    options?: StartOptions,
  ) => Promise<void>;
  /** Push a backend event through the mocked event plugin. */
  emit: (name: string, payload: unknown) => Promise<void>;
  pipelineStarted: (payload: PipelineStartedEvent) => Promise<void>;
  pipelineProgress: (payload: PipelineProgressEvent) => Promise<void>;
  pipelineDone: (payload: CompileResult) => Promise<void>;
  pipelineError: (payload: PipelineErrorEvent) => Promise<void>;
  /** Resolve a deferred command, waiting for the UI to have issued it. */
  settle: (command: string, result: unknown) => Promise<void>;
  /** Reject a deferred command with an `AppError`. */
  fail: (command: string, error: AppError) => Promise<void>;
  /** Arguments of every `invoke` for `command`, in call order. */
  callsTo: (command: string) => Promise<Record<string, unknown>[]>;
  /** Replace top-level keys of the backend state mid-test. */
  patchState: (patch: Partial<BackendState>) => Promise<void>;
}

export const test = base.extend<{ app: AppHarness }>({
  app: async ({ page }, use) => {
    let started = false;

    const emit = async (name: string, payload: unknown): Promise<void> => {
      await page.evaluate(
        (event: { name: string; payload: unknown }) => {
          window.__E2E__.emit(event.name, event.payload);
        },
        { name, payload },
      );
    };

    const waitForPending = async (command: string): Promise<void> => {
      await page.waitForFunction(
        (name: string) => window.__E2E__.isPending(name),
        command,
        { timeout: PENDING_TIMEOUT_MS },
      );
    };

    const harness: AppHarness = {
      async start(scenario = makeScenario(), path = "/", options = {}) {
        if (started) throw new Error("app.start() must be called exactly once");
        started = true;

        // Freeze `Date` so `todayIso()` and every "x hours ago" label are
        // stable; timers keep running, so React and sonner behave normally.
        await page.clock.setFixedTime(new Date(options.now ?? FIXED_NOW));
        await page.addInitScript({ content: CJS_SHIM });
        await page.addInitScript({ path: MOCKS_CJS_PATH });
        await page.addInitScript(installMockBackend, scenario);

        await page.goto(path);
        // The shell is the proof the mock took: the app shows nothing at all
        // if `invoke` throws on the very first command.
        await expect(
          page.getByRole("navigation", { name: "Main" }),
        ).toBeVisible();
      },

      emit,

      pipelineStarted: (payload) => emit("pipeline-started", payload),
      pipelineProgress: (payload) => emit("pipeline-progress", payload),
      pipelineDone: (payload) => emit("pipeline-done", payload),
      pipelineError: (payload) => emit("pipeline-error", payload),

      async settle(command, result) {
        await waitForPending(command);
        await page.evaluate(
          (settled: { command: string; result: unknown }) =>
            window.__E2E__.settle(settled.command, settled.result),
          { command, result },
        );
      },

      async fail(command, error) {
        await waitForPending(command);
        await page.evaluate(
          (failed: { command: string; error: AppError }) =>
            window.__E2E__.fail(failed.command, failed.error),
          { command, error },
        );
      },

      callsTo: (command) =>
        page.evaluate((name: string) => window.__E2E__.callsTo(name), command),

      async patchState(patch) {
        await page.evaluate(
          (value: Partial<BackendState>) => window.__E2E__.patchState(value),
          patch,
        );
      },
    };

    await use(harness);
  },
});

export { expect };
export type { Scenario };
