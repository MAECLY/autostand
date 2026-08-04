/**
 * The page fits on a phone.
 *
 * Horizontal overflow is the failure mode a component test cannot see: it is
 * emergent, produced by one element in one section that is wider than its
 * container, and it makes the whole document scroll sideways. The audit section
 * is the risk here — it renders a table with `min-w-2xl` (672px) inside a 390px
 * viewport, and it is only safe because the table ships its own scroll wrapper.
 * If that wrapper is ever dropped, this file is what notices.
 *
 * The mobile navigation is the other phone-only path: below `md` the desktop nav
 * is display:none and the only way to reach a section is the `<details>` menu.
 */
import { expect, test } from "@playwright/test";

import {
  documentWidths,
  expectAnchorLanded,
  gotoLanding,
  hydrated,
  scrollToBottom,
  SECTION_IDS,
} from "./fixtures";

/** iPhone 12/13/14 logical width — the reference phone in the design docs. */
const PHONE = { width: 390, height: 844 } as const;

test.use({ viewport: PHONE });

test("does not scroll sideways at 390px", async ({ page }) => {
  await gotoLanding(page);
  await page.waitForLoadState("networkidle");

  const { scrollWidth, clientWidth } = await documentWidths(page);
  expect(
    scrollWidth,
    `the document is ${scrollWidth}px wide inside a ${clientWidth}px viewport`,
  ).toBeLessThanOrEqual(clientWidth);
});

test("does not scroll sideways once every section has been rendered", async ({ page }) => {
  await gotoLanding(page);
  // The FAQ is a `client:visible` island: its panels only exist after hydration,
  // so the widest state of the page is not the one that first paints.
  await page.locator("#faq").scrollIntoViewIfNeeded();
  await hydrated(page, "Faq");
  await page.waitForLoadState("networkidle");
  // Open the longest answer: an expanded panel is the widest the FAQ ever gets.
  await page.locator("#faq button[aria-expanded]").last().click();
  await scrollToBottom(page);

  const { scrollWidth, clientWidth } = await documentWidths(page);
  expect(scrollWidth).toBeLessThanOrEqual(clientWidth);
});

test("keeps the wide audit table inside its own scroller", async ({ page }) => {
  await gotoLanding(page);
  await page.locator("#audit").scrollIntoViewIfNeeded();

  const table = page.locator("#audit table");
  await expect(table).toBeVisible();

  // The table is wider than the phone on purpose; what matters is that the
  // overflow is absorbed by an ancestor rather than by <html>.
  const { tableWidth, scrollerWidth, scrollerOverflowX } = await page.evaluate(() => {
    const element = document.querySelector("#audit table");
    if (!element) throw new Error("no audit table");
    let ancestor = element.parentElement;
    while (ancestor) {
      const overflowX = getComputedStyle(ancestor).overflowX;
      if (overflowX === "auto" || overflowX === "scroll") {
        return {
          tableWidth: element.getBoundingClientRect().width,
          scrollerWidth: ancestor.clientWidth,
          scrollerOverflowX: overflowX,
        };
      }
      ancestor = ancestor.parentElement;
    }
    return { tableWidth: element.getBoundingClientRect().width, scrollerWidth: -1, scrollerOverflowX: "" };
  });

  expect(scrollerOverflowX, "the audit table needs a horizontally scrollable wrapper").toMatch(
    /auto|scroll/,
  );
  expect(tableWidth).toBeGreaterThan(scrollerWidth);

  const { scrollWidth, clientWidth } = await documentWidths(page);
  expect(scrollWidth).toBeLessThanOrEqual(clientWidth);
});

test("swaps the desktop nav for the disclosure menu", async ({ page }) => {
  await gotoLanding(page);

  await expect(page.getByRole("navigation", { name: "Main" })).toBeHidden();

  const menu = page.locator("#mobile-menu");
  const summary = menu.locator("summary");
  await expect(summary).toBeVisible();
  await expect(menu).not.toHaveAttribute("open");
  // Closed `<details>` hides its contents, so the links must not be reachable yet.
  await expect(page.getByRole("navigation", { name: "Mobile" })).toBeHidden();
});

test("the mobile menu opens, lists every section, and fits the screen", async ({ page }) => {
  await gotoLanding(page);
  await page.locator("#mobile-menu summary").click();

  const mobileNav = page.getByRole("navigation", { name: "Mobile" });
  await expect(mobileNav).toBeVisible();

  for (const id of SECTION_IDS) {
    await expect(mobileNav.locator(`a[href="#${id}"]`), `mobile link to #${id}`).toBeVisible();
  }
  await expect(mobileNav.getByRole("link", { name: "Get it on GitHub" })).toBeVisible();

  // The panel is absolutely positioned across the header; an off-by-one in its
  // insets would push the document sideways only while the menu is open.
  const panel = await mobileNav.boundingBox();
  expect(panel).not.toBeNull();
  expect(panel?.x).toBeGreaterThanOrEqual(0);
  expect((panel?.x ?? 0) + (panel?.width ?? 0)).toBeLessThanOrEqual(PHONE.width);

  const { scrollWidth, clientWidth } = await documentWidths(page);
  expect(scrollWidth).toBeLessThanOrEqual(clientWidth);
});

test("the mobile menu is reachable from the keyboard", async ({ page }) => {
  await gotoLanding(page);

  // Tab past the skip link, the logo and the theme toggle; the disclosure has to
  // be in the tab order, not a click-only control.
  const summaryFocused = () =>
    page.evaluate(() => {
      const active = document.activeElement;
      return active instanceof HTMLElement && active.matches("#mobile-menu > summary");
    });

  for (let press = 0; press < 8 && !(await summaryFocused()); press += 1) {
    await page.keyboard.press("Tab");
  }
  expect(await summaryFocused(), "the menu button never took focus").toBe(true);

  await page.keyboard.press("Enter");
  await expect(page.getByRole("navigation", { name: "Mobile" })).toBeVisible();
});

test("tapping a mobile link scrolls to the section and closes the menu", async ({ page }) => {
  await gotoLanding(page);
  await page.locator("#mobile-menu summary").click();

  await page.getByRole("navigation", { name: "Mobile" }).locator('a[href="#audit"]').click();

  await expect(page).toHaveURL(/#audit$/);
  // An in-page anchor does not reload the document, so a menu left open would sit
  // on top of the section it just scrolled to.
  await expect(page.locator("#mobile-menu")).not.toHaveAttribute("open");
  await expect(page.getByRole("navigation", { name: "Mobile" })).toBeHidden();
  await expectAnchorLanded(page, "audit");
});

for (const width of [320, 360, 414]) {
  test(`does not scroll sideways at ${width}px`, async ({ page }) => {
    await page.setViewportSize({ width, height: PHONE.height });
    await gotoLanding(page);
    await page.locator("#faq").scrollIntoViewIfNeeded();
    await page.waitForLoadState("networkidle");

    const { scrollWidth, clientWidth } = await documentWidths(page);
    expect(
      scrollWidth,
      `the document is ${scrollWidth}px wide inside a ${clientWidth}px viewport`,
    ).toBeLessThanOrEqual(clientWidth);
  });
}
