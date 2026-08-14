/**
 * Settings: data-source toggles, provider connectivity and tab memory.
 *
 * The invariant worth an E2E is that `local-git` cannot be turned off — every
 * AUTO bullet has to trace back to a commit, so the switch is pinned on in the
 * UI as well as rejected by the backend (`AGENTS.md` § Data sources).
 */

import type { Page } from "@playwright/test";

import { expect, test, toasts } from "./support/fixtures";
import { makeScenario } from "./support/scenario";

/**
 * Provider cards ship collapsed — mode, model, "Test" and "Store key" only
 * exist once the card's own "Configure" is pressed. Only one card is expanded
 * at a time, so afterwards those controls are unambiguous.
 */
async function expandProvider(page: Page, index: number) {
  await page.getByRole("button", { name: "Configure" }).nth(index).click();
}

/** Settings opens on Providers; the toggles live one tab over. */
async function openDataSources(page: Page) {
  await page.getByRole("tab", { name: "Data Sources" }).click();
  await expect(page.getByRole("heading", { name: "Data sources" })).toBeVisible();
}

test("persists a data source toggle across a navigation round trip", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/settings");
  await openDataSources(page);

  const github = page.getByRole("switch", { name: "Enable GitHub" });
  await expect(github).not.toBeChecked();

  await github.click();
  await expect(github).toBeChecked();
  await expect(app.callsTo("toggle_data_source")).resolves.toEqual([
    { id: "github", enabled: true },
  ]);

  // Leave and come back: the switch re-reads `list_data_sources`, so it can
  // only still be on if the backend actually kept the change.
  await page.getByRole("link", { name: "Dashboard" }).click();
  await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
  await page.getByRole("link", { name: "Settings" }).click();
  await openDataSources(page);

  await expect(page.getByRole("switch", { name: "Enable GitHub" })).toBeChecked();
});

test("pins the local-git toggle on and explains why", async ({ page, app }) => {
  await app.start(makeScenario(), "/settings");
  await openDataSources(page);

  const localGit = page.getByRole("switch", { name: "Enable Local git" });
  await expect(localGit).toBeChecked();
  await expect(localGit).toBeDisabled();

  // A disabled switch swallows pointer events, so the hint hangs off the
  // focusable wrapper the component puts around it.
  await localGit.locator("xpath=..").hover();
  await expect(
    page.getByText(
      "local-git is the authoritative source — every AUTO bullet must trace back to a commit, so it cannot be turned off.",
    ),
  ).toBeVisible();

  await expect(app.callsTo("toggle_data_source")).resolves.toEqual([]);
});

test("reopens Settings on the tab the user left it on", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/settings");

  const notifications = page.getByRole("tab", { name: "Notifications" });
  await notifications.click();
  await expect(notifications).toHaveAttribute("aria-selected", "true");
  await expect(
    page.getByRole("tabpanel", { name: "Notifications" }),
  ).toBeVisible();

  // TanStack Router unmounts the route, so Radix's own tab state is gone by the
  // time we come back — only the UI store can carry the choice across.
  await page.getByRole("link", { name: "Dashboard" }).click();
  await expect(page.getByRole("heading", { name: "Dashboard" })).toBeVisible();
  await page.getByRole("link", { name: "Settings" }).click();

  await expect(page.getByRole("tab", { name: "Notifications" })).toHaveAttribute(
    "aria-selected",
    "true",
  );
  await expect(page.getByRole("tab", { name: "Providers" })).toHaveAttribute(
    "aria-selected",
    "false",
  );
  await expect(
    page.getByRole("tabpanel", { name: "Notifications" }),
  ).toBeVisible();
  await expect(
    page.getByText(
      "Choose which provider, usage and scheduler events may reach the operating system.",
    ),
  ).toBeVisible();
});

test("reports a provider test with its transport and latency", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/settings");

  await expect(
    page.getByRole("heading", { name: "Claude", exact: true }),
  ).toBeVisible();
  // One card per provider, in `list_llm_providers` order — Claude first. The
  // recorded arguments below are what actually prove the right card was hit.
  await expandProvider(page, 0);
  await page.getByRole("button", { name: "Test" }).click();

  // Claude is configured CLI-first, so "Test" must probe the CLI transport.
  await expect(app.callsTo("test_llm_provider")).resolves.toEqual([
    { provider: "claude", mode: "cli" },
  ]);

  const providers = page.getByRole("tabpanel", { name: "Providers" });
  await expect(providers.getByText("OK · 42 ms")).toBeVisible();
  await expect(providers.getByText("claude-sonnet-4 responded")).toBeVisible();
  await expect(toasts(page).getByText("claude (cli) — 42 ms")).toBeVisible();
});

test("reports a failing provider test without claiming success", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/settings");

  await expect(
    page.getByRole("heading", { name: "Ollama", exact: true }),
  ).toBeVisible();
  // Second card, same ordering as the fixture; the arguments assert the rest.
  await expandProvider(page, 1);
  await page.getByRole("button", { name: "Test" }).click();

  // Ollama is API-only, so the probe must go to the API transport.
  await expect(app.callsTo("test_llm_provider")).resolves.toEqual([
    { provider: "ollama", mode: "api" },
  ]);

  const providers = page.getByRole("tabpanel", { name: "Providers" });
  await expect(providers.getByText("Test failed")).toBeVisible();
  await expect(providers.getByText("connection refused")).toBeVisible();
  await expect(toasts(page).getByText("ollama (api) failed")).toBeVisible();
});
