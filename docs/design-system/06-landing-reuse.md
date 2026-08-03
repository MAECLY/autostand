# Landing Page Reuse

The design system must be reusable on a marketing landing page — separate from the Tauri app — so branding stays consistent across all surfaces (app, landing, docs, Storybook).

## Goal

A user should see the same colors, typography, and component feel on:
- The Tauri desktop app
- The marketing landing page (web)
- The docs site (web)
- The Storybook (web)

All four consume the same tokens + base components. The Tauri app adds app components + Tauri-specific logic; the web surfaces use base components only.

## Shared assets

These files are shared verbatim across all surfaces:

| Asset | Path | Used by |
|-------|------|---------|
| Design tokens | `design-system/tokens/tokens.css` | App, Storybook, landing, docs |
| Tailwind v4 theme | `design-system/styles/theme.css` (the `@theme` block) | App, Storybook, landing |
| Inter + JetBrains Mono | self-hosted woff2 or Google Fonts | App, landing |
| Logo SVGs | `brand/logo/*.svg` | App, landing, docs, social |
| Custom icons | `design-system/icons/*.svg` | App, landing |
| Base components | `design-system/components/*.tsx` | App, landing (subset) |

## Landing page

The landing page is a separate project (either `apps/landing/` in the monorepo or a standalone repo).

**Framework:** Next.js (static export) or Astro. Both produce static HTML — no server runtime needed for a marketing page.

**Why separate from the Tauri app:**
- The Tauri app is a desktop binary; the landing page is a public website.
- Different audiences (users vs. visitors).
- Different deployment (GitHub Releases vs. GitHub Pages / Vercel / Netlify).

### Setup (Next.js example)

```bash
pnpm create next-app apps/landing --typescript --tailwind --app
```

`apps/landing/src/app/globals.css`:
```css
@import "tailwindcss";
@import "../../design-system/tokens/tokens.css";

@theme {
  /* Same @theme mapping as the app */
  --color-background: var(--bg-base);
  --color-foreground: var(--fg-base);
  --color-primary: var(--brand-primary);
  /* ... */
}
```

This imports the SAME tokens file the app uses. No duplication.

### Landing page sections

| Section | Components used |
|---------|----------------|
| Navbar | `logo-horizontal.svg` in an `<img>`, nav links, `Button` (Download) |
| Hero | Headline, subhead, `Button` (Download) + `Button` (outline, GitHub), app screenshot mockup |
| Features | `Card` × N (3-col grid) with lucide icon + title + description |
| How it works | `Accordion` or step list |
| Audit demo | `AuditViewer` (app component, with mock data) — shows the phantom detection visually |
| Pricing / FAQ | `Card`, `Accordion` |
| Footer | `logo-mono.svg`, links, copyright |

## Component portability

Base components (Button, Card, Badge, Input, Alert, Accordion, etc.) are **pure React + Tailwind** — they work in any React app, not just Tauri.

### Option A: Copy

Copy the needed base components into the landing project:

```bash
cp design-system/components/button.tsx apps/landing/src/components/ui/
cp design-system/components/card.tsx apps/landing/src/components/ui/
cp design-system/components/badge.tsx apps/landing/src/components/ui/
```

Simple, no publishing overhead. Drift risk — keep in sync manually (or via a sync script).

### Option B: Publish as npm package

Publish the design system as `@autostand/ui`:

```bash
# In design-system/
pnpm pack    # produces autostand-ui-0.1.0.tgz
# Or publish to npm (public):
pnpm publish --access public
```

`apps/landing/package.json`:
```json
{
  "dependencies": {
    "@autostand/ui": "^0.1.0"
  }
}
```

```tsx
import { Button, Card, Badge } from "@autostand/ui";
```

No drift — the landing page always uses the latest published version. More overhead (publishing, versioning, semver).

**Recommendation:** start with Option A (copy) for the MVP. Move to Option B when the design system stabilizes and multiple surfaces need it.

### App components on the landing page

App components (`AuditViewer`, `PipelineProgress`, etc.) use Tauri `invoke`. On the landing page, replace `invoke` with mock data:

```tsx
// Landing page version of AuditViewer (no Tauri)
import { AuditViewer } from "@autostand/ui/components/audit-viewer";
import { mockAuditSidecar } from "@autostand/ui/mocks";

export function AuditDemo() {
  return <AuditViewer date="2026-08-03" audit={mockAuditSidecar} />;
}
```

The app component accepts an `audit` prop — it doesn't care if it came from Tauri or a mock. This is why app components take props (not fetch internally) where possible.

## Brand consistency checklist

Before shipping the landing page (or any web surface):

- [ ] Primary blue is `#2563eb` (via `--brand-primary`)
- [ ] Inter for UI text, JetBrains Mono for code
- [ ] Border radius: `--radius-lg` for cards, `--radius-md` for inputs, `--radius-full` for pills
- [ ] Shadows: `--shadow-sm` / `--shadow-md` / `--shadow-lg` (not custom values)
- [ ] Icons: lucide-react or the custom set (same stroke weight)
- [ ] Voice: technical, concise, no-nonsense
- [ ] Dark mode works (`.dark` class swaps semantic tokens)
- [ ] Logo uses the variant appropriate for context (see `docs/design-system/02-brand.md`)
- [ ] Landing page hero uses the same gradient as the app's onboarding screen

## Logo usage on the landing page

| Context | Logo file |
|---------|-----------|
| Navbar | `logo-horizontal.svg` |
| Favicon | `logo-favicon.svg` (or `logo-mark.svg`) |
| Footer | `logo-mono.svg` |
| Social cards (OG image) | `logo-mark.svg` on a branded background |
| Splash / loading | `logo-vertical.svg` |

```tsx
// Navbar
<Image src="/brand/logo-horizontal.svg" alt="autostand" width={150} height={32} />

// Favicon (in metadata)
<icon>/brand/logo-favicon.svg</icon>

// OG image (1200×630 PNG, generated from logo-mark.svg + brand colors)
<meta property="og:image" content="/brand/logo-og.png" />
```

## Dark mode on the landing page

The landing page supports `.dark` class (same token swap as the app):

```tsx
// Toggle in navbar
<button onClick={() => document.documentElement.classList.toggle("dark")}>
  <SunIcon className="dark:hidden" />
  <MoonIcon className="hidden dark:block" />
</button>
```

Or follow the system preference:
```tsx
useEffect(() => {
  const mq = matchMedia("(prefers-color-scheme: dark)");
  document.documentElement.classList.toggle("dark", mq.matches);
  mq.addEventListener("change", (e) => {
    document.documentElement.classList.toggle("dark", e.matches);
  });
}, []);
```

The same `tokens.css` `.dark` block handles the swap — no extra CSS needed.

## Hero gradient (consistency)

The app's onboarding screen and the landing page hero use the **same gradient**:

```css
.hero-gradient {
  background: linear-gradient(
    135deg,
    var(--bg-base) 0%,
    color-mix(in srgb, var(--brand-primary) 10%, var(--bg-surface)) 100%
  );
}
```

This creates visual continuity: a user who sees the landing page, then installs the app, recognizes the same look.

## Future: `@autostand/ui` package

Once the design system stabilizes, publish it as `@autostand/ui` for reuse across:

| Surface | How it consumes |
|---------|----------------|
| Tauri app | `@autostand/ui` + app components (local) |
| Landing page | `@autostand/ui` (base components) |
| Docs site | `@autostand/ui` (base components, minimal) |
| Storybook | `@autostand/ui` source (stories live in the package) |

This unifies all surfaces under one package. Version bumps propagate everywhere. Breaks in one surface are caught in CI (Storybook test-runner runs against the package).

Until then, the monorepo structure (shared `design-system/` folder) is sufficient — all surfaces live in the same repo and import via relative paths.