/** Pure helpers shared across the app UI. */

import { clsx, type ClassValue } from "clsx";
import { format, formatDistanceToNow, isValid, parseISO } from "date-fns";
import { twMerge } from "tailwind-merge";

import type { StandupFileContent } from "@/lib/types";

/** Merge conditional class names, resolving conflicting Tailwind utilities. */
export function cn(...inputs: ClassValue[]): string {
  return twMerge(clsx(inputs));
}

/** Backend filing-date format (`YYYY-MM-DD`). */
export const ISO_DATE_FORMAT = "yyyy-MM-dd";

const DISPLAY_DATE_FORMAT = "MMM d, yyyy";

/**
 * Format an ISO-8601 date or timestamp for display.
 * Unparseable input is returned verbatim so the UI never renders "Invalid Date".
 */
export function formatIsoDate(iso: string, pattern = DISPLAY_DATE_FORMAT): string {
  const parsed = parseISO(iso);
  return isValid(parsed) ? format(parsed, pattern) : iso;
}

/** Human-readable distance from now, e.g. "about 2 hours ago". */
export function formatRelative(iso: string): string {
  const parsed = parseISO(iso);
  return isValid(parsed)
    ? formatDistanceToNow(parsed, { addSuffix: true })
    : iso;
}

/** Today's filing date in the backend's `YYYY-MM-DD` format (local time). */
export function todayIso(): string {
  return format(new Date(), ISO_DATE_FORMAT);
}

/**
 * Token classes cycled for host chips. Drawn from the audit palette because
 * those are the only multi-hue tokens the design system exposes.
 */
const HOST_COLORS = [
  "text-audit-commit",
  "text-audit-github",
  "text-audit-review",
  "text-audit-note",
  "text-audit-phantom",
] as const;

/** Stable per-host token class, so the same slug always gets the same hue. */
export function hostColor(host: string): string {
  if (host.length === 0) return "text-audit-unverified";
  let hash = 0;
  for (let i = 0; i < host.length; i += 1) {
    // `>>> 0` keeps the accumulator an unsigned 32-bit int across platforms.
    hash = (hash * 31 + host.charCodeAt(i)) >>> 0;
  }
  return HOST_COLORS[hash % HOST_COLORS.length];
}

/**
 * Reconstruct the markdown source of a standup file from its parsed content,
 * so the clipboard gets the same text that lives on disk (minus HTML comments
 * the renderer strips for display).
 */
export function serializeStandup(content: StandupFileContent): string {
  const parts: string[] = [`# ${content.title}`, "", content.subtitle, ""];

  for (const block of content.auto_blocks) {
    const body = stripHtmlComments(block.body);
    if (body.length === 0) continue;
    parts.push(`<!-- AUTO:${block.host}:START -->`, body, `<!-- AUTO:${block.host}:END -->`, "");
  }

  const manual = stripHtmlComments(content.manual_region);
  if (manual.length > 0) {
    parts.push("<!-- MANUAL:START -->", manual, "<!-- MANUAL:END -->", "");
  }

  return parts.join("\n").trim() + "\n";
}

const HTML_COMMENT_LINE = /^\s*<!--.*-->\s*$/;

export function stripHtmlComments(body: string): string {
  return body
    .split("\n")
    .filter((line) => !HTML_COMMENT_LINE.test(line))
    .join("\n")
    .trim();
}
