/**
 * Audit: the provenance trail behind a rendered standup.
 *
 * One sidecar is written per host per render, so the page lists them for a
 * filing date and opens one at a time. The classification legend is the part
 * that matters most — `phantom` is the class that fails an audit
 * (`docs/specs/audit.md` § Phantom detection).
 */

import { expect, test } from "./support/fixtures";
import {
  DAILIES_DIR,
  HOST,
  makeScenario,
  OTHER_HOST,
  sidecarPath,
  TODAY,
} from "./support/scenario";

/** The six classes the auditor can assign, in legend order. */
const CLASSIFICATIONS = [
  "commit",
  "github",
  "review",
  "note",
  "phantom",
  "unverified",
] as const;

test("lists a sidecar per host and opens the first one", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/audit");

  const sidecars = page
    .getByRole("table")
    .filter({ has: page.getByRole("columnheader", { name: "Render used" }) });

  // Header plus one row per host that rendered on this date.
  await expect(sidecars.getByRole("row")).toHaveCount(3);
  await expect(sidecars.getByRole("row", { name: new RegExp(HOST) })).toBeVisible();
  await expect(
    sidecars.getByRole("row", { name: new RegExp(OTHER_HOST) }),
  ).toBeVisible();

  await expect(app.callsTo("list_audit_sidecars")).resolves.toEqual([
    { date: TODAY },
  ]);

  // The first row is selected without a click, so a fresh date always shows
  // evidence rather than an empty pane.
  await expect(sidecars.getByRole("button", { name: "Viewing" })).toHaveCount(1);
  await expect(
    sidecars.getByRole("button", { name: "View", exact: true }),
  ).toHaveCount(1);
  await expect(app.callsTo("read_audit_sidecar")).resolves.toEqual([
    { path: sidecarPath(TODAY, HOST) },
  ]);

  await expect(
    page.getByRole("heading", { name: `${DAILIES_DIR}/${TODAY}.md` }),
  ).toBeVisible();
  await expect(page.getByText("sha256:deadbeef")).toBeVisible();
  await expect(page.getByText("claude-sonnet-4", { exact: false })).toBeVisible();
});

test("opens another host's sidecar on demand", async ({ page, app }) => {
  await app.start(makeScenario(), "/audit");

  const sidecars = page
    .getByRole("table")
    .filter({ has: page.getByRole("columnheader", { name: "Render used" }) });

  await expect(page.getByText("sha256:deadbeef")).toBeVisible();
  await sidecars.getByRole("button", { name: "View", exact: true }).click();

  await expect(app.callsTo("read_audit_sidecar")).resolves.toEqual([
    { path: sidecarPath(TODAY, HOST) },
    { path: sidecarPath(TODAY, OTHER_HOST) },
  ]);
  // The viewer swapped documents, not just its header.
  await expect(page.getByText("sha256:cafebabe")).toBeVisible();
  await expect(page.getByText("sha256:deadbeef")).toHaveCount(0);
  await expect(sidecars.getByRole("button", { name: "Viewing" })).toHaveCount(1);
});

test("shows every classification badge, phantom included", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/audit");

  await expect(
    page.getByRole("heading", { name: "Classification legend" }),
  ).toBeVisible();
  for (const classification of CLASSIFICATIONS) {
    await expect(
      page.getByText(classification, { exact: true }).first(),
    ).toBeVisible();
  }

  // Phantom is not decorative: it labels the forbidden-ticket list, which is
  // exactly the evidence the class is derived from.
  await expect(page.getByRole("heading", { name: "Tickets" })).toBeVisible();
  await expect(
    page.getByText("Forbidden — a code-change bullet here is a phantom"),
  ).toBeVisible();
  await expect(page.getByText("FIF-133", { exact: true })).toBeVisible();
  await expect(
    page.getByText("Covered — backed by a fact or a note"),
  ).toBeVisible();
  // FIF-136 also appears as a row in the facts table above; either occurrence
  // proves the covered ticket made it out of the sidecar.
  await expect(page.getByText("FIF-136", { exact: true }).first()).toBeVisible();
});

test("explains a date with no sidecar instead of showing a blank page", async ({
  page,
  app,
}) => {
  await app.start(makeScenario(), "/audit");
  await expect(page.getByText("sha256:deadbeef")).toBeVisible();

  await page.getByLabel("Filing date").click();
  await page.getByLabel("ISO date").fill("2026-07-31");
  await page.getByLabel("ISO date").press("Enter");

  await expect(
    page.getByRole("heading", { name: "No audit sidecar for 2026-07-31" }),
  ).toBeVisible();
  await expect(
    page.getByText("A sidecar is written on every render."),
  ).toBeVisible();
  await expect(page.getByText("sha256:deadbeef")).toHaveCount(0);
});
