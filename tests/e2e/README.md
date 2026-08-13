# E2E — app UI against a mocked Tauri IPC

Playwright specs that drive the **real** autostand frontend in Chromium with the
Tauri IPC boundary replaced by a scripted backend.

```bash
pnpm test:e2e                       # from the repo root
pnpm --filter autostand-app exec playwright test --ui   # interactive
```

Config: [`apps/autostand-app/playwright.config.ts`](../../apps/autostand-app/playwright.config.ts).
It starts `pnpm dev` (Vite on `http://localhost:1420`), reuses an already-running
dev server locally, and always starts a clean one on CI.

## What runs for real

Everything above `invoke`. TanStack Router does the routing, TanStack Query does
the caching and invalidation, sonner renders the toasts, the design system
renders the components, Tailwind produces the styles. The specs click, type and
assert against that live application.

## What is faked, and how

`@tauri-apps/api/mocks` — the genuine `mockIPC`, loaded straight out of the app's
`node_modules` — intercepts `invoke` and the event plugin before the app boots.
Three init scripts run in order (see [`support/fixtures.ts`](support/fixtures.ts)):

1. a two-line CommonJS shim, so the mocks build has an `exports` object to
   write to;
2. `@tauri-apps/api/mocks` itself, as a classic script;
3. `installMockBackend`, which hands `mockIPC` a dispatcher for the 28 IPC
   commands and publishes `window.__E2E__` for the test helpers.

The fake backend ([`support/mock-backend.ts`](support/mock-backend.ts)) is a real
little state machine, not a lookup table: `toggle_data_source` mutates the source
list, `add_manual_item` appends to the MANUAL region, `set_scheduler_schedule`
rewrites the cron, and the `pipeline-*` events a spec emits are mirrored into
`get_pipeline_status` so a cache invalidation refetches something consistent.
An unlisted command **rejects** rather than resolving `undefined`, so a renamed
command fails a spec instead of quietly emptying the UI.

Seed data lives in [`support/scenario.ts`](support/scenario.ts) and is typed
against `apps/autostand-app/src/lib/types.ts`, so a change to the frozen IPC
contract breaks the fixtures rather than producing payloads the UI cannot read.

Two things are pinned so the suite is reproducible on any machine, on any day:
the browser clock (frozen to `2026-08-03T12:00:00Z`) and the timezone (`UTC`).

## What these specs cover

| Spec | Journey |
|------|---------|
| `dashboard.spec.ts` | Today's standup loads; one card per AUTO block; the MANUAL region stays visible and labelled "never overwritten" |
| `compile.spec.ts` | "Compile now" disables, streams `pipeline-*` progress, then shows the result; a failure toasts without wiping the previous standup |
| `settings.spec.ts` | A data-source toggle survives a navigation round trip; `local-git` is pinned on with its reason; a provider test reports transport, latency and failure |
| `audit.spec.ts` | One sidecar per host is listed and the first opens automatically; another host's opens on demand; all six classification badges render, phantom included |
| `navigation.spec.ts` | All five routes reachable from the sidebar; the status bar tracks idle → running → error, including across a route change; a `scheduler-tick` refreshes the next scheduled run |
| `empty-state.spec.ts` | A date with no standup file shows the empty state, not a crash — and a genuine read failure is told apart from it |

## What these specs do **not** cover

**No Rust runs here.** Nothing in this directory proves the pipeline works. The
backend is a fixture; if the Rust side changed its behaviour but kept its DTO
shapes, every spec here would still pass.

In particular, these specs say nothing about:

- gather, scrub, redact, accumulate, the deterministic renderer, or any LLM
  adapter;
- anti-backdating, phantom classification, or skew detection — the audit specs
  render a sidecar the fixture wrote, they do not compute one;
- host slug stability, atomic write-then-rename, file locking, or the union
  merge driver;
- the scheduler actually installing a launchd/systemd/Task Scheduler unit;
- anything about the packaged desktop app: window creation, the tray, the
  updater, permissions, or the Tauri capability allowlist.

All of that is covered by the Rust suite (`cargo test --workspace`) and the
integration tests it owns. This suite is the layer above: it proves the UI wired
to those commands behaves correctly, which the Rust tests cannot see.

**Why not drive the real app?** A true Tauri E2E needs `tauri-driver`, a compiled
platform binary and a display server — none of which a plain CI runner has, and
none of which would make the failures above visible anyway. Mocking IPC buys a
suite that runs in seconds on every push, at the cost of not testing the IPC
implementation itself. The Rust command handlers are unit-tested on their side of
that boundary; the DTO shapes are what hold the two halves together, which is why
the fixtures here are typed against the real contract.

## Adding a spec

```ts
import { expect, test } from "./support/fixtures";
import { makeScenario } from "./support/scenario";

test("does the thing", async ({ page, app }) => {
  const scenario = makeScenario();
  scenario.state.standups = {};          // tailor the backend
  scenario.defer = ["trigger_run_now"];  // hold a command in flight
  await app.start(scenario, "/settings");

  // …click, then assert
  await expect(app.callsTo("get_config")).resolves.toHaveLength(1);
});
```

The `app` fixture exposes `start`, `emit` / `pipelineStarted` / `pipelineProgress`
/ `pipelineDone` / `pipelineError`, `settle`, `fail`, `callsTo` and `patchState`.
Prefer asserting on what the user sees; use `callsTo` when the point of the test
is *which* command the UI issued.

## Layout notes

`tests/` is not a pnpm workspace package, so nothing is linked into
`tests/node_modules`. Two small files bridge that:

- `package.json` sets `type: commonjs`, making this directory a CJS island — the
  repo root is `type: module`, and ESM resolution cannot follow a tsconfig path
  mapping into another package's `node_modules`;
- `tsconfig.json` maps `@playwright/test` to `apps/autostand-app/node_modules`.

Traces, screenshots and the HTML report land in `.artifacts/` (git-ignored).
