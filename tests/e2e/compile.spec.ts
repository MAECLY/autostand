/**
 * "Compile now": the dashboard's single action, from click to result.
 *
 * `trigger_run_now` is deferred so the run stays in flight for as long as the
 * spec needs, and progress is driven by the same `pipeline-*` events the Rust
 * side emits (`docs/tauri/02-ipc-contracts.md` § Events). That is the only way
 * to assert the in-flight UI without racing a resolved promise.
 */

import { expect, test, toasts } from "./support/fixtures";
import {
  HOST,
  makeCompileResult,
  makeScenario,
  makeStandupFile,
  TODAY,
} from "./support/scenario";

test("disables the button, streams progress, then shows the result", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.defer = ["trigger_run_now"];
  await app.start(scenario);

  const main = page.getByRole("main");
  const compile = main.getByRole("button", { name: "Compile now" });
  // While a run is live the button reports the step instead of its label.
  const running = (label: string) => main.getByRole("button", { name: label });

  await expect(compile).toBeEnabled();
  await compile.click();

  // The click alone takes the action away — the mutation is pending before any
  // event arrives, so a double-click cannot start a second run.
  await expect(compile).toHaveCount(0);
  await expect(running("starting · 0%")).toBeDisabled();

  await app.pipelineStarted({ date: TODAY, host: HOST, trigger: "manual" });
  await expect(page.getByText("gathering", { exact: true })).toBeVisible();

  await app.pipelineProgress({
    date: TODAY,
    host: HOST,
    step: "gather",
    percent: 25,
  });
  await expect(running("gather · 25%")).toBeDisabled();
  await expect(page.getByText("Gathering", { exact: true })).toBeVisible();

  await app.pipelineProgress({
    date: TODAY,
    host: HOST,
    step: "render_llm",
    percent: 70,
  });
  await expect(running("render_llm · 70%")).toBeDisabled();
  await expect(page.getByText("Rendering", { exact: true })).toBeVisible();
  // The status bar mirrors the same percentage as a real progress bar.
  await expect(
    page.getByRole("progressbar").first(),
  ).toHaveAttribute("aria-valuenow", "70");

  // The run wrote a new bullet, so the refetch after `pipeline-done` must pick
  // it up rather than re-showing the pre-compile body.
  await app.patchState({
    standups: {
      [TODAY]: makeStandupFile({
        auto_blocks: [
          { host: HOST, body: "- FIF-136 shipped the release workflow" },
        ],
      }),
    },
  });

  const result = makeCompileResult({ message: "4 bullets across 2 repos" });
  await app.pipelineDone(result);
  await app.settle("trigger_run_now", result);

  await expect(compile).toBeEnabled();
  await expect(page.getByText("done", { exact: true })).toBeVisible();
  const toast = toasts(page);
  await expect(toast.getByText(`Standup compiled — ${TODAY}`)).toBeVisible();
  await expect(toast.getByText("4 bullets across 2 repos")).toBeVisible();
  await expect(
    page.getByText("FIF-136 shipped the release workflow"),
  ).toBeVisible();
});

test("surfaces a pipeline failure as a toast without wiping the last result", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.defer = ["trigger_run_now"];
  await app.start(scenario);

  const previousBullet = page.getByText(
    "FIF-136 wired the compile pipeline end to end",
  );
  await expect(previousBullet).toBeVisible();

  await page.getByRole("button", { name: "Compile now" }).click();
  await app.pipelineStarted({ date: TODAY, host: HOST, trigger: "manual" });
  await app.pipelineError({
    code: "llm_timeout",
    message: "claude CLI timed out after 180s",
    step: "render_llm",
    date: TODAY,
  });
  await app.fail("trigger_run_now", {
    code: "llm_timeout",
    message: "claude CLI timed out after 180s",
  });

  // One toast, from the mutation that started the run — the event handler
  // defers to it so a manual run is never announced twice.
  const toast = toasts(page);
  await expect(toast.getByText("Run now — llm_timeout")).toBeVisible();
  await expect(toast.getByText("claude CLI timed out after 180s")).toBeVisible();

  await expect(
    page.getByRole("heading", { name: "Pipeline failed" }),
  ).toBeVisible();
  await expect(page.getByText("Error", { exact: true })).toBeVisible();

  // The failure must not take the last good standup down with it.
  await expect(previousBullet).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Daily Standup — August 03, 2026" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Compile now" })).toBeEnabled();
});
