# Sharing the design system across repos

This document used to argue about how a marketing site could reuse the design system, and weighed two options:
copy the components into the site, or publish the design system as a package. **Option B was taken.** The design
system now lives in its own repository and is installed as a dependency; the marketing site lives in a third
repository. This is the record of what was actually done and what it costs.

## The three repos

| Repo | Contains | Ships as |
| --- | --- | --- |
| [`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui) | Tokens, 24 base components, the icon set, the brand fonts, Storybook | the `@autostand/ui` package |
| [`MAECLY/autostand`](https://github.com/MAECLY/autostand) | The Rust workspace and the Tauri desktop app | desktop bundles from `release.yml` |
| [`MAECLY/autostand-landing-page`](https://github.com/MAECLY/autostand-landing-page) | The Next.js 15 marketing site | a Vercel deployment |

Both consumers depend on the same package, so a token edit lands in the product and the marketing site the same
way: bump the pinned commit, install, done. That is the whole point of the split — one source of truth for how
autostand looks, with no copy to keep in sync and no relative path reaching across project boundaries.

## What the package looks like

`@autostand/ui` ships **source**, not a build. There is no `dist/`, no `tsup`, no rollup step. Every consumer
already runs a bundler that handles TypeScript and JSX, so a build step would only be one more thing to keep in
sync. Vite consumes it as-is; Next.js needs it listed in `transpilePackages`.

```jsonc
// apps/autostand-app/package.json
{
  "dependencies": {
    "@autostand/ui": "github:MAECLY/autostand-ui#main"
  }
}
```

```tsx
import "@autostand/ui/styles.css";                        // once, from the app stylesheet
import { Button } from "@autostand/ui/components/button";
import { PipelineIcon } from "@autostand/ui/icons";
import { cn } from "@autostand/ui/lib/utils";
```

The subpath exports are declared in the package's `exports` map — `./styles.css`, `./tokens.css`, `./fonts.css`,
`./components`, `./components/*`, `./icons`, `./lib/utils`. Nothing resolves by deep relative path, in either
direction.

Components inside the package import each other **relatively** (`../lib/utils`). The `@/` alias means something
different in every consuming project, so it is banned there.

### Versioning

The specifier is a branch (`#main`), but pnpm resolves it once and pins the commit in the lockfile:

```yaml
'@autostand/ui@git+ssh://git@github.com/MAECLY/autostand-ui.git#f53413ec…':
  resolution: {commit: f53413ec…, repo: git@github.com:MAECLY/autostand-ui.git, type: git}
```

So an install is reproducible: pushing to `autostand-ui/main` does **not** silently change what this repo builds.
Picking up a design-system change is a deliberate act — re-resolve the dependency and commit the new lockfile.
There is no semver and no npm registry; the commit SHA in `pnpm-lock.yaml` is the version.

## How each surface wires it up

### The desktop app

`apps/autostand-app/src/styles/globals.css` is the only file that touches the stylesheet:

```css
@import "@autostand/ui/styles.css";

@source "../../node_modules/@autostand/ui/components";
@source "../../node_modules/@autostand/ui/icons";
```

The `@import` pulls in Tailwind v4, `tokens.css`, the `@theme` mapping and the `@font-face` rules in one go. The
two `@source` lines exist because Tailwind v4's automatic content detection never descends into `node_modules`.
They are belt and braces rather than load-bearing today — the package declares its own `@source "../components"`,
which Tailwind resolves against the package's own stylesheet — but they keep the app's CSS independent of that
internal detail. (Verified: building with them commented out produces a byte-identical stylesheet.)

The fonts travel with the package. `styles/fonts.css` points at `../fonts/*.woff2` and Tailwind rebases those
urls against the file that declares them, so Vite fingerprints all seven files into the app bundle:

```
dist/assets/inter-400-C38fXH4l.woff2            23.66 kB
dist/assets/inter-500-Cerq10X2.woff2            24.27 kB
dist/assets/inter-600-LgqL8muc.woff2            24.45 kB
dist/assets/inter-700-Drs_5D37.woff2            24.25 kB
dist/assets/jetbrains-mono-400-V6pRDFza.woff2   21.17 kB
dist/assets/jetbrains-mono-500-BWZEU5yA.woff2   21.83 kB
dist/assets/jetbrains-mono-700-BYuf6tUa.woff2   21.91 kB
```

No surface fetches a font at runtime, which matters most for the Tauri app: it has no network budget to spend on
typography.

### The marketing site

Same import, one extra step for Next.js (`transpilePackages: ["@autostand/ui"]`, because the package is source).
Its `src/app/globals.css` declares the same pair of `@source` lines and adds two site-local things the product has
no use for: a `--text-hero` display size and the `.hero-gradient` shared with the app's onboarding screen.

## What the package deliberately does not contain

Base components are pure presentation — props in, markup out, no data fetching, no app types, no Tauri. That is
what makes them portable to a marketing site at all.

**App components stay in the app.** `AuditViewer`, `StandupPreview`, `PipelineCard`, `SchedulerForm` and the rest
live in `apps/autostand-app/src/components/`, because they know about autostand's domain types and about `invoke`.
The marketing site does not import them; it renders its own static demo built from base components and hardcoded
sample data. Shipping a Tauri-aware component to a web page would mean shipping a mock layer with it, and the
mock would be the thing that drifts.

## The cost

The split is not free, and the bill lands in exactly one place: **CI authentication.**

`autostand-ui` is a private repository, so pnpm resolves it over SSH and every consumer needs credentials to
install. On a developer machine this is invisible — the SSH key that clones the repo also clones the dependency.
In GitHub Actions it is not: the built-in `GITHUB_TOKEN` is scoped to the repository the workflow runs in, so it
cannot read `autostand-ui`, and `pnpm install --frozen-lockfile` fails inside git with a bare authentication
error.

Both JS jobs in `.github/workflows/ci.yml` therefore run an `Authenticate to MAECLY/autostand-ui` step first,
which requires a repository secret named `AUTOSTAND_UI_TOKEN` (a fine-grained PAT with `Contents: Read` on
`MAECLY/autostand-ui`, or a GitHub App installation token) and rewrites the git URL to use it. **Until that
secret is created, the `frontend` and `e2e` jobs fail** — with a clear message naming the secret rather than a
git error, but they fail. See `docs/dev/04-ci-cd.md` § Private dependency authentication.

The other costs are smaller and were accepted knowingly:

- A design-system change is now two commits in two repos plus a lockfile bump, not one commit.
- A breaking change to a component is caught by the consumer's typecheck at bump time, not at edit time.
- Three repos means three CI configurations to keep honest.

Against that: no copy of the components exists anywhere, so no copy can drift — which was the failure mode
Option A guaranteed.

## Brand consistency checklist

Still applies to any surface, in any repo:

- [ ] Colour, radius, shadow and font come from tokens — no hardcoded values
- [ ] Primary blue via `--brand-primary`; Inter for UI text, JetBrains Mono for code
- [ ] Icons are lucide-react or the package's custom set (same stroke weight)
- [ ] Dark mode works — `.dark` on `<html>` swaps the semantic layer
- [ ] Logo variant matches the context (see `docs/design-system/02-brand.md`)
- [ ] Voice: technical, concise, no marketing fluff in the product
- [ ] Every claim on the marketing site is true of autostand as it exists

## Hero gradient

The app's onboarding screen and the site's hero use the same gradient, so a visitor who installs the app
recognises what they saw:

```css
.hero-gradient {
  background: linear-gradient(
    135deg,
    var(--bg-base) 0%,
    color-mix(in srgb, var(--brand-primary) 10%, var(--bg-surface)) 100%
  );
}
```

It is defined per surface rather than in the package: it is a composition, not a token, and only two screens in
two repos use it.
