/**
 * Shared constants and helpers for the landing-page E2E suite.
 *
 * Everything here is deliberately literal rather than imported from
 * `astro.config.mjs` or the components: the point of these tests is to catch a
 * change in those files, so re-deriving the expected values from them would make
 * the suite agree with any regression.
 */
import { expect, type Page } from "@playwright/test";

/**
 * Where GitHub Pages serves the site. Mirrors `base` in `astro.config.mjs`, with
 * the trailing slash a static server adds. Hardcoded on purpose — if `base`
 * changes, every navigation in this suite must fail loudly.
 */
export const BASE_PATH = "/autostand/";

/** The key `Base.astro`'s pre-paint script and `ThemeToggle.tsx` must agree on. */
export const THEME_STORAGE_KEY = "autostand-theme";

/** Every in-page target the navbar links to, in document order. */
export const SECTION_IDS = ["features", "how-it-works", "audit", "faq"] as const;

/**
 * `scroll-pt-18` on `<html>` in `Base.astro`: 18 × 0.25rem = 72px, chosen to clear
 * the 65px sticky header. An anchored section should land exactly here.
 */
export const SCROLL_PADDING_TOP = 72;

/** Rounding slack for a settled scroll position, in CSS pixels. */
const SCROLL_TOLERANCE = 2;

/**
 * Load the landing page at its deployed path.
 *
 * `domcontentloaded` rather than `load`: several assertions care about the state
 * the document is in before islands hydrate, and waiting for `load` would race
 * them. Callers that need an island wait for it explicitly with {@link hydrated}.
 */
export async function gotoLanding(page: Page): Promise<void> {
  await page.goto(BASE_PATH, { waitUntil: "domcontentloaded" });
}

/**
 * Wait until an Astro island has hydrated.
 *
 * Astro renders `<astro-island ... ssr>` and drops the `ssr` attribute once the
 * component is live. Before that point the markup is real but inert, so clicking
 * it silently does nothing — which is a flaky test, not a failing one.
 */
export async function hydrated(page: Page, componentExport: string): Promise<void> {
  const island = page.locator(`astro-island[component-export="${componentExport}"]`);
  await expect(island).toHaveCount(1);
  await expect(island).not.toHaveAttribute("ssr");
}

export interface AnchorPosition {
  /** Distance from the top of the viewport to the top of the section, in px. */
  readonly sectionTop: number;
  /** Bottom edge of the sticky header, in the same coordinate space. */
  readonly headerBottom: number;
}

/** Measure an anchored section against the sticky header currently over it. */
export async function measureAnchor(page: Page, id: string): Promise<AnchorPosition> {
  return page.evaluate((sectionId) => {
    const section = document.getElementById(sectionId);
    if (!section) throw new Error(`no element with id "${sectionId}"`);
    const header = document.querySelector("body > header");
    if (!header) throw new Error("no sticky header");
    return {
      sectionTop: section.getBoundingClientRect().top,
      headerBottom: header.getBoundingClientRect().bottom,
    };
  }, id);
}

/**
 * Assert an anchored section came to rest just under the sticky header rather
 * than behind it — the failure mode `scroll-pt-18` exists to prevent.
 *
 * Polled rather than sampled once: `<html>` carries `scroll-smooth`, Chromium
 * animates the jump even under `prefers-reduced-motion: reduce`, and the
 * animation may not have started by the time the click returns. Waiting for the
 * position to stop changing is not enough for the same reason — "not moving yet"
 * and "finished moving" look identical. Polling the destination is the only
 * reading that cannot be taken too early.
 */
export async function expectAnchorLanded(page: Page, id: string): Promise<void> {
  await expect
    .poll(
      async () => {
        const { sectionTop, headerBottom } = await measureAnchor(page, id);
        const clearsHeader = sectionTop >= headerBottom - SCROLL_TOLERANCE;
        const reachedPadding = sectionTop <= SCROLL_PADDING_TOP + SCROLL_TOLERANCE;
        return clearsHeader && reachedPadding
          ? "landed"
          : `sectionTop=${Math.round(sectionTop)} headerBottom=${Math.round(headerBottom)}`;
      },
      {
        timeout: 5_000,
        message: `#${id} never came to rest between the sticky header and ${SCROLL_PADDING_TOP}px`,
      },
    )
    .toBe("landed");
}

/** Assert the document scrolled back to its very top, once the jump finishes. */
export async function expectScrolledToTop(page: Page): Promise<void> {
  await expect
    .poll(() => page.evaluate(() => Math.round(window.scrollY)), {
      timeout: 5_000,
      message: "the document never returned to the top",
    })
    .toBe(0);
}

/**
 * Jump to the bottom without animating. `scroll-smooth` applies to programmatic
 * scrolling too, and a layout assertion should not have to outwait it.
 */
export async function scrollToBottom(page: Page): Promise<void> {
  await page.evaluate(() => {
    window.scrollTo({ top: document.body.scrollHeight, behavior: "instant" });
  });
}

export interface DocumentWidths {
  readonly scrollWidth: number;
  readonly clientWidth: number;
}

/**
 * Widths of the scrolling element.
 *
 * Compared against each other rather than against the viewport, so a platform
 * that reserves space for a classic scrollbar (Linux CI) and one that overlays it
 * (macOS) both give the same verdict.
 */
export async function documentWidths(page: Page): Promise<DocumentWidths> {
  return page.evaluate(() => ({
    scrollWidth: document.documentElement.scrollWidth,
    clientWidth: document.documentElement.clientWidth,
  }));
}
