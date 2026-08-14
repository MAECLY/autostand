/**
 * Copy-to-clipboard hook with a transient "copied" flag.
 *
 * Prefers the Tauri clipboard plugin (works in the desktop webview); falls back
 * to `navigator.clipboard` so the hook also works in browser dev / vitest.
 */

import { useCallback, useEffect, useRef, useState } from "react";

async function writeText(text: string): Promise<void> {
  // Dynamic import: `@tauri-apps/plugin-clipboard-manager` is only present in
  // the Tauri webview. In vitest (jsdom) the import throws, so fall back.
  try {
    const mod = await import("@tauri-apps/plugin-clipboard-manager");
    await mod.writeText(text);
    return;
  } catch {
    // Browser / test environment.
    if (typeof navigator !== "undefined" && navigator.clipboard) {
      await navigator.clipboard.writeText(text);
      return;
    }
    throw new Error("Clipboard unavailable");
  }
}

export interface UseClipboardResult {
  copy: (text: string) => Promise<void>;
  /** True for `resetMs` after a successful copy, then flips back to false. */
  copied: boolean;
}

export function useClipboard(resetMs = 2000): UseClipboardResult {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const copy = useCallback(
    async (text: string) => {
      await writeText(text);
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), resetMs);
    },
    [resetMs],
  );

  useEffect(() => {
    return () => {
      if (timer.current) clearTimeout(timer.current);
    };
  }, []);

  return { copy, copied };
}