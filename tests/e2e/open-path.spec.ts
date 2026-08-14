/**
 * Settings: handing a configured folder to the OS file manager.
 *
 * The invariant worth an E2E is that the button forwards the path the user is
 * *looking at* — the Paths field, the discovered repo row, the cloud root — and
 * that a folder the backend reports as missing cannot be handed to the shell at
 * all. Both are cross-component wiring the unit tests cannot see.
 */

import type { Page } from "@playwright/test";

import { expect, test } from "./support/fixtures";
import { makeScenario } from "./support/scenario";

/** Settings opens on Providers; the folder fields live on Paths. */
async function openPaths(page: Page) {
  await page.getByRole("tab", { name: "Paths" }).click();
  await expect(
    page.getByRole("textbox", { name: "GitHub directory" }),
  ).toBeVisible();
}

test("opens the GitHub directory the Paths field is showing", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  await app.start(scenario, "/settings");
  await openPaths(page);

  await page
    .getByRole("button", { name: "Open GitHub directory in the file manager" })
    .click();

  await expect(app.callsTo("open_in_file_manager")).resolves.toEqual([
    { path: scenario.state.config.github_dir },
  ]);
});

test("opens a discovered repo from its own table row", async ({ page, app }) => {
  const scenario = makeScenario();
  const repo = scenario.state.repos[0];
  await app.start(scenario, "/settings");
  await openPaths(page);

  await page.getByRole("button", { name: "Discover repos" }).click();
  await expect(
    page.getByRole("cell", { name: repo.name, exact: true }),
  ).toBeVisible();

  await page
    .getByRole("button", { name: `Open ${repo.name} in the file manager` })
    .click();

  await expect(app.callsTo("open_in_file_manager")).resolves.toEqual([
    { path: repo.path },
  ]);
});

test("refuses to hand an undetected cloud root to the shell", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  const [detected, undetected] = scenario.state.cloudFolders;
  await app.start(scenario, "/settings");
  await page.getByRole("tab", { name: "Sync" }).click();

  await expect(
    page.getByRole("button", {
      name: `Open ${undetected.label} in the file manager`,
    }),
  ).toBeDisabled();

  await page
    .getByRole("button", { name: `Open ${detected.label} in the file manager` })
    .click();

  await expect(app.callsTo("open_in_file_manager")).resolves.toEqual([
    { path: detected.path },
  ]);
});
