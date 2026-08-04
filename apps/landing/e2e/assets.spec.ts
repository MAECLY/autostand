/**
 * Every asset the page asks for is really there, under the deployed base path.
 *
 * This is the regression a static-site base path fails at first: `astro build`
 * writes root-absolute URLs like `/autostand/brand/logo-mark.svg`, and the moment
 * `base` moves — or a component concatenates `BASE_URL` without normalising the
 * trailing slash and emits `/autostandbrand/…` — the markup still renders and the
 * page still scores 200. Only the sub-resources break.
 *
 * Asserted on the response status of the actual resource rather than on
 * `naturalWidth` alone: some static servers answer a missing path with the SPA
 * fallback HTML at 200, which decodes to a zero-width image, and some answer 404.
 * Both are checked so neither shape can slip through.
 */
import { expect, test, type Response } from "@playwright/test";

import { BASE_PATH, gotoLanding } from "./fixtures";

/** Same-origin sub-resources the browser fetched while loading the page. */
interface RecordedResponse {
  readonly url: string;
  readonly status: number;
  readonly pathname: string;
}

test("loads the page without a single failed request", async ({ page }) => {
  const recorded: RecordedResponse[] = [];
  const record = (response: Response) => {
    recorded.push({
      url: response.url(),
      status: response.status(),
      pathname: new URL(response.url()).pathname,
    });
  };

  page.on("response", record);
  await gotoLanding(page);
  // The FAQ island only loads its bundle once the section is on screen, so the
  // below-the-fold JavaScript has to be pulled in before the tally means anything.
  await page.locator("#faq").scrollIntoViewIfNeeded();
  await page.waitForLoadState("networkidle");
  page.off("response", record);

  const failed = recorded.filter((entry) => entry.status >= 400);
  expect(failed, `failed requests:\n${failed.map((f) => `  ${f.status} ${f.url}`).join("\n")}`)
    .toEqual([]);

  // Everything the page pulls is same-origin: the fonts are self-hosted and the
  // logos are inlined or served from public/. A request off this origin means the
  // page grew a third-party dependency, which a local-first landing page must not.
  const offOrigin = recorded.filter((entry) => !entry.url.startsWith("http://127.0.0.1"));
  expect(offOrigin.map((entry) => entry.url)).toEqual([]);

  // Every same-origin path lives under the base. This is the base-path assertion.
  const offBase = recorded.filter((entry) => !entry.pathname.startsWith(BASE_PATH));
  expect(offBase.map((entry) => entry.pathname)).toEqual([]);

  // Sanity: the page did fetch its CSS and its islands, so the checks above were
  // not vacuously true because nothing loaded.
  expect(recorded.filter((entry) => entry.pathname.endsWith(".css")).length).toBeGreaterThan(0);
  expect(recorded.filter((entry) => entry.pathname.endsWith(".js")).length).toBeGreaterThan(0);
});

test("every image resolves to a real image", async ({ page }) => {
  await gotoLanding(page);
  await page.waitForLoadState("networkidle");

  const images = await page.locator("img").evaluateAll((elements) =>
    elements.map((element) => {
      const image = element as HTMLImageElement;
      return {
        src: image.getAttribute("src") ?? "",
        resolved: image.currentSrc || image.src,
        naturalWidth: image.naturalWidth,
        alt: image.getAttribute("alt"),
      };
    }),
  );

  expect(images.length, "the page should still ship at least one <img>").toBeGreaterThan(0);

  for (const image of images) {
    expect(image.src, `src of ${image.src}`).toMatch(new RegExp(`^${BASE_PATH}`));
    // Decoded by the browser: catches an SVG that parses as XML but renders nothing.
    expect(image.naturalWidth, `${image.src} decoded`).toBeGreaterThan(0);
    // `alt` must be present — empty is correct for a decorative mark, missing is not.
    expect(image.alt, `alt of ${image.src}`).not.toBeNull();

    const response = await page.request.get(image.resolved, { failOnStatusCode: false });
    expect(response.status(), `status of ${image.resolved}`).toBe(200);
    expect(
      response.headers()["content-type"] ?? "",
      `content-type of ${image.resolved}`,
    ).toMatch(/^image\//);
  }
});

test("the favicon and the social card ship with the build", async ({ page }) => {
  await gotoLanding(page);

  const favicon = await page.locator('link[rel="icon"]').getAttribute("href");
  expect(favicon).toMatch(new RegExp(`^${BASE_PATH}`));
  const faviconResponse = await page.request.get(String(favicon), { failOnStatusCode: false });
  expect(faviconResponse.status(), `favicon ${favicon}`).toBe(200);
  expect(faviconResponse.headers()["content-type"] ?? "").toMatch(/^image\//);

  // og:image is absolute because social scrapers do not resolve relative URLs.
  // Its host is fetched by nobody here — the suite stays hermetic — but the path
  // it promises has to exist in the artifact we just built.
  const ogImage = await page.locator('meta[property="og:image"]').getAttribute("content");
  expect(ogImage).toBeTruthy();
  const ogPath = new URL(String(ogImage)).pathname;
  expect(ogPath).toMatch(new RegExp(`^${BASE_PATH}`));

  const ogResponse = await page.request.get(ogPath, { failOnStatusCode: false });
  expect(ogResponse.status(), `og:image ${ogPath}`).toBe(200);
  expect(ogResponse.headers()["content-type"] ?? "").toMatch(/^image\//);
});

test("a missing asset under the base path is a 404, not a 200", async ({ page }) => {
  // Guards the guard: if the preview server answered every unknown path with the
  // index document, the status assertions above would pass on a broken build.
  const response = await page.request.get(`${BASE_PATH}brand/does-not-exist.svg`, {
    failOnStatusCode: false,
  });
  expect(response.status()).toBe(404);
});
