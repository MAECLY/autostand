/**
 * Regression: the dashboard grid must never blow out the viewport.
 *
 * The fix in `__root.tsx:23` added `min-h-0 overflow-hidden` to the right
 * column so the grid item cannot grow beyond the viewport in WebKit, and
 * `<main>` stays the single scroll container. This spec pins that invariant
 * at four viewport widths so a future change that re-introduces a
 * horizontal scrollbar or an unbounded right column fails loudly.
 */

import { expect, test } from "./support/fixtures";
import { makeScenario, TODAY_LABEL } from "./support/scenario";

const VIEWPORTS = [
  { width: 640, height: 480, label: "mobile" },
  { width: 1024, height: 768, label: "tablet" },
  { width: 1440, height: 900, label: "desktop" },
  { width: 1920, height: 1080, label: "wide" },
];

for (const { width, height, label } of VIEWPORTS) {
  test(`regression_dashboard_overflow — no horizontal scrollbar at ${label} (${width}x${height})`, async ({
    page,
    app,
  }) => {
    await page.setViewportSize({ width, height });
    await app.start(makeScenario());

    await expect(
      page.getByRole("heading", { name: `Today — ${TODAY_LABEL}` }),
    ).toBeVisible();

    const overflow = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      innerWidth: window.innerWidth,
      mainScrollHeight: document.querySelector("main")?.scrollHeight ?? 0,
      mainClientHeight: document.querySelector("main")?.clientHeight ?? 0,
    }));

    expect(overflow.scrollWidth, "no horizontal page overflow").toBeLessThanOrEqual(
      overflow.innerWidth,
    );
    expect(
      overflow.mainScrollHeight,
      "main is the scroll container (scrollHeight > clientHeight)",
    ).toBeGreaterThan(0);
  });
}

for (const { width, height, label } of VIEWPORTS) {
  test(`regression_history_overflow — no horizontal scrollbar at ${label} (${width}x${height})`, async ({
    page,
    app,
  }) => {
    await page.setViewportSize({ width, height });
    await app.start(makeScenario(), "/history");

    await expect(page.getByRole("tablist", { name: "History view" })).toBeVisible();

    const overflow = await page.evaluate(() => ({
      scrollWidth: document.documentElement.scrollWidth,
      innerWidth: window.innerWidth,
    }));

    expect(overflow.scrollWidth, "no horizontal page overflow").toBeLessThanOrEqual(
      overflow.innerWidth,
    );
  });
}

test("regression_dashboard_overflow — right column never exceeds viewport height", async ({
  page,
  app,
}) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  await app.start(makeScenario());

  const rightColumn = page.locator("main > div > div:last-child");
  await expect(rightColumn).toBeVisible();

  const bounds = await rightColumn.boundingBox();
  expect(bounds).not.toBeNull();
  expect(bounds!.y + bounds!.height, "right column bottom within viewport").toBeLessThanOrEqual(
    900,
  );
});