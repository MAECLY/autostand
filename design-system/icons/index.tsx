/**
 * Custom icons for concepts lucide does not cover (see `docs/design-system/02-brand.md`).
 *
 * The `.svg` files next to this module are the source of truth — the JSX below is a
 * literal transcription of them. Edit the SVG first, then mirror the change here, so
 * the file a designer opens and the component an app renders never drift apart.
 *
 * @packageDocumentation
 */

import * as React from "react";

/** Props of every icon in the set — a lucide-compatible signature, so they drop in. */
export interface IconProps extends React.SVGProps<SVGSVGElement> {
  /** Edge length of the rendered square. Matches lucide's `size` prop. */
  size?: number | string;
}

/**
 * lucide's own root attributes. Sharing them is what makes a custom icon sit next to
 * `<GitBranch />` without looking foreign: same 24 grid, same 2px stroke, same joins.
 */
const svgAttributes = {
  xmlns: "http://www.w3.org/2000/svg",
  viewBox: "0 0 24 24",
  fill: "none",
  stroke: "currentColor",
  strokeWidth: 2,
  strokeLinecap: "round",
  strokeLinejoin: "round",
} as const;

/**
 * A standup `.md` file: a card with a date header and body lines.
 * Echoes the card in the brand mark, so the app icon and the file icon read as a set.
 */
export function StandupFileIcon({ size = 24, ...props }: IconProps) {
  return (
    <svg {...svgAttributes} width={size} height={size} aria-hidden="true" {...props}>
      <rect x="4" y="3" width="16" height="18" rx="2" />
      <path d="M4 8h16" />
      <path d="M8 12h8" />
      <path d="M8 16h5" />
    </svg>
  );
}

/**
 * The compile pipeline: gather, render, write — three stages wired in order.
 * Staggered rather than in a row: three nodes on one axis collapse into an
 * ellipsis at 16px, and the rising diagonal also carries the direction of flow.
 */
export function PipelineIcon({ size = 24, ...props }: IconProps) {
  return (
    <svg {...svgAttributes} width={size} height={size} aria-hidden="true" {...props}>
      <circle cx="5" cy="18" r="3" />
      <path d="m7.3 16 2.4-2.1" />
      <circle cx="12" cy="12" r="3" />
      <path d="m14.3 10 2.4-2.1" />
      <circle cx="19" cy="6" r="3" />
    </svg>
  );
}

/** A host: a machine with a shell prompt on screen. One AUTO block is written per host. */
export function HostIcon({ size = 24, ...props }: IconProps) {
  return (
    <svg {...svgAttributes} width={size} height={size} aria-hidden="true" {...props}>
      <rect x="2" y="3" width="20" height="14" rx="2" />
      <path d="m6.5 7 3 3-3 3" />
      <path d="M12 13h6" />
      <path d="M12 17v4" />
      <path d="M8 21h8" />
    </svg>
  );
}

/** Audit classification `phantom`: a bullet that claims work with no matching source. */
export function AuditPhantomIcon({ size = 24, ...props }: IconProps) {
  return (
    <svg {...svgAttributes} width={size} height={size} aria-hidden="true" {...props}>
      <path d="M4.5 10a7.5 7.5 0 0 1 15 0v8l-2.5 2-2.5-2-2.5 2-2.5-2-2.5 2-2.5-2Z" />
      <path d="M9.5 10.5h.01" />
      <path d="M14.5 10.5h.01" />
    </svg>
  );
}

/**
 * Audit classification `commit`: a node on the local git history that backs a bullet.
 * The edge leaving the node is the point of the classification — the commit is
 * evidence *for* something — and it also stops the glyph reading as a lone pin.
 */
export function AuditCommitIcon({ size = 24, ...props }: IconProps) {
  return (
    <svg {...svgAttributes} width={size} height={size} aria-hidden="true" {...props}>
      <path d="M6 2v6" />
      <circle cx="6" cy="12" r="4" />
      <path d="M6 16v6" />
      <path d="M10 12h12" />
    </svg>
  );
}

/**
 * Audit classifications `github` / `review`: evidence gathered through the `gh` CLI.
 *
 * The one filled icon in the set. The GitHub mark is a silhouette by convention and
 * outlining it would make it unreadable at 16px, so the exception is deliberate — it
 * still paints with `currentColor`, so it tints and inverts like every other icon.
 *
 * Drawn at 92% of the grid the stroked icons use: solid shapes read heavier than
 * outlines, so matching their bounding box would make this one look oversized.
 */
export function AuditGithubIcon({ size = 24, ...props }: IconProps) {
  return (
    <svg {...svgAttributes} width={size} height={size} aria-hidden="true" {...props}>
      <path
        fill="currentColor"
        stroke="none"
        d="M12 2.34A9.66 9.66 0 0 0 8.95 22.09c.48 .09 .65-.21 .65-.46v-1.78c-2.69 .59-3.26-1.14-3.26-1.14-.44-1.12-1.08-1.42-1.08-1.42-.87-.6 .06-.59 .06-.59 .97 .06 1.48 1 1.48 1 .86 1.47 2.26 1.05 2.81 .8 .09-.63 .34-1.05 .62-1.29-2.14-.25-4.4-1.08-4.4-4.78 0-1.06 .38-1.91 1-2.59-.1-.25-.43-1.23 .09-2.57 0 0 .81-.26 2.66 .99a9.2 9.2 0 0 1 4.84 0c1.85-1.25 2.66-.99 2.66-.99 .52 1.33 .19 2.32 .09 2.57 .63 .68 1 1.54 1 2.59 0 3.71-2.26 4.53-4.42 4.77 .35 .3 .66 .89 .66 1.8v2.67c0 .26 .17 .56 .66 .47A9.66 9.66 0 0 0 12 2.34Z"
      />
    </svg>
  );
}
