/**
 * The shell: five routes behind one sidebar, and a status bar that reports what
 * the pipeline is doing regardless of which route is open.
 *
 * `usePipelineEvents` is mounted once, at the root layout, so the status bar has
 * to keep tracking a run the user navigated away from.
 */

import { expect, ROUTES, test } from "./support/fixtures";
import { HOST, makeScenario, TODAY } from "./support/scenario";

test("reaches every route from the sidebar", async ({ page, app }) => {
  await app.start();

  for (const route of ROUTES) {
    await page.getByRole("link", { name: route.link }).click();
    await expect(page).toHaveURL(new RegExp(`${route.path}$`));
    await expect(
      page.getByRole("heading", { name: route.title, level: 1 }),
    ).toBeVisible();
  }

  // Back to the start, to prove the loop left the app usable.
  await page.getByRole("link", { name: "Dashboard" }).click();
  await expect(
    page.getByRole("button", { name: "Compile now" }),
  ).toBeVisible();
});

test("status bar reports idle, then the live run, then the failure", async ({
  page,
  app,
}) => {
  await app.start();

  const statusBar = page.getByRole("contentinfo");
  await expect(statusBar.getByText("Idle", { exact: true })).toBeVisible();
  await expect(statusBar.getByText("No run in progress")).toBeVisible();
  // Machine identity and the next scheduled run are always on screen.
  await expect(statusBar.getByText(HOST, { exact: true })).toBeVisible();
  await expect(statusBar.getByText("Next run Aug 4, 09:00")).toBeVisible();

  await app.pipelineStarted({ date: TODAY, host: HOST, trigger: "scheduled" });
  await expect(statusBar.getByText("Gathering", { exact: true })).toBeVisible();

  await app.pipelineProgress({
    date: TODAY,
    host: HOST,
    step: "render_det",
    percent: 80,
  });
  await expect(statusBar.getByText("Rendering", { exact: true })).toBeVisible();
  await expect(statusBar.getByText("render_det")).toBeVisible();
  await expect(statusBar.getByText("80%")).toBeVisible();

  await app.pipelineError({
    code: "git_error",
    message: "repository is locked by another process",
    step: "gather",
    date: TODAY,
  });
  await expect(statusBar.getByText("Error", { exact: true })).toBeVisible();
  await expect(
    statusBar.getByText("repository is locked by another process"),
  ).toBeVisible();
});

test("refreshes the next scheduled run on a scheduler tick", async ({
  page,
  app,
}) => {
  await app.start();

  const statusBar = page.getByRole("contentinfo");
  await expect(statusBar.getByText("Next run Aug 4, 09:00")).toBeVisible();

  await app.patchState({
    schedulerStatus: {
      enabled: true,
      source: "launchd",
      cron: "30 7 * * 1-5",
      next_run_at: "2026-08-05T07:30:00Z",
      last_run_at: "2026-08-04T09:00:00Z",
      last_trigger: "scheduled",
    },
  });
  await app.emit("scheduler-tick", {
    next_run_at: "2026-08-05T07:30:00Z",
    source: "launchd",
  });

  await expect(statusBar.getByText("Next run Aug 5, 07:30")).toBeVisible();
});

test("keeps tracking a run started before the user changed route", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/settings");

  const statusBar = page.getByRole("contentinfo");
  await app.pipelineStarted({ date: TODAY, host: HOST, trigger: "self-heal" });
  await app.pipelineProgress({
    date: TODAY,
    host: HOST,
    step: "gather_github",
    percent: 40,
  });
  await expect(statusBar.getByText("gather_github")).toBeVisible();

  await page.getByRole("link", { name: "Debug" }).click();
  await expect(
    page.getByRole("heading", { name: "Debug", level: 1 }),
  ).toBeVisible();
  await expect(statusBar.getByText("Gathering", { exact: true })).toBeVisible();
  await expect(statusBar.getByText("gather_github")).toBeVisible();
  await expect(statusBar.getByText("40%")).toBeVisible();
});
