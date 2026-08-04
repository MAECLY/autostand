import { defineConfig, devices } from "@playwright/test";

/**
 * Playwright config for the landing page.
 *
 * The suite runs against the *built* site served the way GitHub Pages serves it:
 * `astro build` writes `dist/`, `astro preview` serves it under the configured
 * `base` (`/autostand`). That is deliberate — a dev server would mount the page
 * at `/` and silently hide a base-path regression, which is exactly the class of
 * bug that only shows up in the deployed artifact (missing images, dead nav).
 *
 * Everything is same-origin and prerendered: no network stubbing is needed, and
 * the suite is hermetic as long as nothing reaches outside 127.0.0.1.
 */

/** Loopback only — the preview server must never bind a routable interface in CI. */
const HOST = "127.0.0.1";

/**
 * Astro's default preview port. Override when something else already owns it;
 * the app's own Tauri E2E suite uses 1420, so these two never collide.
 */
const PORT = Number(process.env.LANDING_E2E_PORT ?? 4321);

const ORIGIN = `http://${HOST}:${PORT}`;

/** Must match `base` in astro.config.mjs, with the trailing slash a server adds. */
const BASE_PATH = "/autostand/";

const isCi = Boolean(process.env.CI);

export default defineConfig({
  testDir: "./e2e",
  // Artifacts stay inside the directory this suite owns, next to e2e/.gitignore,
  // so a test run never leaves untracked noise at the package root.
  outputDir: "./e2e/.artifacts/test-results",
  fullyParallel: true,
  // A stray `test.only` must fail the build rather than silently skip the suite.
  forbidOnly: isCi,
  retries: isCi ? 2 : 0,
  workers: isCi ? 1 : undefined,
  reporter: isCi
    ? [["github"], ["html", { outputFolder: "./e2e/.artifacts/report", open: "never" }]]
    : [["list"]],

  use: {
    baseURL: ORIGIN,
    trace: "on-first-retry",
    screenshot: "only-on-failure",
    // Pin the media query the pre-paint script in Base.astro branches on, so a
    // first visit with no stored preference resolves to light everywhere. The
    // theme spec opts into dark explicitly where that is the thing under test.
    colorScheme: "light",
    // Cuts the Radix accordion's open/close transition, so "the panel is gone"
    // is a state rather than a race. It does NOT stop `scroll-smooth` — Chromium
    // animates anchor jumps regardless — which is why `expectAnchorLanded` in
    // e2e/fixtures.ts polls for the destination instead of trusting the click.
    contextOptions: { reducedMotion: "reduce" },
  },

  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"] } }],

  webServer: {
    // Build, then serve the build. `astro preview` honours `base`, so the site
    // answers on /autostand/ and 404s anything outside it — same as Pages.
    command: `pnpm run build && pnpm exec astro preview --host ${HOST} --port ${PORT}`,
    url: `${ORIGIN}${BASE_PATH}`,
    reuseExistingServer: !isCi,
    timeout: 180_000,
    stdout: "pipe",
    stderr: "pipe",
  },
});
