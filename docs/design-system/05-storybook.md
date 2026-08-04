# Storybook 8

> **Where the code lives.** Storybook is part of the `@autostand/ui` package and runs from
> [`MAECLY/autostand-ui`](https://github.com/MAECLY/autostand-ui) — `pnpm storybook` in **that** repo, not this
> one. There is no Storybook script here any more, and nothing in this repo builds or deploys it. Paths below
> are relative to the `autostand-ui` root.

Storybook is the development and documentation environment for the autostand design system. It documents the
base components, the icon set and the tokens — everything the desktop app and the marketing site consume.

## Setup

Storybook 8 is installed at the root of the `autostand-ui` repo. Config in `.storybook/`:

### `main.ts`

```ts
import type { StorybookConfig } from "@storybook/react-vite";

const config: StorybookConfig = {
  stories: ["../**/*.stories.tsx"],
  addons: [
    "@storybook/addon-essentials",
    "@storybook/addon-links",
    "@storybook/addon-a11y",
    // "@storybook/addon-interactions",  // if using play functions
  ],
  framework: {
    name: "@storybook/react-vite",
    options: {},
  },
  viteFinal: (config) => {
    return {
      ...config,
      plugins: [
        ...config.plugins,
        // Tailwind v4 Vite plugin + tokens injection
      ],
    };
  },
};

export default config;
```

### `preview.ts`

```ts
import type { Preview } from "@storybook/react";
import "../styles/globals.css";  // Tailwind v4 + tokens; the app imports this same file

const preview: Preview = {
  parameters: {
    actions: { argTypesRegex: "^on[A-Z].*" },
    controls: {
      matchers: { color: /(background|color)$/i, date: /Date$/i },
    },
    backgrounds: {
      default: "light",
      values: [
        { name: "light", value: "var(--bg-base)" },
        { name: "dark", value: "var(--color-slate-950)" },
      ],
    },
    layout: "centered",   // or "padded" for full-width components
  },
};

export default preview;
```

Dark mode: the backgrounds addon toggles between light (`--bg-base`) and dark (`--color-slate-950`). The `.dark` class is applied to the preview root when dark is selected (via a decorator or the `darkMode` addon).

## Story file structure

`ComponentName.stories.tsx`:

```tsx
import type { Meta, StoryObj } from "@storybook/react";
import { Button } from "./button";

const meta = {
  title: "Components/Button",       // hierarchy in the sidebar
  component: Button,
  parameters: {
    layout: "centered",
    docs: { description: { component: "Primary action button..." } },
  },
  tags: ["autodocs"],
  argTypes: {
    variant: {
      control: "select",
      options: ["default", "destructive", "outline", "secondary", "ghost", "link"],
    },
  },
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = {
  args: { children: "Button", variant: "default" },
};

export const Variants: Story = {
  render: () => (
    <div className="flex gap-2">
      <Button variant="default">Default</Button>
      <Button variant="destructive">Destructive</Button>
      <Button variant="outline">Outline</Button>
      <Button variant="secondary">Secondary</Button>
      <Button variant="ghost">Ghost</Button>
      <Button variant="link">Link</Button>
    </div>
  ),
};

export const States: Story = {
  render: () => (
    <div className="flex gap-2">
      <Button>Normal</Button>
      <Button disabled>Disabled</Button>
      <Button size="sm">Small</Button>
      <Button size="lg">Large</Button>
    </div>
  ),
};
```

Conventions:
- `satisfies Meta<typeof Component>` — type-safe meta.
- `Default` story — the most common usage with `args`.
- `Variants` story — all variants side-by-side.
- `States` story — all states (default, hover, focus, disabled, loading).
- Use `args` for interactive props (Controls addon).
- `tags: ["autodocs"]` — generates docs page.

## Categories

Three story categories in the sidebar:

### Base components

`components/*.stories.tsx`

```
Components/
├── Button
├── Card
├── Dialog
├── DropdownMenu
├── Input
├── Textarea
├── Label
├── Select
├── Switch
├── Checkbox
├── Tabs
├── Tooltip
├── Badge
├── Progress
├── Separator
├── ScrollArea
├── Sonner
├── Alert
├── Table
├── Accordion
└── Collapsible
```

### App components

`app-components/*.stories.tsx`

```
App Components/
├── StandupPreview
├── AutoBlockView
├── ManualEditor
├── ProviderCard
├── DataSourceToggle
├── PathInput
├── PipelineProgress
├── AuditBadge
├── AuditViewer
├── SchedulerControl
├── HostSlugDisplay
├── CalendarPicker
├── StatusBar
└── QuickAddDialog
```

### Tokens page

`tokens/tokens.stories.tsx` — a special story showing all tokens visually:

```
Tokens/
├── Colors       (grid of all color tokens with hex values)
├── Spacing      (visual scale)
├── Typography   (all text sizes + weights)
├── Radius       (all radii)
├── Shadow       (all shadows)
└── Audit Colors (all audit classification colors)
```

This is the design system's "source of truth" page — designers and devs check it to confirm token values render correctly.

## Commands

| Command | What it does |
|---------|--------------|
| `pnpm storybook` | Start Storybook dev server at `localhost:6006` |
| `pnpm build-storybook` | Build static Storybook to `storybook-static/` |
| `pnpm storybook test` | Run Storybook test-runner (interaction tests) |

All three run **from the `autostand-ui` repo**, whose `package.json` defines them.

## CI

`autostand-ui`'s own workflow builds Storybook on every push and pull request — that build is what catches a
broken story. Hosting goes through Vercel like the rest of autostand's hosting: `vercel.json` in that repo
already declares the build command and output directory, so connecting the repo in Vercel is all it takes.
Nothing is published until someone does, so until then Storybook is a local tool.

This repo used to publish it at `https://MAECLY.github.io/autostand/storybook/` from `pages.yml`, alongside the
Astro landing page. That workflow is deleted; the monorepo no longer deploys any web surface.

## Testing

### Storybook test-runner

`@storybook/test-runner` runs interaction tests against stories (headless browser):

```bash
pnpm storybook test
```

Use `play` functions in stories to test interactions:

```tsx
export const Interactive: Story = {
  play: async ({ canvasElement }) => {
    const canvas = within(canvasElement);
    const button = canvas.getByRole("button");
    await userEvent.click(button);
    await expect(canvas.getByText("Clicked")).toBeInTheDocument();
  },
};
```

### Chromatic (optional)

[Chromatic](https://chromatic.com) for visual regression:
- Snapshots every story on every PR.
- Catches unintended visual changes.
- Requires a Chromatic token (secret: `CHROMATIC_PROJECT_TOKEN`).
- Free for open-source projects.

Not required — the test-runner covers interaction testing. Add Chromatic when visual regression becomes a pain point.

## Adding a new story

1. Create the component (e.g., `components/pagination.tsx`).
2. Create the story (`components/pagination.stories.tsx`).
3. Run `pnpm storybook` (in `autostand-ui`) — the story appears in the sidebar.
4. Verify in light + dark mode (use the backgrounds toolbar).
5. Add `tags: ["autodocs"]` for a docs page.
6. Commit both files in the same PR.

## Storybook + Tauri

Storybook runs in the browser (Vite), NOT in Tauri — and after the split it cannot reach Tauri even in
principle: it lives in a repo that has no Rust, no `@tauri-apps/api` and no autostand domain types.

That is why only **base** components have stories. App components (`AuditViewer`, `PipelineCard`, …) stay in
`apps/autostand-app/src/components/` and are covered by vitest and Playwright in this repo instead — see
`docs/design-system/04-app-components.md` and `docs/dev/03-testing.md`.