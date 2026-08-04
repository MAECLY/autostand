/**
 * Nothing filed yet.
 *
 * `read_standup_file` rejects with `not_found` until the day's first compile
 * writes the file. That is the normal state every morning, so the dashboard has
 * to tell the user what to do next — and keep telling `not_found` apart from a
 * genuine read failure, which is not something a "Compile now" button fixes.
 */

import { expect, test } from "./support/fixtures";
import { makeScenario, TODAY_LABEL } from "./support/scenario";

test("shows the informative empty state for a date with no standup file", async ({
  page,
  app,
}) => {
  const scenario = makeScenario();
  scenario.state.standups = {};
  await app.start(scenario);

  await expect(
    page.getByRole("heading", { name: `No standup filed for ${TODAY_LABEL}` }),
  ).toBeVisible();
  await expect(
    page.getByText("Compiling gathers commits, notes and enrichment"),
  ).toBeVisible();

  // The empty state offers the fix, and the shell around it is intact — this is
  // an empty day, not a broken app.
  await expect(page.getByRole("button", { name: "Compile now" })).toHaveCount(2);
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
    page.getByRole("heading", { name: `No standup filed for ${TODAY_LABEL}` }),
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
