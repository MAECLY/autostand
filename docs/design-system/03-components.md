# Base Components (shadcn/ui)

Base components are the presentational primitives — shadcn/ui components installed into `design-system/components/`. They're pure UI: no business logic, no Tauri invoke calls, no state management beyond local UI state. App components (see `docs/design-system/04-app-components.md`) compose these and add logic.

## Base components

All live in `design-system/components/`. Each is a shadcn/ui component (Radix UI primitive + Tailwind styling) customized to use our design tokens.

### Button

`design-system/components/button.tsx`

Variants:
| Variant | Use | Token |
|---------|-----|-------|
| `default` | Primary action (Compile now, Save) | `--brand-primary` bg, `--fg-on-brand` text |
| `destructive` | Delete, uninstall | `--status-error` bg |
| `outline` | Secondary action (Cancel) | `--bg-surface` bg, `--border-default` border |
| `secondary` | Tertiary action | `--bg-muted` bg |
| `ghost` | No background (toolbar buttons) | transparent bg, hover `--bg-elevated` |
| `link` | Inline link button | `--brand-primary` text, underline |

Sizes:
| Size | Padding | Text |
|------|---------|------|
| `sm` | `--space-2` `--space-3` | `--text-sm` |
| `default` | `--space-2` `--space-4` | `--text-base` |
| `lg` | `--space-3` `--space-6` | `--text-lg` |
| `icon` | square `--space-10` | (icon only) |

### Card

`design-system/components/card.tsx`

Subcomponents:
- `Card` — outer container (`--bg-surface`, `--border-default`, `--radius-lg`, `--shadow-sm`)
- `CardHeader` — top section (title + description)
- `CardTitle` — `--text-lg`, `--font-weight-semibold`
- `CardDescription` — `--text-sm`, `--fg-muted`
- `CardContent` — main body
- `CardFooter` — bottom section (actions, right-aligned)

### Dialog / Sheet

`design-system/components/dialog.tsx` (modal) and `sheet.tsx` (slide-over).

- Overlay: `--bg-base` at 50% opacity
- Content: `--bg-surface`, `--border-default`, `--radius-lg`, `--shadow-lg`
- Close button: `ghost` variant
- Used for: Quick Add dialog, settings modals, confirm dialogs

### DropdownMenu

`design-system/components/dropdown-menu.tsx`

- Trigger: any element (usually a Button)
- Content: `--bg-elevated`, `--border-default`, `--shadow-md`, `--radius-md`
- Items: hover `--bg-muted`, focus ring `--border-focus`
- Separators: `--border-default`
- Used for: provider menu, source filter, history date actions

### Input / Textarea / Label

`design-system/components/input.tsx`, `textarea.tsx`, `label.tsx`

- Input/Textarea: `--bg-surface`, `--border-default`, `--radius-md`, focus ring `--border-focus`
- Label: `--text-sm`, `--font-weight-medium`
- Used for: settings fields, Quick Add text, host slug override

### Select

`design-system/components/select.tsx`

- Trigger: like an Input + chevron icon
- Content: `--bg-elevated`, `--shadow-md`
- Used for: model dropdown, provider mode toggle, date filters

### Switch / Checkbox

`design-system/components/switch.tsx`, `checkbox.tsx`

- Switch on: `--brand-primary` bg
- Switch off: `--bg-muted` bg, `--border-strong` border
- Checkbox checked: `--brand-primary` bg, white check
- Used for: data source toggles, scheduler enable, self-heal toggle

### Tabs

`design-system/components/tabs.tsx`

- Trigger: `--fg-muted`, active `--fg-base` + bottom border `--brand-primary`
- Content: `--bg-surface`
- Used for: Settings tabs (Providers, Data Sources, Paths, Scheduler, Scrub)

### Tooltip

`design-system/components/tooltip.tsx`

- Content: `--bg-elevated` (dark), `--fg-inverse` text, `--text-xs`, `--radius-md`, `--shadow-md`
- Used for: icon button hints, audit badge explanations

### Badge

`design-system/components/badge.tsx`

Variants:
| Variant | Bg | Text | Use |
|---------|----|----|-----|
| `default` | `--brand-primary` | `--fg-on-brand` | Primary tags |
| `secondary` | `--bg-muted` | `--fg-base` | Neutral tags |
| `success` | `--status-success-bg` | `--status-success` | "Done", "OK" |
| `warning` | `--status-warning-bg` | `--status-warning` | "Partial", "Stale" |
| `error` | `--status-error-bg` | `--status-error` | "Error", "Phantom" |
| `outline` | transparent | `--fg-base` + `--border-default` | Subtle tags |

Used by: `AuditBadge` (app component), status indicators, provider status.

### Progress

`design-system/components/progress.tsx`

- Track: `--bg-muted`, `--radius-full`
- Bar: `--brand-primary`, animated width
- Used by: `PipelineProgress` (app component)

### Separator

`design-system/components/separator.tsx`

- Horizontal/vertical, `--border-default`, 1px
- Used for: card section dividers, settings section dividers

### ScrollArea

`design-system/components/scroll-area.tsx`

- Custom scrollbar (Radix), `--border-default` thumb, `--bg-muted` track
- Used for: history list, audit table, settings (long pages)

### Sonner

`design-system/components/sonner.tsx`

Toast notifications:
- Position: bottom-right (default) or bottom-center (configurable)
- Success toast: `--status-success` accent
- Error toast: `--status-error` accent
- Used for: "Compile done", "Saved to keychain", "Scheduler installed"

### Alert

`design-system/components/alert.tsx`

Variants: `default`, `destructive`, `success`, `warning`.

| Variant | Bg | Border | Icon |
|---------|----|----|------|
| `default` | `--bg-surface` | `--border-default` | info |
| `destructive` | `--status-error-bg` | `--status-error` | alert-triangle |
| `success` | `--status-success-bg` | `--status-success` | check-circle |
| `warning` | `--status-warning-bg` | `--status-warning` | alert-circle |

Used for: error banners, "no standup for this date" notices.

### Table

`design-system/components/table.tsx`

- Header: `--bg-muted`, `--text-sm`, `--font-weight-medium`, `--fg-muted`
- Rows: `--bg-surface`, hover `--bg-muted`, border-bottom `--border-default`
- Ships its own `overflow-auto` wrapper so a wide table never widens the page. That
  wrapper is `tabindex="0"`, because a scroll container only a mouse can drive is
  unreachable for a keyboard (WCAG 2.1.1, axe `scrollable-region-focusable`). Pass
  `scrollRegionLabel` to name it — it then becomes an announced `role="region"`
  instead of an anonymous tab stop.
- Used by: `AuditViewer` (app component), settings tables

### Accordion

`design-system/components/accordion.tsx`

- Trigger: `--bg-surface`, hover `--bg-muted`, chevron rotates
- Content: `--bg-surface`, `--border-default` top border
- Used for: settings sections (Scrub tab is accordion-collapsed by default)

### Collapsible

`design-system/components/collapsible.tsx`

- Trigger: any element
- Content: shown/hidden with animation
- Used for: AUTO block collapse in History view, audit bullet expansion

### Spinner

`design-system/components/spinner.tsx`

- `lucide-react` `Loader2` with `animate-spin`, sizes `sm` | `default` | `lg`
- Takes a `label` announced to screen readers (the icon itself is `aria-hidden`)
- Used for: pending queries in every page (history probes, sidecar reads, gather preview)

## Storybook stories

One `.stories.tsx` per component in `design-system/components/`:

```
design-system/components/
├── button.tsx
├── button.stories.tsx
├── card.tsx
├── card.stories.tsx
├── dialog.tsx
├── dialog.stories.tsx
├── badge.tsx
├── badge.stories.tsx
└── ...
```

Each story shows all variants + states. Example (`button.stories.tsx`):

```tsx
import type { Meta, StoryObj } from "@storybook/react";
import { Button } from "./button";

const meta = {
  title: "Components/Button",
  component: Button,
  parameters: { layout: "centered" },
  tags: ["autodocs"],
} satisfies Meta<typeof Button>;

export default meta;
type Story = StoryObj<typeof meta>;

export const Default: Story = { args: { children: "Button", variant: "default" } };
export const Destructive: Story = { args: { children: "Delete", variant: "destructive" } };
export const Outline: Story = { args: { children: "Cancel", variant: "outline" } };
export const Secondary: Story = { args: { children: "Secondary", variant: "secondary" } };
export const Ghost: Story = { args: { children: "Ghost", variant: "ghost" } };
export const Link: Story = { args: { children: "Link", variant: "link" } };
export const Small: Story = { args: { children: "Small", size: "sm" } };
export const Large: Story = { args: { children: "Large", size: "lg" } };
export const Icon: Story = { args: { size: "icon", children: <PlusIcon /> } };
export const Disabled: Story = { args: { children: "Disabled", disabled: true } };
```

**Rules:**
- Use design tokens (no hardcoded colors).
- Show every variant, size, and state (default, hover, focus, disabled).
- No business logic — pure props in/out.

## Composition rules

Base components are **presentational only**:

✅ Do:
- Accept props (variants, sizes, children, callbacks)
- Render based on props
- Local UI state (open/closed) is fine

❌ Don't:
- Call Tauri `invoke`
- Fetch data
- Read/write config
- Import app-specific types (e.g., `StandupFileContent`) — use generic props instead

App components (see `docs/design-system/04-app-components.md`) wrap base components and add:
- Tauri `invoke` calls
- Data fetching
- Config read/write
- App-specific types
- Business logic

This separation keeps base components reusable on the landing page (see `docs/design-system/06-landing-reuse.md`).