/**
 * "Compile now": the dashboard's single action, from click to filed standup.
 *
 * The click no longer writes the file. `preview_regeneration` renders a
 * candidate in an isolated directory and the review dialog resolves it, so the
 * journey is: candidate in flight → progress → review → apply. That command is
 * deferred so the run stays in flight for as long as the spec needs, and
 * progress is driven by the same events the Rust side emits while the candidate
 * renders (`docs/tauri/02-ipc-contracts.md` § Events). That is the only way to
 * assert the in-flight UI without racing a resolved promise.
 *
 * A regeneration emits no `pipeline-started` / `pipeline-done`: those belong to
 * `pipeline_runner::run`, which `preview_regeneration` bypasses by calling
 * `compile_one` directly. What brackets the work is the run itself —
 * `run-started` / `run-finished` with `pipeline: true` — and the
 * `pipeline-progress` events each step still emits.
 */

import type {
  RegenerationPreview,
  RunFinishedEvent,
  RunStartedEvent,
} from "../../apps/autostand-app/src/lib/types";
import { expect, test, toasts } from "./support/fixtures";
import {
  FILING_DATE,
  HOST,
  makeFilingDateStandupFile,
  makeScenario,
} from "./support/scenario";

/**
 * The AUTO body already on disk, and the one the candidate proposes.
 *
 * Each has two forms on purpose: the review dialog shows the raw markdown in a
 * `<pre>`, while the standup preview renders it, so the leading `- ` is part of
 * the body but never part of the bullet the user reads.
 */
const CURRENT_BULLET = "FIF-136 wired the compile pipeline end to end";
const CANDIDATE_BULLET = "FIF-136 shipped the release workflow";
const CURRENT_BODY = `- ${CURRENT_BULLET}`;
const CANDIDATE_BODY = `- ${CANDIDATE_BULLET}`;

/** 64 hex characters — the only token shape `apply_regeneration` accepts. */
const TOKEN = "a".repeat(64);

const RUN_ID = "regenerate-2026-08-04";

function makePreview(): RegenerationPreview {
  return {
    token: TOKEN,
    date: FILING_DATE,
    host: HOST,
    current_auto: CURRENT_BODY,
    candidate_auto: CANDIDATE_BODY,
    base_hash: "b".repeat(64),
    expires_at: "2026-08-03T12:30:00Z",
    render_used: "llm",
    fellback: false,
    message: "4 bullets across 2 repos",
  };
}

/** `pipeline: true` is what hands a run the dashboard's progress bar. */
function makeRunStarted(): RunStartedEvent {
  return {
    run_id: RUN_ID,
    kind: "regenerate",
    title: "Regenerate standup",
    date: FILING_DATE,
    host: HOST,
    pipeline: true,
  };
}

function makeRunFinished(ok: boolean, message: string): RunFinishedEvent {
  return {
    run_id: RUN_ID,
    kind: "regenerate",
    ok,
    message,
    duration_ms: 4_200,
    pipeline: true,
  };
}

test("disables the button, streams progress, then files the reviewed candidate", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.defer = ["preview_regeneration"];
  await app.start(scenario);

  const main = page.getByRole("main");
  const statusBar = page.getByRole("contentinfo");
  const compile = main.getByRole("button", { name: "Compile now" });
  // While the candidate is being generated the button reports that phase
  // instead of its label.
  const running = main.getByRole("button", { name: /Generating comparison/ });

  await expect(compile).toBeEnabled();
  await compile.click();

  // The click alone takes the action away — the mutation is pending before any
  // event arrives, so a double-click cannot start a second run.
  await expect(compile).toHaveCount(0);
  await expect(running).toBeDisabled();

  await app.emit("run-started", makeRunStarted());
  await expect(statusBar.getByText("Gathering", { exact: true })).toBeVisible();

  await app.pipelineProgress({
    date: FILING_DATE,
    host: HOST,
    step: "gather",
    percent: 25,
  });
  await expect(running).toBeDisabled();
  await expect(statusBar.getByText("gather", { exact: true })).toBeVisible();

  await app.pipelineProgress({
    date: FILING_DATE,
    host: HOST,
    step: "render_llm",
    percent: 70,
  });
  await expect(statusBar.getByText("Rendering", { exact: true })).toBeVisible();
  // The status bar mirrors the same percentage as a real progress bar.
  await expect(statusBar.getByRole("progressbar")).toHaveAttribute(
    "aria-valuenow",
    "70",
  );

  // The candidate arriving ends the run and opens the review — the file on disk
  // is still untouched at this point.
  await app.settle("preview_regeneration", makePreview());
  await app.emit(
    "run-finished",
    makeRunFinished(true, "candidate ready for review"),
  );
  // The filing date, so the candidate is generated against the file the
  // dashboard is showing rather than against the calendar day.
  await expect(app.callsTo("preview_regeneration")).resolves.toEqual([
    { date: FILING_DATE },
  ]);

  const dialog = page.getByRole("dialog");
  await expect(
    dialog.getByRole("heading", { name: "Review regenerated standup" }),
  ).toBeVisible();
  await expect(dialog.getByText(CURRENT_BODY)).toBeVisible();
  await expect(
    dialog.locator("pre").filter({ hasText: CANDIDATE_BODY }),
  ).toBeVisible();
  // The editable result starts from the candidate, so "Save combined" without
  // an edit means the same thing as "Use new".
  await expect(
    dialog.getByRole("textbox", { name: "Combined AUTO block" }),
  ).toHaveValue(CANDIDATE_BODY);

  // Applying writes the AUTO block, so the refetch that follows must pick the
  // new bullet up rather than re-showing the pre-compile body.
  await app.patchState({
    standups: {
      [FILING_DATE]: makeFilingDateStandupFile({
        auto_blocks: [{ host: HOST, body: CANDIDATE_BODY }],
      }),
    },
  });

  await dialog.getByRole("button", { name: "Use new" }).click();

  await expect(dialog).toHaveCount(0);
  await expect(app.callsTo("apply_regeneration")).resolves.toEqual([
    { token: TOKEN, resolution: "use_candidate", mergedAuto: null },
  ]);
  const toast = toasts(page);
  await expect(toast.getByText("Standup regenerated")).toBeVisible();
  await expect(toast.getByText("regeneration applied")).toBeVisible();
  await expect(page.getByText(CANDIDATE_BULLET)).toBeVisible();
  await expect(compile).toBeEnabled();
  await expect(statusBar.getByText("Done", { exact: true })).toBeVisible();
});

test("surfaces a failed regeneration as a toast without wiping the last result", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.defer = ["preview_regeneration"];
  await app.start(scenario);

  const statusBar = page.getByRole("contentinfo");
  const previousBullet = page.getByText(CURRENT_BULLET);
  await expect(previousBullet).toBeVisible();

  await page.getByRole("button", { name: "Compile now" }).click();
  await app.emit("run-started", makeRunStarted());
  await app.emit(
    "run-finished",
    makeRunFinished(false, "claude CLI timed out after 180s"),
  );
  await app.fail("preview_regeneration", {
    code: "llm_timeout",
    message: "claude CLI timed out after 180s",
  });

  // One toast, from the mutation the click started. A regeneration that dies
  // mid-render reports through its rejection, not through `pipeline-error` —
  // that event carries only the non-fatal warnings (an LLM fallback, the
  // anti-regression skip), and the run bracket carries no diagnostic of its own.
  const toast = toasts(page);
  await expect(
    toast.getByText("Generate standup comparison — llm_timeout"),
  ).toBeVisible();
  await expect(toast.getByText("claude CLI timed out after 180s")).toBeVisible();

  await expect(statusBar.getByText("Error", { exact: true })).toBeVisible();
  await expect(
    statusBar.getByText("claude CLI timed out after 180s"),
  ).toBeVisible();

  await page.getByRole("tab", { name: "Pipeline" }).click();
  await expect(
    page.getByRole("heading", { name: "Pipeline failed" }),
  ).toBeVisible();

  // The failure must not take the last good standup down with it, and nothing
  // was written: the body on screen is still the pre-compile one.
  await page.getByRole("tab", { name: "Standup" }).click();
  await expect(previousBullet).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Daily Standup — August 04, 2026" }),
  ).toBeVisible();
  await expect(page.getByRole("button", { name: "Compile now" })).toBeEnabled();
});
