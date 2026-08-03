# Testing Strategy

autostand uses three test layers matching the `tests/` directory structure. Tests are hermetic (no network, no real HOME, no real git remotes) unless explicitly marked otherwise.

## Test layers

### 1. Unit tests

**Location:** `tests/unit/` + `#[cfg(test)]` modules inside each crate (`crates/*/src/*.rs`).

Unit tests cover individual functions in isolation:

| Module | Tests |
|--------|-------|
| `dates` | `next_business_day`, `prev_business_day_before`, weekend skipping, holiday handling, timezone edge cases |
| `host` | host slug validation (rejects numeric, IP-like, empty), slug stability across runs |
| `scrub` | CLAIM regex matching, FORBIDDEN/COVERED classification, alias scrub min-token threshold |
| `textsim` | fuzzy match score, threshold tuning, edge cases (empty strings, very long strings) |
| `redact` | secrets redaction regex (API keys, tokens, passwords, emails, IP addresses) |
| `format` | file format parser (AUTO/MANUAL block detection, title/subtitle extraction) |
| `accumulate` | re-injection of prior items, never-delete invariant, dedup against AUTO |
| `deterministic` | pure-Rust renderer output (deterministic given same input), no LLM dependency |
| `render_prompt` | system prompt is loaded via `include_str!`, no runtime file read |

Run:
```bash
cargo test --workspace          # all unit tests
cargo test -p autostand-core    # core only
```

### 2. Integration tests

**Location:** `tests/integration/`

Integration tests exercise multiple crates together with mocked dependencies:

| Test | What it verifies |
|------|------------------|
| `pipeline.rs` | End-to-end compile: gather → scrub → render → write, using `MockSource` + `MockLlmAdapter` |
| `format_roundtrip.rs` | Write a standup file, read it back, parse — verify AUTO/MANUAL blocks survive |
| `audit_sidecar.rs` | Write audit sidecar JSON, read it back, verify provenance entries match bullets |
| `cache.rs` | Cache TTL behavior (fresh → hit, expired → miss → refill), disk cache persistence |
| `lock.rs` | Lock acquisition (single process succeeds, second process blocks/fails), stale lock auto-clear (>10min) |
| `merge.rs` | Union merge driver simulation (two-machine writes produce no conflict markers) |

Run:
```bash
cargo test --test '*'            # all integration tests
cargo test --test pipeline       # single test file
```

### 3. E2E tests

**Location:** `tests/e2e/`

E2E tests launch the full Tauri app via Playwright + `tauri-driver`:

```bash
pnpm test:e2e
```

| Test | What it verifies |
|------|------------------|
| `app-launch.spec.ts` | App window opens, dashboard renders, status bar shows host slug |
| `compile.spec.ts` | Configure settings → click "Compile now" → standup file written to temp DAILIES_DIR → dashboard preview updates |
| `audit.spec.ts` | After compile, navigate to Audit page → audit sidecar JSON shown with classification badges |
| `settings.spec.ts` | Change provider → save → recompile with new provider |
| `quick-add.spec.ts` | Open Quick Add → type note → submit → note appears in MANUAL region of today's file |

E2E tests use a hermetic temp HOME (see fixtures below) so they never touch real user data.

## Test fixtures

**Location:** `tests/fixtures/`

| Fixture | Description |
|---------|-------------|
| `git-repos/` | Bare git repos with known commits (used by `local_git` source). Each repo has a `README.md` documenting its commit history. |
| `claude-projects/` | Sample `.claude/projects/*/` directory with JSONL transcripts (prompts, plans, file edits) |
| `remember-notes/` | Sample `.remember/today-*.md` files for various dates |
| `opencode-db/` | Sample OpenCode SQLite DB with known sessions |
| `codex-jsonl/` | Sample Codex `.jsonl` transcripts |
| `standup-files/` | Sample daily standup `.md` files (AUTO/MANUAL blocks) for round-trip tests |
| `audit-sidecars/` | Sample `.audit.json` sidecars for audit read tests |
| `home/` | Hermetic temp HOME: `.claude/`, `.codex/`, `.config/`, `.remember/` populated for E2E |

Tests set `HOME` (or equivalent) to `tests/fixtures/home/` to ensure hermetic, reproducible runs. This mirrors the App Script's `test_provenance.sh` approach.

## Mock LLM adapter

`MockLlmAdapter` implements the `LlmAdapter` trait:

```rust
pub struct MockLlmAdapter {
    pub response: String,
    pub should_timeout: bool,
    pub should_error: bool,
    pub call_count: AtomicUsize,
}
```

- Returns canned responses (configurable per-test)
- Simulates timeout / error (to test fallback paths)
- Tracks call count (to verify provider selection)

Used in `tests/integration/pipeline.rs` and any test that exercises the render step without a real LLM CLI.

## Regression tests (ported from App Script)

The App Script had a suite of regression tests for historical bugs. These are ported to Rust to prevent regressions:

| Regression | Test file | What it checks |
|------------|-----------|----------------|
| **FIF-133 phantom** | `tests/integration/phantom_fif133.rs` | A note restating already-committed work → phantom detected by audit (note matches a commit, flagged as redundant claim) |
| **2026-07-28 two-machine corruption** | `tests/integration/two_machine_merge.rs` | Two hosts write to same date file → union merge driver produces clean union, no conflict markers |
| **Anti-backdate scrub** | `tests/unit/scrub.rs` | CLAIM regex catches notes that restate past work as today's; FORBIDDEN/COVERED classification correct |
| **Accumulate re-injection** | `tests/integration/accumulate.rs` | Prior MANUAL items re-injected on recompile; never deleted; dedup against AUTO blocks |
| **SKEW detector** | `tests/unit/skew.rs` | Notes claiming work outside the compile window → flagged |
| **Meta-work filter** | `tests/unit/meta.rs` | "Met with X", "attended standup", "read PR" → filtered as meta-work (not real work) |
| **Host slug rejection** | `tests/unit/host.rs` | Numeric hostnames, IP-like strings → rejected, fall back to override |
| **Atomic write** | `tests/integration/atomic_write.rs` | Crash mid-write (simulated by killing process) → file unchanged (no partial write, no corruption) |

## Test commands

```bash
# Rust
cargo test --workspace                    # all Rust tests
cargo test -p autostand-core --lib        # core unit tests only
cargo test --test pipeline -- --nocapture # integration test with stdout

# Frontend
pnpm test                                 # vitest unit tests
pnpm test:coverage                        # with coverage

# E2E
pnpm test:e2e                             # Playwright (launches Tauri app)
pnpm test:e2e -- --grep "compile"         # filter by name
```

## Coverage

| Tool | Scope | Command |
|------|-------|---------|
| `cargo-tarpaulin` | Rust | `cargo tarpaulin --workspace --out Html` → `tarpaulin-report.html` |
| `vitest --coverage` | Frontend | `pnpm test:coverage` → `apps/autostand-app/coverage/` |

**Target:** 80%+ coverage on `autostand-core` (the critical path). Adapters and UI have lower targets (60%+) since they're thinner.

Coverage is NOT enforced in CI (to keep PRs fast) but is reported on a nightly schedule.

## Writing new tests

### New unit test (Rust)

Add to the module being tested:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_business_day_skips_weekend() {
        let friday = NaiveDate::from_ymd_opt(2026, 8, 7).unwrap();
        assert_eq!(next_business_day(friday), NaiveDate::from_ymd_opt(2026, 8, 10).unwrap());
    }
}
```

### New integration test

Create `tests/integration/<name>.rs`:

```rust
use autostand_core::pipeline::Pipeline;
use autostand_adapters::mock::{MockSource, MockLlmAdapter};

#[tokio::test]
async fn pipeline_renders_with_mock_llm() {
    let pipeline = Pipeline::builder()
        .source(MockSource::with_fixtures("tests/fixtures/"))
        .llm(MockLlmAdapter::with_response("## Today\n- did stuff"))
        .build();
    let result = pipeline.compile(today()).await.unwrap();
    assert!(result.content.contains("did stuff"));
}
```

### New E2E test

Create `tests/e2e/<name>.spec.ts`:

```ts
import { test, expect } from '@playwright/test';

test('compile produces a standup file', async ({ page }) => {
  await page.goto('http://localhost:1420');
  await page.click('text=Compile now');
  await page.waitForSelector('text=Status: done', { timeout: 30000 });
  const file = await readDailiesDir(today());
  expect(file).toContain('## AUTO');
});
```

## Hermetic test environment

All tests run without network access. The `gh` CLI is mocked (or its output fixtures are pre-fetched). LLM CLIs are mocked via `MockLlmAdapter`. Git repos are bare local repos (no remotes). This ensures tests pass in CI, on any developer's machine, and offline.