/**
 * The theme survives a reload, and the two things that set it agree.
 *
 * There are two independent implementations of "what theme is this": the inline
 * script in `Base.astro`, which runs before first paint and stamps `.dark` on
 * `<html>`, and `ThemeToggle.tsx`, which reads that class on mount and writes
 * `localStorage` on click. They share nothing but a string key. If they drift,
 * the page renders in one theme and the button claims the other — a bug no unit
 * test can see, because neither half is wrong on its own.
 */
import { expect, test, type Page } from "@playwright/test";

import { gotoLanding, hydrated, THEME_STORAGE_KEY } from "./fixtures";

const themeToggle = (page: Page) =>
  page.getByRole("button", { name: /^Switch to (light|dark) theme$/ });

const isDark = (page: Page) =>
  page.evaluate(() => document.documentElement.classList.contains("dark"));

const storedTheme = (page: Page) =>
  page.evaluate((key) => localStorage.getItem(key), THEME_STORAGE_KEY);

test("follows the system preference when nothing is stored", async ({ page }) => {
  await gotoLanding(page);

  // colorScheme is pinned to light in playwright.config.ts.
  expect(await isDark(page)).toBe(false);
  expect(await storedTheme(page)).toBeNull();

  await hydrated(page, "ThemeToggle");
  await expect(themeToggle(page)).toHaveAttribute("aria-pressed", "false");
  await expect(themeToggle(page)).toHaveAccessibleName("Switch to dark theme");
});

test.describe("with a dark system preference", () => {
  test.use({ colorScheme: "dark" });

  test("paints dark before the toggle hydrates", async ({ page }) => {
    await gotoLanding(page);

    // Asserted at DOMContentLoaded: the only code that has run by then is the
    // inline head script, so this is the no-flash guarantee, not React catching up.
    expect(await isDark(page)).toBe(true);
    expect(await storedTheme(page)).toBeNull();

    await hydrated(page, "ThemeToggle");
    await expect(themeToggle(page)).toHaveAttribute("aria-pressed", "true");
    await expect(themeToggle(page)).toHaveAccessibleName("Switch to light theme");
  });

  test("a stored preference beats the system preference", async ({ page }) => {
    await page.addInitScript(
      ([key, value]) => localStorage.setItem(key ?? "", value ?? ""),
      [THEME_STORAGE_KEY, "light"] as const,
    );
    await gotoLanding(page);

    expect(await isDark(page)).toBe(false);
    await hydrated(page, "ThemeToggle");
    await expect(themeToggle(page)).toHaveAttribute("aria-pressed", "false");
  });
});

test("toggling flips the class, persists it, and survives a reload", async ({ page }) => {
  await gotoLanding(page);
  await hydrated(page, "ThemeToggle");

  await themeToggle(page).click();
  expect(await isDark(page)).toBe(true);
  expect(await storedTheme(page)).toBe("dark");
  await expect(themeToggle(page)).toHaveAttribute("aria-pressed", "true");

  await page.reload({ waitUntil: "domcontentloaded" });
  // The pre-paint script read what the toggle wrote…
  expect(await isDark(page)).toBe(true);
  await hydrated(page, "ThemeToggle");
  // …and the toggle read back what the pre-paint script applied.
  await expect(themeToggle(page)).toHaveAttribute("aria-pressed", "true");

  await themeToggle(page).click();
  expect(await isDark(page)).toBe(false);
  expect(await storedTheme(page)).toBe("light");

  await page.reload({ waitUntil: "domcontentloaded" });
  expect(await isDark(page)).toBe(false);
  await hydrated(page, "ThemeToggle");
  await expect(themeToggle(page)).toHaveAttribute("aria-pressed", "false");
});

test("writes the theme under the key the pre-paint script reads", async ({ page }) => {
  await gotoLanding(page);
  await hydrated(page, "ThemeToggle");
  await themeToggle(page).click();

  // Not a paraphrase of the assertions above: this pins the wire format of the
  // one value the two implementations exchange. Renaming the key, or storing a
  // JSON blob instead of the bare word, breaks the reload path silently.
  const everything = await page.evaluate(() =>
    Object.fromEntries(
      Object.keys(localStorage).map((key) => [key, localStorage.getItem(key)] as const),
    ),
  );
  expect(everything).toEqual({ [THEME_STORAGE_KEY]: "dark" });
});

test("repaints the page, not just the class", async ({ page }) => {
  await gotoLanding(page);
  await hydrated(page, "ThemeToggle");

  const bodyBackground = () =>
    page.evaluate(() => getComputedStyle(document.body).backgroundColor);

  const light = await bodyBackground();
  await themeToggle(page).click();
  const dark = await bodyBackground();

  // The `.dark` class is only useful if the tokens under it actually resolve —
  // a missing `.dark` block in tokens.css leaves the class on and the page white.
  expect(dark).not.toBe(light);
});
