/**
 * Settings: data-source toggles and provider connectivity.
 *
 * The invariant worth an E2E is that `local-git` cannot be turned off — every
 * AUTO bullet has to trace back to a commit, so the switch is pinned on in the
 * UI as well as rejected by the backend (`AGENTS.md` § Data sources).
 */

import type { Page } from "@playwright/test";

import { expect, test, toasts } from "./support/fixtures";
import { makeScenario } from "./support/scenario";

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
  await page.getByRole("button", { name: "Test" }).first().click();

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
  await page.getByRole("button", { name: "Test" }).nth(1).click();

  // Ollama is API-only, so the probe must go to the API transport.
  await expect(app.callsTo("test_llm_provider")).resolves.toEqual([
    { provider: "ollama", mode: "api" },
  ]);

  const providers = page.getByRole("tabpanel", { name: "Providers" });
  await expect(providers.getByText("Test failed")).toBeVisible();
  await expect(providers.getByText("connection refused")).toBeVisible();
  await expect(toasts(page).getByText("ollama (api) failed")).toBeVisible();
});
