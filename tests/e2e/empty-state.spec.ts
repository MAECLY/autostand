/**
 * Nothing filed yet.
 *
 * `read_standup_file` rejects with `not_found` until the day's first compile
 * writes the file. That is the normal state every morning, so the dashboard has
 * to tell the user what to do next — and keep telling `not_found` apart from a
 * genuine read failure, which is not something a "Compile now" button fixes.
 *
 * The date in the empty state is the **filing** date: the file that does not
 * exist yet is the one a compile would create, not the calendar day.
 */

import { expect, test } from "./support/fixtures";
import { FILING_DATE_LABEL, makeScenario } from "./support/scenario";

test("shows the informative empty state for a date with no standup file", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.state.standups = {};
  await app.start(scenario);

  await expect(
    page.getByRole("heading", { name: `No standup filed for ${FILING_DATE_LABEL}` }),
  ).toBeVisible();
  await expect(
    page.getByText("Use Compile now to gather commits, notes and enrichment"),
  ).toBeVisible();

  // The empty state names the fix and the header owns it — one "Compile now" on
  // the page, never a second copy — and the shell around it is intact: this is
  // an empty day, not a broken app.
  await expect(page.getByRole("button", { name: "Compile now" })).toHaveCount(1);
  await expect(
    page.getByRole("heading", { name: "Could not read today's standup" }),
  ).toHaveCount(0);
  await expect(page.getByRole("contentinfo")).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Main" })).toBeVisible();
});

test("distinguishes a real read failure from an empty day", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.errors.read_standup_file = {
    code: "io_error",
    message: "permission denied reading the dailies directory",
  };
  await app.start(scenario);

  await expect(
    page.getByRole("heading", { name: "Could not read today's standup" }),
  ).toBeVisible();
  await expect(page.getByText("io_error")).toBeVisible();
  await expect(
    page.getByText("permission denied reading the dailies directory"),
  ).toBeVisible();
  await expect(
    page.getByRole("heading", { name: `No standup filed for ${FILING_DATE_LABEL}` }),
  ).toHaveCount(0);
});

test("history reports an empty dailies directory rather than a blank rail", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.state.standups = {};
  await app.start(scenario, "/history");

  await expect(
    page.getByRole("heading", { name: "Nothing filed yet" }),
  ).toBeVisible();
  await expect(
    page.getByText("0 days with a standup file"),
  ).toBeVisible();
  await expect(page.getByText("no file").first()).toBeVisible();
});
