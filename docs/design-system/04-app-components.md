# App Components

App components are composite, autostand-specific components in `design-system/app-components/`. They compose base components (see `docs/design-system/03-components.md`) and add business logic — Tauri `invoke` calls, data fetching, config read/write.

## App components

### `StandupPreview`

Renders a `StandupFileContent` as formatted Markdown with syntax highlighting for AUTO/MANUAL blocks.

```tsx
interface StandupPreviewProps {
  content: StandupFileContent;  // { date, autoBlocks: AutoBlock[], manual: ManualRegion }
  hostSlug?: string;            // highlight this host's block
}
```

- Renders each AUTO block in a `Card` with the host slug as a `Badge`.
- Renders the MANUAL region in a `Card` with amber accent.
- Uses `react-markdown` for rendering.
- Code/SHA spans use `JetBrains Mono`.

### `AutoBlockView`

Renders one host's AUTO block with edit/preview toggle.

```tsx
interface AutoBlockViewProps {
  block: AutoBlock;             // { hostSlug, date, bullets: Bullet[] }
  editable?: boolean;           // default false (history is read-only)
  onEdit?: (block: AutoBlock) => void;
}
```

- Preview mode: rendered Markdown.
- Edit mode: `Textarea` with raw Markdown.
- Host slug shown as `Badge`.

### `ManualEditor`

Textarea + preview for the MANUAL region; "Add" button calls `add_manual_item` (Tauri IPC).

```tsx
interface ManualEditorProps {
  date: string;                 // "2026-08-03"
  initialContent?: string;
}
```

- `Textarea` for the note.
- Date selector: Today / Tomorrow (`Select`).
- "Add" button → `invoke("add_manual_item", { date, item })` → toast (Sonner) on success.
- Preview pane shows how the note will look in the file.

### `ProviderCard`

Shows provider status (CLI detected, API key set, model, mode toggle, Test button).

```tsx
interface ProviderCardProps {
  provider: ProviderConfig;     // { id, name, cliDetected, cliPath, cliVersion, apiKeySet, models, model, mode, timeout }
  onTest: () => Promise<TestResult>;
  onSaveKey: (key: string) => Promise<void>;
  onSetMode: (mode: ProviderMode) => void;
  onSetModel: (model: string) => void;
  isPreferred: boolean;
  onSetPreferred: () => void;
}
```

- `Card` with provider name + icon.
- CLI status: green check or gray X (`Badge`).
- API key: `Input` (password type) + "Save to keychain" button.
- Model: `Select` (populated from `provider.models`).
- Mode: `Select` (CLI-first / CLI-only / API-only).
- Timeout: `Input` (number, seconds).
- Test button → calls `onTest` → shows result `Badge`.
- Preferred: radio button.

### `DataSourceToggle`

Switch + config expand (per source).

```tsx
interface DataSourceToggleProps {
  source: DataSourceConfig;     // { id, name, enabled, config: Record<string, any> }
  onToggle: (enabled: boolean) => void;
  onConfigChange: (config: Record<string, any>) => void;
}
```

- `Switch` to enable/disable.
- Expandable config section (`Collapsible`):
  - GitHub: reviewer login, org, max PRs, comment length, include self-reviews.
  - Claude Code: path to `.claude/projects/`.
  - etc.
- `Badge` showing "Detected" or "Not found" (checks path existence).

### `PathInput`

File picker + validate button + status indicator.

```tsx
interface PathInputProps {
  label: string;
  value: string;
  onChange: (path: string) => void;
  validate?: (path: string) => Promise<ValidationResult>;
  placeholder?: string;
}
```

- `Input` for the path.
- "Browse" button → native file picker (`@tauri-apps/api/dialog`).
- "Validate" button → calls `validate` → shows green ✓ or red ✗ with error.
- Remembers last validated state.

### `PipelineProgress`

Step indicator (Gathering → Scrubbing → Rendering → Writing → Done) with percentage.

```tsx
interface PipelineProgressProps {
  status: PipelineStatus;       // "idle" | "gathering" | "scrubbing" | "rendering" | "writing" | "done" | "error"
  progress?: number;            // 0–100
  error?: string;
}
```

- 5-step horizontal indicator (`Progress` + step labels).
- Current step highlighted with `--brand-primary`.
- Completed steps: green check.
- Error: red `Alert` with the error message.

### `AuditBadge`

Colored badge for audit classification.

```tsx
interface AuditBadgeProps {
  classification: "commit" | "github" | "review" | "note" | "phantom" | "unverified";
  size?: "sm" | "default";
}
```

| Classification | Color token | Bg token |
|----------------|-------------|----------|
| `commit` | `--audit-commit` | `--audit-commit-bg` |
| `github` | `--audit-github` | `--audit-github-bg` |
| `review` | `--audit-review` | `--audit-review-bg` |
| `note` | `--audit-note` | `--audit-note-bg` |
| `phantom` | `--audit-phantom` | `--audit-phantom-bg` |
| `unverified` | `--audit-unverified` | `--audit-unverified-bg` |

Built on the `Badge` base component with custom variants.

### `AuditViewer`

Table of AUTO bullets with `AuditBadge` + expandable sidecar JSON.

```tsx
interface AuditViewerProps {
  date: string;
  audit: AuditSidecar;          // { date, entries: AuditEntry[] }
}
```

- `Table` with columns: Bullet, Classification (`AuditBadge`), Source, Expand.
- Expand a row → shows the matching source (commit SHA + link, PR URL, review link, note text).
- Phantom rows (red badge) highlighted with `--audit-phantom-bg` row background.
- Empty state: `Alert` "No audit sidecar for this date."

### `SchedulerControl`

Enable toggle + cron input + next-run display + "Run now" button.

```tsx
interface SchedulerControlProps {
  enabled: boolean;
  cron: string;
  nextRun: string | null;       // ISO timestamp or null
  selfHeal: boolean;
  onToggle: (enabled: boolean) => void;
  onCronChange: (cron: string) => void;
  onSelfHealChange: (enabled: boolean) => void;
  onInstall: () => Promise<void>;
  onUninstall: () => Promise<void>;
  onRunNow: () => Promise<void>;
}
```

- `Switch` for enable.
- `Input` for cron (with helper text showing human-readable schedule).
- Next run: `Text` with formatted timestamp.
- `Switch` for self-heal.
- Buttons: "Install system scheduler", "Uninstall system scheduler", "Run now".

### `HostSlugDisplay`

Shows current host slug + override input + "detect" button.

```tsx
interface HostSlugDisplayProps {
  currentSlug: string;
  detectedSlug: string;
  onOverride: (slug: string) => void;
  onDetect: () => Promise<string>;
}
```

- Shows current slug as `Badge`.
- Shows detected slug as muted text.
- `Input` for override.
- "Detect" button → calls `onDetect` → updates detected slug.
- "Save" button → calls `onOverride`.
- Validation: rejects numeric / IP-like slugs, shows error `Alert`.

### `CalendarPicker`

Date picker for History page.

```tsx
interface CalendarPickerProps {
  selectedDate: string | null;  // "2026-08-03" or null
  datesWithFiles: string[];     // ["2026-08-03", "2026-08-02", ...]
  onSelect: (date: string) => void;
}
```

- Month grid with dots on dates that have standup files.
- Click a date → calls `onSelect`.
- Keyboard navigable (arrow keys).

### `StatusBar`

Bottom bar: scheduler status, last run, provider, host slug.

```tsx
interface StatusBarProps {
  schedulerEnabled: boolean;
  lastRun: string | null;       // ISO timestamp
  provider: string;             // "claude" | "ollama" | ...
  hostSlug: string;
  status: PipelineStatus;
}
```

- Left: scheduler status (icon + "Scheduler: on/off").
- Center: last run timestamp (relative, e.g., "2h ago").
- Right: provider + host slug + current status (with colored dot).

### `QuickAddDialog`

Modal for "add to my standup" (textarea + date selector + submit).

```tsx
interface QuickAddDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  onSubmit: (item: string, date: string) => Promise<void>;
}
```

- `Dialog` with `Textarea` for the note.
- `Select` for date (Today / Tomorrow).
- "Add" button → calls `onSubmit` → closes dialog + toast.
- Global hotkey (`Cmd/Ctrl+Shift+S`) opens it.

## Storybook stories

Each app component has stories in `design-system/app-components/*.stories.tsx` with mock data:

```tsx
// audit-badge.stories.tsx
import type { Meta, StoryObj } from "@storybook/react";
import { AuditBadge } from "./audit-badge";

const meta = {
  title: "App Components/AuditBadge",
  component: AuditBadge,
  parameters: { layout: "centered" },
} satisfies Meta<typeof AuditBadge>;
export default meta;

export const Commit: Story = { args: { classification: "commit" } };
export const Github: Story = { args: { classification: "github" } };
export const Review: Story = { args: { classification: "review" } };
export const Note: Story = { args: { classification: "note" } };
export const Phantom: Story = { args: { classification: "phantom" } };
export const Unverified: Story = { args: { classification: "unverified" } };
```

Mock data lives in `design-system/app-components/__mocks__/`:
- `mockStandupFile.ts` — sample `StandupFileContent` with 2 AUTO blocks + MANUAL region.
- `mockProviderConfig.ts` — sample `ProviderConfig` per provider (detected/not, key set/not).
- `mockPipelineStatus.ts` — sample `PipelineStatus` for each state.
- `mockAuditSidecar.ts` — sample `AuditSidecar` with one of each classification.

Stories use mock data (no Tauri invoke in Storybook — callbacks are no-ops or `console.log`).