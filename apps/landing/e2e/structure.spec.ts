/**
 * The page renders, in one piece, at the path it is deployed to.
 *
 * These are the assertions that fail first when a section stops being composed
 * into `src/pages/index.astro`, when an island throws during SSR, or when the
 * Astro `base` moves and the whole document stops answering on `/autostand/`.
 */
import { expect, test } from "@playwright/test";

import { BASE_PATH, gotoLanding, hydrated, SECTION_IDS } from "./fixtures";

test.beforeEach(async ({ page }) => {
  await gotoLanding(page);
});

test("serves the document under the deployed base path", async ({ page }) => {
  await expect(page).toHaveURL(new RegExp(`${BASE_PATH}$`));
  await expect(page).toHaveTitle(/^autostand — /);
  await expect(page.locator("html")).toHaveAttribute("lang", "en");

  const description = page.locator('head meta[name="description"]');
  await expect(description).toHaveAttribute("content", /autostand gathers/);
});

test("nothing outside the base path is served", async ({ page }) => {
  // A regression in `base` would make the site answer on `/` instead, and every
  // root-absolute asset reference in the built HTML would 404 on Pages.
  const atRoot = await page.request.get("/", { failOnStatusCode: false });
  expect(atRoot.status()).toBe(404);
});

test("has exactly one h1, and it is the tagline", async ({ page }) => {
  const h1 = page.locator("h1");
  await expect(h1).toHaveCount(1);
  await expect(h1).toContainText("Automate your standup.");
  await expect(h1).toContainText("Know what you did.");
});

test("exposes the document landmarks a screen reader navigates by", async ({ page }) => {
  await expect(page.getByRole("banner")).toBeVisible();
  await expect(page.getByRole("main")).toBeVisible();
  await expect(page.getByRole("contentinfo")).toBeVisible();
  await expect(page.getByRole("navigation", { name: "Main" })).toBeAttached();
  await expect(page.getByRole("navigation", { name: "Footer" })).toBeVisible();
});

test("renders every section the navbar links to", async ({ page }) => {
  for (const id of SECTION_IDS) {
    const section = page.locator(`section[id="${id}"]`);
    // Exactly one: a duplicated id silently breaks every anchor pointing at it.
    await expect(section, `section #${id}`).toHaveCount(1);
    await expect(section, `section #${id}`).toBeAttached();
  }

  // Each section owns a heading, so the page outline is not a wall of prose.
  await expect(page.getByRole("heading", { name: "One compile a day" })).toBeVisible();
  await expect(page.getByRole("heading", { name: "How it works" })).toBeVisible();
  await expect(
    page.getByRole("heading", { name: "Every bullet says where it came from" }),
  ).toBeVisible();
  await expect(page.getByRole("heading", { name: "Before you run it" })).toBeVisible();
});

test("hydrates both islands and only those", async ({ page }) => {
  // ThemeToggle is client:load; Faq is client:visible, so it only comes alive
  // once scrolled to. Any third island is a JavaScript regression on a page whose
  // whole point is that it ships almost none.
  await expect(page.locator("astro-island")).toHaveCount(2);
  await hydrated(page, "ThemeToggle");

  await page.locator("#faq").scrollIntoViewIfNeeded();
  await hydrated(page, "Faq");
});

test("renders the dashboard mockup as a single labelled image", async ({ page }) => {
  const mockup = page.getByRole("img", { name: /Illustration of the autostand dashboard/ });
  await expect(mockup).toBeVisible();

  // Nothing inside is interactive: a focusable control in there would be
  // reachable by keyboard yet invisible to anyone reading the label.
  await expect(mockup.locator("a, button, input, select, textarea, [tabindex]")).toHaveCount(0);
});
