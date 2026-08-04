/**
 * Dashboard: today's standup, rendered from `read_standup_file`.
 *
 * The file format guarantees one AUTO block per host plus exactly one global
 * MANUAL region (`docs/specs/standup-file-format.md`), and the preview has to
 * keep those apart — the MANUAL region is the part autostand never overwrites.
 */

import { expect, test } from "./support/fixtures";
import {
  HOST,
  makeScenario,
  makeStandupFile,
  OTHER_HOST,
  TODAY,
  TODAY_LABEL,
} from "./support/scenario";

test("loads today's standup and renders one card per AUTO block", async ({
  page,
  app,
}) => {
  await app.start();

  await expect(
    page.getByRole("heading", { name: `Today — ${TODAY_LABEL}` }),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Daily Standup — August 03, 2026" }),
  ).toBeVisible();

  // Two hosts wrote into today's file, so two AUTO cards, each tagged with its
  // own slug — that separation is what makes a two-machine day readable.
  const autoCards = page.getByRole("heading", { name: "Auto", exact: true });
  await expect(autoCards).toHaveCount(2);
  // Scoped to the page body: the status bar also prints this machine's slug.
  const body = page.getByRole("main");
  await expect(body.getByText(HOST, { exact: true })).toBeVisible();
  await expect(body.getByText(OTHER_HOST, { exact: true })).toBeVisible();

  await expect(
    page.getByText("FIF-136 wired the compile pipeline end to end"),
  ).toBeVisible();
  await expect(page.getByText("FIF-141 drafted the discovery KB")).toBeVisible();

  await expect(app.callsTo("read_standup_file")).resolves.toEqual([
    { date: TODAY },
  ]);
});

test("keeps the MANUAL region visible and labelled as never overwritten", async ({
  page,
  app,
}) => {
  await app.start();

  await expect(
    page.getByRole("heading", { name: "Manual", exact: true }),
  ).toHaveCount(1);
  await expect(page.getByText("never overwritten")).toBeVisible();
  await expect(
    page.getByText("Pairing session with the platform team"),
  ).toBeVisible();
});

test("shows the MANUAL card even when no host filed an AUTO block", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.state.standups = {
    [TODAY]: makeStandupFile({
      auto_blocks: [],
      manual_region: "- Reviewed the release checklist",
    }),
  };

  await app.start(scenario);

  await expect(
    page.getByRole("heading", { name: "Auto", exact: true }),
  ).toHaveCount(0);
  await expect(
    page.getByRole("heading", { name: "Manual", exact: true }),
  ).toBeVisible();
  await expect(
    page.getByText("Reviewed the release checklist"),
  ).toBeVisible();
});
