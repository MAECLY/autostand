# Brand

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
| **Inter** | Sans — UI text, display, body | 400, 500, 600, 700 | Google Fonts or self-hosted woff2 |
| **JetBrains Mono** | Mono — code, audit JSON, commit SHAs, file paths | 400, 500, 700 | Google Fonts or self-hosted woff2 |

Both are open-source (SIL OFL). Self-host woff2 for the Tauri app (no network dependency on Google Fonts). For the landing page, Google Fonts is fine.

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

| Role | Token | Hex | Usage |
|------|-------|-----|-------|
| Primary | `--brand-primary` (`--color-blue-600`) | `#2563eb` | Primary buttons, links, active states, focus ring |
| Primary hover | `--brand-primary-hover` (`--color-blue-700`) | `#1d4ed8` | Hover state for primary |
| Accent | `--brand-accent` (`--color-purple-500`) | `#ab7aff` | Highlights, accent badges, secondary CTA |
| Neutral | slate scale (50–950) | `#f8fafc`–`#020617` | Backgrounds, text, borders |
| Success | `--status-success` (`--color-green-600`) | `#16a34a` | "Done" status, commit badge |
| Warning | `--status-warning` (`--color-amber-600`) | `#d97706` | "Partial" / "needs attention", note badge |
| Error | `--status-error` (`--color-red-600`) | `#dc2626` | Errors, phantom badge |
| Info | `--status-info` (`--color-blue-600`) | `#2563eb` | Info badges, github badge |

Full palette in `docs/design-system/01-tokens.md`.

## Iconography

| Set | Usage |
|-----|-------|
| **lucide-react** | Standard UI icons (consistent stroke weight, MIT licensed). Pre-installed via shadcn/ui. |
| **Custom SVG** | App-specific concepts not in lucide |

Custom icons (in `design-system/icons/`):

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
| `brand/logo/logo-og.png` | 1200×630 social card | PNG | Open Graph meta tag |
| `design-system/tokens/tokens.css` | Design tokens | CSS | App + Storybook + landing |
| `design-system/icons/*.svg` | Custom icons | SVG | App components |
| `apps/autostand-app/src/fonts/*.woff2` | Self-hosted fonts | woff2 | Tauri app |

### Adding a new brand asset

1. Place the SVG/PNG in `brand/logo/` (or `brand/` subfolder).
2. Update the asset inventory table above.
3. If it's a logo variant, ensure a mono version exists (`currentColor`).
4. If it's an icon, add to `design-system/icons/` and export from the icon index.

## Brand consistency checklist

Before shipping any UI surface (app, landing, docs site):

- [ ] Primary blue is `#2563eb` (via `--brand-primary`, never hardcoded)
- [ ] Inter for UI text, JetBrains Mono for code
- [ ] Border radius uses `--radius-lg` for cards, `--radius-md` for inputs, `--radius-full` for pills
- [ ] Shadows use `--shadow-sm` / `--shadow-md` / `--shadow-lg` (not custom values)
- [ ] Icons are from lucide-react or the custom set (consistent stroke)
- [ ] Voice is technical, concise, no-nonsense
- [ ] Dark mode works (`.dark` class swaps semantic tokens)
- [ ] Logo uses the variant appropriate for context (see table above)