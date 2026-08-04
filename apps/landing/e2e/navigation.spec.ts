/**
 * Every navbar anchor reaches its section.
 *
 * A dead in-page link is invisible to a unit test — the `href` and the `id` can
 * both be present and still not meet, because the section may not exist yet at
 * click time, or the sticky header may sit on top of whatever it scrolls to.
 * Only a browser can tell you where the page actually ended up.
 */
import { expect, test } from "@playwright/test";

import { expectAnchorLanded, expectScrolledToTop, gotoLanding, SECTION_IDS } from "./fixtures";

test.beforeEach(async ({ page }) => {
  await gotoLanding(page);
});

for (const id of SECTION_IDS) {
  test(`the "${id}" navbar anchor scrolls to its section`, async ({ page }) => {
    const link = page.getByRole("navigation", { name: "Main" }).locator(`a[href="#${id}"]`);
    await expect(link).toHaveCount(1);

    await link.click();

    await expect(page).toHaveURL(new RegExp(`#${id}$`));
    await expectAnchorLanded(page, id);
  });
}

test("the logo returns to the top of the document", async ({ page }) => {
  await page.getByRole("navigation", { name: "Main" }).locator('a[href="#faq"]').click();
  await expectAnchorLanded(page, "faq");

  // "#top" has no matching element: the HTML spec says that fragment means the
  // top of the document, which is exactly what the logo link relies on.
  await page.getByRole("banner").locator('a[href="#top"]').click();
  await expectScrolledToTop(page);
});

test("the navbar only links to targets that exist", async ({ page }) => {
  const hrefs = await page
    .getByRole("navigation", { name: "Main" })
    .locator("a")
    .evaluateAll((links) => links.map((link) => link.getAttribute("href") ?? ""));

  expect(hrefs).toEqual(SECTION_IDS.map((id) => `#${id}`));

  for (const href of hrefs) {
    await expect(page.locator(href), `target of ${href}`).toHaveCount(1);
  }
});

test("deep-linking straight to a section lands on it", async ({ page }) => {
  // Someone pasting https://…/autostand/#audit into a browser should not have to
  // scroll: the fragment has to resolve on first load, not only after a click.
  await page.goto("/autostand/#audit", { waitUntil: "domcontentloaded" });

  await expectAnchorLanded(page, "audit");
});
