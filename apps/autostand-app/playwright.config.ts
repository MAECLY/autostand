/**
 * Playwright config for the app's E2E suite.
 *
 * The specs live in `tests/e2e/` (the repo convention) and drive the real Vite
 * dev build in Chromium with the Tauri IPC boundary mocked — see
 * `tests/e2e/README.md` for what that does and does not prove. A true Tauri
 * E2E needs `tauri-driver` plus a compiled binary and a display server, which
 * a plain CI runner does not have.
 */

import { defineConfig, devices } from "@playwright/test";

const PORT = 1420;
const BASE_URL = `http://localhost:${PORT}`;

const isCI = process.env.CI !== undefined && process.env.CI !== "";

/** Traces, screenshots and the HTML report, kept out of the app package. */
const ARTIFACTS = "../../tests/e2e/.artifacts";

export default defineConfig({
  testDir: "../../tests/e2e",
  outputDir: `${ARTIFACTS}/test-results`,

  fullyParallel: true,
  // A stray `test.only` must never silently shrink the CI suite.
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  // One worker on CI: every worker drives the same single Vite dev server, and
  // a cold Tailwind/TS compile under contention is the usual source of flake.
  workers: isCI ? 1 : undefined,

  timeout: 30_000,
  expect: { timeout: 10_000 },

  reporter: isCI
    ? [["github"], ["html", { outputFolder: `${ARTIFACTS}/report`, open: "never" }]]
    : [["list"], ["html", { outputFolder: `${ARTIFACTS}/report`, open: "never" }]],

  use: {
    baseURL: BASE_URL,
    // Pinned so `todayIso()` and date-fns output are identical everywhere;
    // `app.start()` freezes the clock to a date inside this zone.
    timezoneId: "UTC",
    locale: "en-US",
    colorScheme: "light",
    trace: "retain-on-failure",
    screenshot: "only-on-failure",
    video: "off",
  },

  // One browser: the suite asserts application behaviour through a mocked
  // backend, not cross-engine rendering. The shipped app runs in a single
  // webview per platform, so a matrix here would only buy runtime.
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],

  webServer: {
    command: "pnpm dev",
    url: BASE_URL,
    // Locally, attach to whatever `pnpm dev` is already running; on CI always
    // start a clean one so a leaked server cannot serve stale assets.
    reuseExistingServer: !isCI,
    timeout: 120_000,
    stdout: "ignore",
    stderr: "pipe",
  },
});
