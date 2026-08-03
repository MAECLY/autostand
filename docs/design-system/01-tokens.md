# Design Tokens

Design tokens are the single source of truth for visual style. They're defined once as CSS custom properties, consumed by Tailwind v4 and shadcn/ui, and shared between the Tauri app, Storybook, and (future) the landing page.

## Token architecture

Three layers, from concrete to abstract:

| Layer | Purpose | Example |
|-------|---------|---------|
| **Primitive** | Raw values (no semantic meaning) | `--color-blue-600: #2563eb;` |
| **Semantic** | Mapped to meaning (role in the UI) | `--brand-primary: var(--color-blue-600);` |
| **Component** | Component-specific (rarely needed if semantic is good) | `--button-primary-bg: var(--brand-primary);` |

All tokens are CSS custom properties (variables), defined in `design-system/tokens/tokens.css`. This file is the **only** place colors, spacing, typography, radius, shadow, z-index, and animation timing are defined. Nothing in the app or Storybook hardcodes these values.

## `design-system/tokens/tokens.css`

Full file content:

```css
:root {
  /* === PRIMITIVE TOKENS === */
  /* Color palette - raw values */
  --color-slate-50:  #f8fafc;
  --color-slate-100: #f1f5f9;
  --color-slate-200: #e2e8f0;
  --color-slate-300: #cbd5e1;
  --color-slate-400: #94a3b8;
  --color-slate-500: #64748b;
  --color-slate-600: #475569;
  --color-slate-700: #334155;
  --color-slate-800: #1e293b;
  --color-slate-900: #0f172a;
  --color-slate-950: #020617;

  --color-blue-50:  #eff6ff;
  --color-blue-100: #dbeafe;
  --color-blue-200: #bfdbfe;
  --color-blue-300: #93c5fd;
  --color-blue-400: #60a5fa;
  --color-blue-500: #3b82f6;
  --color-blue-600: #2563eb;
  --color-blue-700: #1d4ed8;
  --color-blue-800: #1e40af;
  --color-blue-900: #1e3a8a;
  --color-blue-950: #172554;

  --color-green-50:  #f0fdf4;
  --color-green-500: #22c55e;
  --color-green-600: #16a34a;
  --color-green-700: #15803d;

  --color-amber-50:  #fffbeb;
  --color-amber-500: #f59e0b;
  --color-amber-600: #d97706;

  --color-red-50:  #fef2f2;
  --color-red-500: #ef4444;
  --color-red-600: #dc2626;
  --color-red-700: #b91c1c;

  --color-purple-50:  #faf5ff;
  --color-purple-500: #ab7aff;
  --color-purple-600: #9333ea;

  /* === SEMANTIC TOKENS === */
  /* Background */
  --bg-base:    var(--color-slate-50);
  --bg-surface: #ffffff;
  --bg-elevated: var(--color-slate-100);
  --bg-muted:   var(--color-slate-100);
  --bg-inset:   var(--color-slate-50);

  /* Foreground / text */
  --fg-base:    var(--color-slate-900);
  --fg-muted:   var(--color-slate-500);
  --fg-subtle:  var(--color-slate-400);
  --fg-inverse: #ffffff;

  /* Border */
  --border-default: var(--color-slate-200);
  --border-strong:  var(--color-slate-300);
  --border-focus:   var(--color-blue-500);

  /* Brand */
  --brand-primary:   var(--color-blue-600);
  --brand-primary-hover: var(--color-blue-700);
  --brand-accent:    var(--color-purple-500);

  /* Status */
  --status-success: var(--color-green-600);
  --status-success-bg: var(--color-green-50);
  --status-warning: var(--color-amber-600);
  --status-warning-bg: var(--color-amber-50);
  --status-error:   var(--color-red-600);
  --status-error-bg: var(--color-red-50);
  --status-info:    var(--color-blue-600);
  --status-info-bg: var(--color-blue-50);

  /* Audit classification colors */
  --audit-commit:    var(--color-green-600);
  --audit-commit-bg: var(--color-green-50);
  --audit-github:    var(--color-blue-600);
  --audit-github-bg: var(--color-blue-50);
  --audit-review:    var(--color-purple-600);
  --audit-review-bg: var(--color-purple-50);
  --audit-note:      var(--color-amber-600);
  --audit-note-bg:   var(--color-amber-50);
  --audit-phantom:   var(--color-red-600);
  --audit-phantom-bg: var(--color-red-50);
  --audit-unverified: var(--color-slate-400);
  --audit-unverified-bg: var(--color-slate-50);

  /* Spacing scale (4px base) */
  --space-0: 0;
  --space-1: 0.25rem;  /* 4px */
  --space-2: 0.5rem;   /* 8px */
  --space-3: 0.75rem;  /* 12px */
  --space-4: 1rem;     /* 16px */
  --space-5: 1.25rem;  /* 20px */
  --space-6: 1.5rem;   /* 24px */
  --space-8: 2rem;     /* 32px */
  --space-10: 2.5rem;  /* 40px */
  --space-12: 3rem;    /* 48px */
  --space-16: 4rem;    /* 64px */

  /* Typography */
  --font-sans: "Inter", system-ui, -apple-system, sans-serif;
  --font-mono: "JetBrains Mono", "SF Mono", "Cascadia Code", monospace;
  --font-display: "Inter", system-ui, sans-serif;

  --text-xs:   0.75rem;   /* 12px */
  --text-sm:   0.875rem;  /* 14px */
  --text-base: 1rem;      /* 16px */
  --text-lg:   1.125rem;  /* 18px */
  --text-xl:   1.25rem;   /* 20px */
  --text-2xl:  1.5rem;    /* 24px */
  --text-3xl:  1.875rem;  /* 30px */
  --text-4xl:  2.25rem;   /* 36px */

  --font-weight-normal: 400;
  --font-weight-medium: 500;
  --font-weight-semibold: 600;
  --font-weight-bold: 700;

  --leading-tight: 1.25;
  --leading-normal: 1.5;
  --leading-relaxed: 1.625;

  /* Radius */
  --radius-sm: 0.25rem;
  --radius-md: 0.375rem;
  --radius-lg: 0.5rem;
  --radius-xl: 0.75rem;
  --radius-full: 9999px;

  /* Shadow */
  --shadow-sm: 0 1px 2px 0 rgb(0 0 0 / 0.05);
  --shadow-md: 0 4px 6px -1px rgb(0 0 0 / 0.1);
  --shadow-lg: 0 10px 15px -3px rgb(0 0 0 / 0.1);

  /* Z-index */
  --z-dropdown: 1000;
  --z-modal: 2000;
  --z-toast: 3000;

  /* Animation */
  --duration-fast: 150ms;
  --duration-normal: 250ms;
  --duration-slow: 400ms;
  --ease-default: cubic-bezier(0.4, 0, 0.2, 1);
}

.dark {
  --bg-base:    var(--color-slate-950);
  --bg-surface: var(--color-slate-900);
  --bg-elevated: var(--color-slate-800);
  --bg-muted:   var(--color-slate-800);
  --bg-inset:   var(--color-slate-900);

  --fg-base:    var(--color-slate-50);
  --fg-muted:   var(--color-slate-400);
  --fg-subtle:  var(--color-slate-500);
  --fg-inverse: var(--color-slate-950);

  --border-default: var(--color-slate-700);
  --border-strong:  var(--color-slate-600);
}
```

## Tailwind v4 integration

In `apps/autostand-app/src/globals.css` (or a shared `design-system/styles/theme.css`):

```css
@import "tailwindcss";
@import "../tokens/tokens.css";

@theme {
  /* Map semantic tokens to Tailwind utilities */
  --color-background: var(--bg-base);
  --color-foreground: var(--fg-base);
  --color-surface: var(--bg-surface);
  --color-elevated: var(--bg-elevated);
  --color-muted: var(--bg-muted);
  --color-inset: var(--bg-inset);

  --color-primary: var(--brand-primary);
  --color-primary-hover: var(--brand-primary-hover);
  --color-accent: var(--brand-accent);

  --color-success: var(--status-success);
  --color-warning: var(--status-warning);
  --color-error: var(--status-error);
  --color-info: var(--status-info);

  --color-border: var(--border-default);
  --color-border-strong: var(--border-strong);
  --color-ring: var(--border-focus);

  /* Audit colors */
  --color-audit-commit: var(--audit-commit);
  --color-audit-github: var(--audit-github);
  --color-audit-review: var(--audit-review);
  --color-audit-note: var(--audit-note);
  --color-audit-phantom: var(--audit-phantom);
  --color-audit-unverified: var(--audit-unverified);

  /* Fonts */
  --font-sans: var(--font-sans);
  --font-mono: var(--font-mono);
  --font-display: var(--font-display);

  /* Radius */
  --radius-sm: var(--radius-sm);
  --radius-md: var(--radius-md);
  --radius-lg: var(--radius-lg);
  --radius-xl: var(--radius-xl);

  /* Shadow */
  --shadow-sm: var(--shadow-sm);
  --shadow-md: var(--shadow-md);
  --shadow-lg: var(--shadow-lg);
}
```

Now `bg-surface`, `text-foreground`, `border-border`, `bg-primary`, `text-success`, etc. all work as Tailwind utilities, and they reference the semantic tokens (which swap in dark mode).

## Token naming convention

`<category>-<property>-<scale>`

| Category | Examples |
|----------|----------|
| `bg-` | `bg-base`, `bg-surface`, `bg-elevated`, `bg-muted`, `bg-inset` |
| `fg-` | `fg-base`, `fg-muted`, `fg-subtle`, `fg-inverse` |
| `border-` | `border-default`, `border-strong`, `border-focus` |
| `brand-` | `brand-primary`, `brand-primary-hover`, `brand-accent` |
| `status-` | `status-success`, `status-warning`, `status-error`, `status-info` (+ `-bg` variants) |
| `audit-` | `audit-commit`, `audit-github`, `audit-review`, `audit-note`, `audit-phantom`, `audit-unverified` (+ `-bg` variants) |
| `space-` | `space-0` ... `space-16` |
| `text-` | `text-xs`, `text-sm`, `text-base`, ... `text-4xl` |
| `font-` | `font-sans`, `font-mono`, `font-display` |
| `radius-` | `radius-sm`, `radius-md`, `radius-lg`, `radius-xl`, `radius-full` |
| `shadow-` | `shadow-sm`, `shadow-md`, `shadow-lg` |
| `z-` | `z-dropdown`, `z-modal`, `z-toast` |
| `duration-` | `duration-fast`, `duration-normal`, `duration-slow` |

## Dark mode

Dark mode swaps **semantic** tokens; primitive tokens are unchanged.

Two strategies (pick one, document in the app):
- **Class-based** (default): `.dark` class on `<html>` (or root element). Toggle via JS.
- **System**: `@media (prefers-color-scheme: dark)`. Auto-follows OS.

autostand uses class-based (user can toggle in Settings, independent of OS preference). The `.dark` block in `tokens.css` (above) redefines only the semantic tokens that change.

## Consuming tokens

### In CSS

```css
.my-element {
  background: var(--bg-surface);
  color: var(--fg-base);
  border: 1px solid var(--border-default);
  padding: var(--space-4);
  border-radius: var(--radius-lg);
}
```

### In Tailwind (via `@theme` mapping)

```tsx
<div className="bg-surface text-fg-base border border-border rounded-lg p-4">
  Surface card
</div>
```

### In shadcn components

shadcn components use `bg-background`, `text-foreground`, `border-border` — these map to our semantic tokens via `@theme`. No edits needed to shadcn components after `init`.

### Never hardcode

❌ Don't:
```tsx
<div className="bg-[#2563eb]">...</div>
<div style={{ color: '#475569' }}>...</div>
```

✅ Do:
```tsx
<div className="bg-primary">...</div>
<div style={{ color: 'var(--fg-muted)' }}>...</div>
```

The only exception is the primitive tokens file itself (`tokens.css`), which defines the raw values.