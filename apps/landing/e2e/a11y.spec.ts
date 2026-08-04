/**
 * Keyboard access and an axe-core audit of the rendered page.
 *
 * axe runs against the live accessibility tree, so it sees what a component test
 * cannot: computed contrast against whatever surface an element actually landed
 * on, the roles Radix produced after hydration, and the scroll containers the
 * layout created. It is run in both themes and at phone width because all three
 * produce different trees.
 *
 * The gate is critical + serious. Those are the impacts axe reserves for
 * "a person using assistive technology cannot do this", and a public marketing
 * page has no excuse for them.
 */
import { AxeBuilder } from "@axe-core/playwright";
import { expect, test, type Page } from "@playwright/test";

import { gotoLanding, hydrated } from "./fixtures";

/**
 * WCAG 2.0/2.1 level A and AA — the conformance target, and a stable rule set.
 * `best-practice` is deliberately excluded: it moves between axe releases, and a
 * dependency bump should not turn CI red on a rule nobody agreed to.
 */
const WCAG_TAGS = ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa"] as const;

const BLOCKING_IMPACTS = new Set(["critical", "serious"]);

type Violations = Awaited<ReturnType<AxeBuilder["analyze"]>>["violations"];

/**
 * Put the page in the state a visitor sees after reading it: both islands live,
 * every section laid out. `client:visible` content that was never scrolled to
 * would otherwise be audited as inert markup.
 */
async function settle(page: Page): Promise<void> {
  await hydrated(page, "ThemeToggle");
  await page.locator("#faq").scrollIntoViewIfNeeded();
  await hydrated(page, "Faq");
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.waitForLoadState("networkidle");
}

async function auditPage(page: Page): Promise<Violations> {
  const results = await new AxeBuilder({ page }).withTags([...WCAG_TAGS]).analyze();
  return results.violations.filter((violation) => BLOCKING_IMPACTS.has(violation.impact ?? ""));
}

/** One readable block per violation, with the selector and the measured reason. */
function describe(violations: Violations): string {
  return violations
    .map((violation) => {
      const nodes = violation.nodes
        .map((node) => {
          const reasons = [...node.any, ...node.all, ...node.none]
            .map((check) => check.message.replace(/\s+/g, " "))
            .join("; ");
          return `      ${node.target.join(" ")}\n        ${reasons}`;
        })
        .join("\n");
      return `  [${violation.impact}] ${violation.id} — ${violation.help}\n${nodes}`;
    })
    .join("\n");
}

async function expectNoBlockingViolations(page: Page): Promise<void> {
  const violations = await auditPage(page);
  expect(
    violations.map((violation) => `${violation.impact}: ${violation.id}`),
    `axe found blocking violations:\n${describe(violations)}`,
  ).toEqual([]);
}

test("the skip link is the first thing a keyboard reaches", async ({ page }) => {
  await gotoLanding(page);

  await page.keyboard.press("Tab");

  const skipLink = page.getByRole("link", { name: "Skip to content" });
  await expect(skipLink).toBeFocused();
  await expect(skipLink).toHaveAttribute("href", "#main");
  // Parked off-screen until focused, then it must actually be on screen — a skip
  // link a sighted keyboard user cannot see is a skip link they cannot use.
  await expect(skipLink).toBeInViewport();
});

test("the skip link moves the keyboard past the navigation", async ({ page }) => {
  await gotoLanding(page);

  await page.keyboard.press("Tab");
  await page.keyboard.press("Enter");
  await expect(page).toHaveURL(/#main$/);

  // `<main>` has no tabindex, so activating the fragment moves the sequential
  // focus navigation starting point rather than focus itself. The next Tab is
  // what proves the nav was skipped. See the note in the suite README on giving
  // `<main id="main">` a `tabindex="-1"`, which would also move focus outright.
  await page.keyboard.press("Tab");
  const landedInsideMain = await page.evaluate(() => {
    const active = document.activeElement;
    return active instanceof HTMLElement && active.closest("main") !== null;
  });
  expect(landedInsideMain, "the first Tab after the skip link stayed outside <main>").toBe(true);
});

test("no critical or serious axe violations in the light theme", async ({ page }) => {
  await gotoLanding(page);
  await settle(page);
  await expectNoBlockingViolations(page);
});

test("no critical or serious axe violations in the dark theme", async ({ page }) => {
  await gotoLanding(page);
  await settle(page);

  await page.getByRole("button", { name: "Switch to dark theme" }).click();
  await expect(page.locator("html")).toHaveClass(/\bdark\b/);

  await expectNoBlockingViolations(page);
});

test("no critical or serious axe violations at phone width", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoLanding(page);
  await settle(page);
  await expectNoBlockingViolations(page);
});

test("no critical or serious axe violations with the mobile menu open", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await gotoLanding(page);
  await settle(page);

  await page.locator("#mobile-menu summary").click();
  await expect(page.getByRole("navigation", { name: "Mobile" })).toBeVisible();

  await expectNoBlockingViolations(page);
});
