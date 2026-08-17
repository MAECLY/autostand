/**
 * Marketing screenshots, captured from the real UI over the mocked IPC.
 *
 * Not a test: it asserts only enough to prove each screen actually rendered
 * before the shutter, so a broken build produces a failure rather than a
 * picture of an error boundary. Run it deliberately:
 *
 *     pnpm --filter autostand-app exec playwright test capture-screenshots
 *
 * Output lands in `tests/e2e/.artifacts/screenshots/`, which is gitignored —
 * copy what you want into the landing page repository.
 */

import { expect, test } from "./support/fixtures";
import { HOST, makeScenario, TODAY } from "./support/scenario";

const OUT = "../../tests/e2e/.artifacts/screenshots";

/** Desktop-app proportions: the product is a window, not a phone. */
test.use({ viewport: { width: 1440, height: 900 } });

/** Reveal Audit and Debug so the rail shows every destination. */
async function seedFullNav(page: Parameters<typeof test>[0] extends never ? never : any) {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "autostand-ui",
      JSON.stringify({
        state: { theme: "system", showAuditNav: true, showDebugNav: true },
        version: 0,
      }),
    );
  });
}

test("dashboard", async ({ page, app }) => {
  await app.start();
  await expect(page.getByRole("button", { name: "Compile now" })).toBeVisible();
  await page.screenshot({ path: `${OUT}/01-dashboard.png` });
});

test("providers and usage", async ({ page, app }) => {
  await app.start(makeScenario(), "/settings");
  await expect(page.getByRole("tab", { name: "Providers" })).toBeVisible();
  await expect(page.getByText("Provider priority")).toBeVisible();
  await page.screenshot({ path: `${OUT}/02-providers.png` });
});

test("history", async ({ page, app }) => {
  await app.start(makeScenario(), "/history");
  await expect(
    page.getByRole("heading", { name: "History", level: 1 }),
  ).toBeVisible();
  await page.screenshot({ path: `${OUT}/03-history.png` });
});

test("audit sidecar", async ({ page, app }) => {
  await seedFullNav(page);
  await app.start(makeScenario(), "/audit");
  await expect(
    page.getByRole("heading", { name: "Audit", level: 1 }),
  ).toBeVisible();
  await page.screenshot({ path: `${OUT}/04-audit.png` });
});

test("local AI", async ({ page, app }) => {
  // The shared fixture ships the prerequisites missing, which is the right
  // default for the specs that assert on remediation. A released bundle
  // carries its own runtime, so the picture shows that instead.
  const scenario = makeScenario();
  scenario.state.dependencies = scenario.state.dependencies.map((dependency) =>
    dependency.group === "local_ai"
      ? {
          ...dependency,
          state: "ok",
          remediation: null,
          detail:
            dependency.id === "local-ai.runtime"
              ? "Bundled with the app."
              : dependency.id === "local-ai.model"
                ? "Qwen 3.5 2B (Balanced)"
                : dependency.detail,
        }
      : dependency,
  );
  await app.start(scenario, "/settings");
  await page.getByRole("tab", { name: "Local AI" }).click();
  await expect(page.getByRole("tab", { name: "Local AI" })).toHaveAttribute(
    "data-state",
    "active",
  );
  // The catalog arrives over IPC; without this the shutter catches the
  // "Loading model catalog…" placeholder and an empty panel.
  const catalogEntry = page.getByText("Qwen 3.5 4B (High Quality)");
  await expect(catalogEntry).toBeVisible();
  // The catalog is what this screen is for, and it sits below the requirements
  // block, so the shutter has to follow it down.
  await catalogEntry.scrollIntoViewIfNeeded();
  await page.screenshot({ path: `${OUT}/05-local-ai.png` });
});

test("dark theme dashboard", async ({ page, app }) => {
  await page.addInitScript(() => {
    window.localStorage.setItem(
      "autostand-ui",
      JSON.stringify({
        state: { theme: "dark", showAuditNav: false, showDebugNav: false },
        version: 0,
      }),
    );
  });
  await app.start();
  await expect(page.getByRole("button", { name: "Compile now" })).toBeVisible();
  await page.screenshot({ path: `${OUT}/06-dashboard-dark.png` });
});

test("standup for a filed day", async ({ page, app }) => {
  await app.start(makeScenario(), `/history?date=${TODAY}`);
  await expect(
    page.getByRole("heading", { name: "History", level: 1 }),
  ).toBeVisible();
  await page.screenshot({ path: `${OUT}/07-standup.png` });
  // Named so a reader of the artifacts folder knows which host filed it.
  expect(HOST).toBeTruthy();
});
