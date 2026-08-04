# autostand landing page

The public marketing site for autostand. Static HTML, no server runtime, no
analytics, no third-party scripts.

It is a separate package from the Tauri app on purpose (`docs/design-system/06-landing-reuse.md`):
the app is a desktop binary, this is a website, and they ship to different
places. What they share is the design system, so the two surfaces cannot drift
apart visually.

## Stack

| Piece | What it is |
|---|---|
| [Astro 5](https://astro.build) | `output: "static"` — every page is prerendered to HTML at build time |
| `@astrojs/react` | React 19 islands, used only where a section is genuinely interactive |
| Tailwind v4 (`@tailwindcss/vite`) | no `tailwind.config.js`; the theme is the `@theme` block in the shared stylesheet |
| TypeScript strict | via `astro/tsconfigs/strict` |

## Commands

Run from this directory, or from the repo root with `pnpm --filter landing <script>`.

```bash
pnpm dev        # astro dev  — http://localhost:4321/autostand
pnpm build      # astro build — writes dist/
pnpm preview    # serve dist/ exactly as it will be deployed (base path included)
pnpm lint       # eslint
pnpm typecheck  # astro check — types inside .astro and .tsx
```

`pnpm build` must be run before `pnpm preview`; preview serves `dist/`, it does
not build.

> `pnpm typecheck` currently reports one error: `astro.config.mjs` cannot resolve
> `node:url` because this package has no `@types/node`. It is a type-only
> diagnostic — the build is unaffected. Fix by adding `@types/node` to
> `devDependencies`.

## Where the styling comes from

Nothing about the look of this site is defined in this package.

```
design-system/tokens/tokens.css     ← raw tokens + the .dark override block
design-system/styles/fonts.css      ← @font-face for Inter + JetBrains Mono
design-system/styles/globals.css    ← imports both, maps them into Tailwind's @theme
  └── apps/landing/src/styles/globals.css   ← imports that, adds --text-hero and .hero-gradient
        └── src/layouts/Base.astro          ← imports that
```

Consequences worth knowing before you edit a component here:

- **Never hardcode a colour, radius, shadow or font.** Use the mapped utilities
  (`bg-surface`, `text-muted-foreground`, `border-border`, `rounded-lg`,
  `shadow-md`, `font-mono`, …). A raw hex here is a drift bug, not a style choice.
- **Dark mode is the `.dark` class on `<html>`**, applied before first paint by an
  inline script in `Base.astro` and toggled by `src/components/ThemeToggle.tsx`
  (persisted under the `autostand-theme` localStorage key). The tokens flip
  themselves, so a `dark:` variant is almost never needed.
- **Base components come from the design system**, imported as
  `import { Button } from "@design-system/components/button"`. They are the same
  files the desktop app renders. Don't copy them into this package.
- The two fonts are self-hosted woff2 and get fingerprinted into `dist/_astro/`
  at build time. The page loads nothing from a CDN.

Path aliases (`astro.config.mjs` + `tsconfig.json`): `@/` → `src/`,
`@design-system/` → `../../design-system/`.

## Islands, and why there are only two

Astro renders React to static HTML by default. A `client:*` directive is the
only thing that ships JavaScript, so each one has to earn its place:

| Component | Directive | Why |
|---|---|---|
| `ThemeToggle` (inside `Navbar.astro`) | `client:load` | it has to agree with the class the pre-paint script already stamped on `<html>` |
| `Faq` | `client:visible` | Radix Accordion; below the fold, and a panel that cannot open is worse than no panel |

Everything else — hero, features, pipeline, audit demo, footer, and the mobile
menu (a native `<details>`) — is markup, and ships zero JavaScript.

## Structure

```
public/brand/            copies of brand/logo/* — served at /autostand/brand/…
src/layouts/Base.astro   <html>, <head>, meta/OG tags, pre-paint theme script
src/pages/index.astro    the whole page: composition only, no styling of its own
src/components/          one file per section
src/styles/globals.css   imports the design system, adds --text-hero + .hero-gradient
```

`index.astro` is deliberately thin. Each section owns its own container and
vertical rhythm, so `<main>` carries no width or padding classes — adding them
double-pads every section.

`public/` is served verbatim at the site base. The brand files there are copies
of `brand/logo/*`; if the source logos change, re-copy them.

## Deployment

`pnpm build` produces a fully static `dist/` — HTML, one CSS file, two island
chunks, the woff2 fonts and `public/`. Any static host will serve it.

It is configured for **GitHub Pages** under the `/autostand` base path
(`astro.config.mjs`):

```js
site: "https://maecly.github.io",
base: "/autostand",
```

So `dist/index.html` is served at `https://maecly.github.io/autostand/`.

If you deploy somewhere the site lives at the domain root, change `base` to
`"/"` and `site` to that origin — both values are baked into asset URLs and the
`og:image` at build time, so a wrong `base` produces 404s rather than a
redirect.

Building an asset URL by hand? `import.meta.env.BASE_URL` is `"/autostand"` with
**no** trailing slash, so normalise it:

```ts
const base = `${import.meta.env.BASE_URL.replace(/\/$/, "")}/`;
// `${base}brand/logo-mark.svg` → /autostand/brand/logo-mark.svg
```

### The Pages workflow

`.github/workflows/pages.yml` publishes this site on every push to `main` (and
on `workflow_dispatch`). A repo gets one Pages site, and this repo has two web
surfaces, so that one workflow builds both and uploads them as a single artifact:

```
_site/            ← the uploaded artifact
├── index.html    ← this package (apps/landing/dist)
├── _astro/
├── brand/
└── storybook/    ← design-system/storybook-static
```

| Path | Served at |
|---|---|
| `_site/` | `https://maecly.github.io/autostand/` |
| `_site/storybook/` | `https://maecly.github.io/autostand/storybook/` |

It uses the first-party flow (`actions/configure-pages` →
`actions/upload-pages-artifact` → `actions/deploy-pages`) with
`permissions: { contents: read, pages: write, id-token: write }`. Nothing is
pushed to a `gh-pages` branch, and the artifact is served verbatim — Jekyll never
runs, so `_astro/` is safe.

Reproduce the artifact locally, exactly as the workflow builds it:

```bash
pnpm --filter landing build
pnpm --filter design-system build-storybook
rm -rf _site && mkdir -p _site/storybook          # from the repo root
cp -R apps/landing/dist/. _site/
cp -R design-system/storybook-static/. _site/storybook/

# serve it under the real base path
mkdir -p /tmp/pages && cp -R _site /tmp/pages/autostand
(cd /tmp/pages && python3 -m http.server 8080)
# → http://localhost:8080/autostand/  and  /autostand/storybook/
```

`pnpm preview` does the same thing for this package alone (base path included),
but it will not show you Storybook or catch a collision between the two.

The workflow's `Verify base paths` step fails the build if `dist/index.html` ever
emits a root-absolute `href`/`src` that is not under `/autostand/`, or references
a file that is not in the artifact. That is the guard against someone changing
`base` and shipping a page whose CSS 404s. Storybook needs no base setting of its
own — `@storybook/builder-vite` emits a fully relative bundle — and the same step
fails if that stops being true.

## Editing the copy

Every claim on this page has to be true of the code in this repo today. There is
no released binary, no pricing, no account, no telemetry and no user count, so
the site has no download link, no pricing section, no testimonials and no
metrics — the CTA points at the GitHub repo and says the binary is not published
yet. Voice rules are in `docs/design-system/02-brand.md` § Voice: short
sentences, active voice, technical, no marketing language.
