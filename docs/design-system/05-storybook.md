# Storybook 8

Storybook is the development + documentation environment for the autostand design system. It lives in `design-system/` and is shared between the Tauri app and (future) landing page.

## Setup

Storybook 8 is installed in `design-system/`:

```bash
pnpm dlx storybook@latest init    # run inside design-system/
```

Config in `design-system/.storybook/`:

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
import "../tokens/tokens.css";
import "../app/globals.css";   // or a shared design-system/styles.css

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

`design-system/components/*.stories.tsx`

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

`design-system/app-components/*.stories.tsx`

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

`design-system/tokens/tokens.stories.tsx` — a special story showing all tokens visually:

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
| `pnpm build-storybook` | Build static Storybook to `design-system/storybook-static/` |
| `pnpm storybook test` | Run Storybook test-runner (interaction tests) |

Run from the repo root. The `pnpm storybook` command is defined in the root `package.json` and invokes Storybook in `design-system/`.

## CI

`storybook.yml` GitHub Action (see `docs/dev/04-ci-cd.md`):

- Trigger: push to `main` with changes in `design-system/`.
- Runs `pnpm build-storybook`.
- Deploys `design-system/storybook-static/` to GitHub Pages.
- URL: `https://MAECLY.github.io/autostand/storybook/`.

This means every merged PR updates the live Storybook — designers can always see the latest state.

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

1. Create the component (e.g., `design-system/components/pagination.tsx`).
2. Create the story (`design-system/components/pagination.stories.tsx`).
3. Run `pnpm storybook` — the story appears in the sidebar.
4. Verify in light + dark mode (use the backgrounds toolbar).
5. Add `tags: ["autodocs"]` for a docs page.
6. Commit both files in the same PR.

## Storybook + Tauri

Storybook runs in the browser (Vite), NOT in Tauri. App components that call `invoke` use mock callbacks in stories (no-ops or `console.log`). This keeps Storybook fast and dependency-free.

The real Tauri integration is tested in E2E tests (see `docs/dev/03-testing.md`), not Storybook.