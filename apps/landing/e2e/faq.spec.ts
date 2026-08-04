/**
 * The FAQ accordion opens, closes, and says so out loud.
 *
 * `Faq` is the one `client:visible` island on the page: its markup is rendered at
 * build time and is inert until the section scrolls into view. That makes "the
 * panel opens" a genuinely browser-only assertion — the DOM looks identical
 * before and after hydration, and a broken directive shows up as a heading that
 * does nothing when you click it.
 */
import { expect, test, type Locator, type Page } from "@playwright/test";

import { gotoLanding, hydrated } from "./fixtures";

const FIRST_QUESTION = "Which AI providers can write my standup?";
const SECOND_QUESTION = "Do I need a subscription or an API key?";
const SECOND_ANSWER = /One of the two, not both/;

const trigger = (page: Page, question: string): Locator =>
  page.locator("#faq").getByRole("button", { name: question });

test.beforeEach(async ({ page }) => {
  await gotoLanding(page);
  await page.locator("#faq").scrollIntoViewIfNeeded();
  await hydrated(page, "Faq");
});

test("opens with the first answer already readable", async ({ page }) => {
  // Radix drops closed panels from the DOM, so one answer is open in the static
  // HTML: something a crawler can index and something to read before hydration.
  await expect(trigger(page, FIRST_QUESTION)).toHaveAttribute("aria-expanded", "true");
  await expect(trigger(page, SECOND_QUESTION)).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator("#faq").getByText(SECOND_ANSWER)).toHaveCount(0);
});

test("opening a question exposes its answer and closes the previous one", async ({ page }) => {
  await trigger(page, SECOND_QUESTION).click();

  await expect(trigger(page, SECOND_QUESTION)).toHaveAttribute("aria-expanded", "true");
  await expect(page.locator("#faq").getByText(SECOND_ANSWER)).toBeVisible();
  // `type="single"`: opening one answer has to collapse the other, or the
  // accordion is just a list of paragraphs with extra steps.
  await expect(trigger(page, FIRST_QUESTION)).toHaveAttribute("aria-expanded", "false");
});

test("clicking the open question closes it again", async ({ page }) => {
  await trigger(page, SECOND_QUESTION).click();
  await expect(page.locator("#faq").getByText(SECOND_ANSWER)).toBeVisible();

  await trigger(page, SECOND_QUESTION).click();

  await expect(trigger(page, SECOND_QUESTION)).toHaveAttribute("aria-expanded", "false");
  await expect(page.locator("#faq").getByText(SECOND_ANSWER)).toHaveCount(0);
  // `collapsible`: every panel may be shut at once.
  await expect(page.locator("#faq button[aria-expanded='true']")).toHaveCount(0);
});

test("the keyboard works the accordion the same way the mouse does", async ({ page }) => {
  const second = trigger(page, SECOND_QUESTION);
  await second.focus();
  await page.keyboard.press("Enter");
  await expect(second).toHaveAttribute("aria-expanded", "true");

  await page.keyboard.press("Space");
  await expect(second).toHaveAttribute("aria-expanded", "false");
});

test("each answer is announced as a region owned by its question", async ({ page }) => {
  const first = trigger(page, FIRST_QUESTION);

  const controls = await first.getAttribute("aria-controls");
  const triggerId = await first.getAttribute("id");
  expect(controls, "trigger must point at its panel").toBeTruthy();
  expect(triggerId, "trigger must be referenceable").toBeTruthy();

  // Attribute selector rather than `#id`: Radix generates ids containing
  // characters a CSS id selector would have to escape.
  const panel = page.locator(`[id="${controls}"]`);
  await expect(panel).toHaveAttribute("role", "region");
  // The panel names itself with the question, so a screen reader landing inside
  // it knows what is being answered.
  await expect(panel).toHaveAttribute("aria-labelledby", String(triggerId));

  // The trigger is wrapped in a real heading, so the FAQ is navigable by heading
  // list and not just by tabbing through eight anonymous buttons.
  expect(await first.evaluate((element) => element.parentElement?.tagName)).toBe("H3");
});

test("every question toggles independently", async ({ page }) => {
  const triggers = page.locator("#faq button[aria-expanded]");
  // One per entry in ENTRIES (Faq.tsx). A dropped question is a content
  // regression worth failing on.
  await expect(triggers).toHaveCount(8);

  const count = await triggers.count();
  for (let index = 0; index < count; index += 1) {
    const button = triggers.nth(index);
    const before = await button.getAttribute("aria-expanded");
    await button.click();
    await expect(button).toHaveAttribute("aria-expanded", before === "true" ? "false" : "true");
  }
});
