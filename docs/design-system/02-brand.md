# Brand

> **Where the code lives.** The logo suite stays in this repo under `brand/logo/` (the generators in `tests/`
> produce it and the app icons from it), and the marketing site keeps its own copy under `public/brand/`. Fonts
> and the custom icon set moved into the `@autostand/ui` package
> ([`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui)); paths like `icons/` and `tokens/` below are
> relative to that repository's root.

The autostand brand is technical, concise, and no-nonsense — matching the product's voice: automate the tedious, surface what matters.

## Brand name

**autostand** — one word, lowercase. Used as "autostand" in prose, "Autostand" at sentence start, "AUTOSTAND" only in wordmarks/lockups where typographic context demands it.

## Logo

SVGs live in `brand/logo/`. All are hand-tuned (not auto-exported) for crispness at small sizes.

| File | Description | Usage |
|------|-------------|-------|
| `logo-mark.svg` | Icon only (the "mark") | Favicon, app icon, social cards, avatar |
| `logo-horizontal.svg` | Icon + wordmark, horizontal | Navbar, header, README badge |
| `logo-vertical.svg` | Icon over wordmark, stacked | Splash screen, loading state |
| `logo-mono.svg` | Monochrome horizontal lockup (`currentColor`) | Footer, dark backgrounds, print — must be inlined |
| `logo-favicon.svg` | Small-optimized mark | Browser tab, PWA icon |

### Icon concept

The mark is a stylized stand-up card (rounded rectangle) with a lightning bolt forming a checkmark — automation (lightning) + completion (check) + the standup card metaphor. Simple geometric forms, recognizable at 16px.

Design rules:
- Works at 16×16 (favicon), 32×32 (toolbar), 256×256 (app icon), 1024×1024 (store).
- Single color (uses `currentColor`) for the mono variant.
- Full-color variant fills the card with `--brand-primary` (`#2563eb`) and knocks the bolt/check out in
  `--bg-surface`. (The roles are the reverse of what this doc originally specified: a white card on a white page
  is an invisible container, and legibility at 16×16 is the harder constraint — a solid tile is what survives at
  favicon and dock sizes.)
- No gradients in the mark itself (gradients allowed in hero backgrounds only).

The logo SVGs carry literal hex rather than `var(--brand-primary)`: CSS variables do not resolve when an SVG is
loaded through `<img>` or as a favicon, so the variable would be dead weight in exactly the contexts these files
are used. `logo-mono.svg` is the exception — it uses `currentColor` throughout and must be **inlined** into the
host document (paste the markup, or a `?raw` import) for that to resolve.

Regenerate the whole suite with `python3 tests/make-wordmark.py`. The wordmark is extracted from Inter 700 as
path outlines, never an SVG `<text>` element, so the lockup cannot reshape itself on a machine without Inter.

## Typography

| Font | Role | Weights | Source |
|------|------|---------|--------|
| **Inter** | Sans — UI text, display, body | 400, 500, 600, 700 | self-hosted woff2 in `@autostand/ui` |
| **JetBrains Mono** | Mono — code, audit JSON, commit SHAs, file paths | 400, 500, 700 | self-hosted woff2 in `@autostand/ui` |

Both are open-source (SIL OFL), which is what makes redistributing them inside the package legal. Every surface
self-hosts: the Tauri app has no network to spend on typography, and the marketing site should not hand Google a
request per visitor. The latin subsets live in `fonts/` and are declared by `styles/fonts.css`, so importing
`@autostand/ui/styles.css` is all a consumer does — the app's Vite build fingerprints all seven woff2 files into
`dist/assets/`.

### Usage

- **Inter 400** — body text, descriptions, labels
- **Inter 500** — buttons, nav items, table headers
- **Inter 600** — section titles, card titles, emphasis
- **Inter 700** — page titles, hero headline, brand wordmark
- **JetBrains Mono 400** — inline code, file paths, commit SHAs
- **JetBrains Mono 500** — audit JSON keys, diff hunks

Font tokens defined in `tokens.css`:
```css
--font-sans: "Inter", system-ui, -apple-system, sans-serif;
--font-mono: "JetBrains Mono", "SF Mono", "Cascadia Code", monospace;
--font-display: "Inter", system-ui, sans-serif;
```

## Color palette

Brand colors (from `tokens.css`):

| Role | Token | Light | Dark | Usage |
|------|-------|-------|------|-------|
| Primary | `--brand-primary` | `#2563eb` (blue-600) | `#60a5fa` (blue-400) | Primary buttons, links, active states |
| Primary hover | `--brand-primary-hover` | `#1d4ed8` (blue-700) | `#93c5fd` (blue-300) | Hover state for primary |
| On primary | `--fg-on-brand` | `#ffffff` | `#020617` (slate-950) | Label on a brand-filled control |
| Accent | `--brand-accent` (`--color-purple-500`) | `#ab7aff` | `#ab7aff` | Highlights, accent badges, secondary CTA |
| Neutral | slate scale (50–950) | `#f8fafc`–`#020617` | `#020617`–`#f8fafc` | Backgrounds, text, borders |
| Success | `--status-success` | `#15803d` (green-700) | `#4ade80` (green-400) | "Done" status, commit badge |
| Warning | `--status-warning` | `#b45309` (amber-700) | `#fbbf24` (amber-400) | "Partial" / "needs attention", note badge |
| Error | `--status-error` | `#b91c1c` (red-700) | `#f87171` (red-400) | Errors, phantom badge |
| Info | `--status-info` | `#2563eb` (blue-600) | `#60a5fa` (blue-400) | Info badges, github badge |

**The brand blue is `#2563eb`.** It stays blue-600 in light mode, where it is the colour a visitor and a user both see as *autostand blue*. Dark mode paints the same hue two stops lighter (`#60a5fa`), because blue-600 on the dark page background is 3.90:1 — under WCAG AA for the links and icons drawn with it. The lighter step is a legibility adjustment to the same brand colour, not a second brand colour, and it is the standard way to carry a brand hue onto a dark surface. The logo assets are unaffected: they carry literal `#2563eb` in both themes.

Green, amber and red are one stop darker than the usual `-600` for the same reason — see `docs/design-system/01-tokens.md` § Contrast. Full palette there too.

## Iconography

| Set | Usage |
|-----|-------|
| **lucide-react** | Standard UI icons (consistent stroke weight, MIT licensed). Pre-installed via shadcn/ui. |
| **Custom SVG** | App-specific concepts not in lucide |

Custom icons (in `icons/`):

| Icon | Concept |
|------|---------|
| `standup-file.svg` | A standup `.md` file (card with lines) |
| `pipeline.svg` | The compile pipeline (connected stages) |
| `host.svg` | A machine/host (monitor + slug) |
| `audit-phantom.svg` | A ghost (phantom detection) |
| `audit-commit.svg` | A commit node |
| `audit-github.svg` | GitHub mark (for PR/review badges) |

All custom icons use `currentColor` and follow lucide's stroke conventions (2px stroke, 24×24 viewBox, rounded caps).

## Voice

| Do | Don't |
|----|-------|
| "Automate your standup." | "Leverage AI to synergize your daily synchronization ritual." |
| "Compile now" | "Execute the compilation procedure" |
| "Phantom — claims work with no matching source" | "Anomalous claim detected via provenance verification" |
| "No standup generated. Install the scheduler." | "It appears no standup was generated. Please ensure the scheduling daemon is properly configured." |

Principles:
- Short sentences.
- Active voice.
- No marketing fluff in the app (save it for the landing page).
- Error messages name the cause and the fix.
- Tagline: **"Automate your standup. Know what you did."**

## Landing page hero

The marketing landing page hero (see `docs/design-system/06-landing-reuse.md`):

- **Background:** gradient from `--bg-base` to a tint of `--brand-primary` (e.g., `#2563eb` at 10% opacity over `--bg-surface`).
- **Headline:** Inter 700, `--text-4xl` (36px) or larger. "Automate your standup. Know what you did."
- **Subhead:** Inter 400, `--text-lg`. "autostand gathers your commits, PRs, and notes — then writes your daily standup for you."
- **CTAs:** Primary button ("Download") + secondary outline button ("View on GitHub").
- **Mockup:** app screenshot (Dashboard) in a browser-frame mockup, with `shadow-lg`.

## Asset inventory

| Path | What | Format | Used by |
|------|------|--------|---------|
| `brand/logo/logo-mark.svg` | Icon mark | SVG | Favicon, app icon, social |
| `brand/logo/logo-horizontal.svg` | Icon + wordmark, horizontal | SVG | Navbar, README |
| `brand/logo/logo-vertical.svg` | Icon over wordmark | SVG | Splash, loading |
| `brand/logo/logo-mono.svg` | Monochrome | SVG | Footer, print |
| `brand/logo/logo-favicon.svg` | Small-optimized | SVG | Browser tab |
| `brand/logo/logo-og.png` | 1200×630 social card | PNG | README header, GitHub social preview, Open Graph meta tag. Built by `pnpm og:image` in `autostand-landing-page` from the real dashboard capture, and vendored here — do not redraw it. |
| `tokens/tokens.css` (in `autostand-ui`) | Design tokens | CSS | App + marketing site + Storybook |
| `icons/*.tsx` (in `autostand-ui`) | Custom icons as React components | TSX | App + marketing site |
| `fonts/*.woff2` (in `autostand-ui`) | Self-hosted fonts | woff2 | App + marketing site |

### Adding a new brand asset

1. Place the SVG/PNG in `brand/logo/` (or `brand/` subfolder).
2. Update the asset inventory table above.
3. If it's a logo variant, ensure a mono version exists (`currentColor`).
4. If it's an icon, add it to `icons/` in the `autostand-ui` repo and export it from the icon index there.

## Brand consistency checklist

Before shipping any UI surface (app, landing, docs site):

- [ ] Primary blue is `#2563eb` in light mode (via `--brand-primary`, never hardcoded; `#60a5fa` under `.dark`)
- [ ] Inter for UI text, JetBrains Mono for code
- [ ] Border radius uses `--radius-lg` for cards, `--radius-md` for inputs, `--radius-full` for pills
- [ ] Shadows use `--shadow-sm` / `--shadow-md` / `--shadow-lg` (not custom values)
- [ ] Icons are from lucide-react or the custom set (consistent stroke)
- [ ] Voice is technical, concise, no-nonsense
- [ ] Dark mode works (`.dark` class swaps semantic tokens, including the saturated ones)
- [ ] Every text pair clears 4.5:1 in both themes, verified against the shipped `tokens.css`
- [ ] Logo uses the variant appropriate for context (see table above)