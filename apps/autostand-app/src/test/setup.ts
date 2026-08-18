import "@testing-library/jest-dom/vitest";
import { cleanup } from "@testing-library/react";
import { afterEach, vi } from "vitest";

// `lib/tauri.ts` imports these at module scope, so anything that reaches it —
// which is nearly every component — needs them stubbed. Without a Tauri host
// they throw when called, and an unhandled rejection inside an effect fails the
// run even when every assertion passed. Mocked here rather than in each test
// file: there is nothing per-test to vary, and 27 copies of the same three
// lines is 27 places to forget one.
vi.mock("@tauri-apps/api/app", () => ({
  getVersion: () => Promise.resolve("0.0.0-test"),
}));
vi.mock("@tauri-apps/plugin-updater", () => ({
  check: () => Promise.resolve(null),
}));
vi.mock("@tauri-apps/plugin-process", () => ({
  relaunch: () => Promise.resolve(),
}));

// Radix Select (and other popovers) call these on open; jsdom does not implement them.
HTMLElement.prototype.scrollIntoView = vi.fn();
HTMLElement.prototype.hasPointerCapture = vi.fn(() => false);
HTMLElement.prototype.setPointerCapture = vi.fn();
HTMLElement.prototype.releasePointerCapture = vi.fn();

afterEach(() => {
  cleanup();
});
