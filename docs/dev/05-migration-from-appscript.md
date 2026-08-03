# Migration from App Script

autostand is a NEW implementation of the daily-standup automation originally built as a collection of Bash/Python scripts ("App Script"). The original lives at `~/Sync/Github_Dailies` (28 scripts). autostand does NOT modify the original — both can coexist during migration.

## Context

The App Script grew organically over months into 28 Bash/Python scripts handling gathering, scrubbing, rendering, auditing, and scheduling. It works, but:
- Hard to install (manual `setup.sh`, edits crontab/launchd)
- Hard to configure (env vars scattered, no UI)
- No Windows support
- No visual audit (text only)
- Single LLM provider (Claude CLI)

autostand reimagines this as a desktop app: same invariants, same file format, broader provider + source support, plus a UI.

## Script → Crate mapping

| App Script | autostand Crate / Module | Notes |
|------------|--------------------------|-------|
| `compile.sh` | `autostand-core::pipeline` | Main orchestrator |
| `lib.sh` | `autostand-core::{dates, host, config}` | Config + helpers |
| `lib-cache.sh` | `autostand-core::cache` | TTL cache |
| `gather-changed-files.sh` + `changed-files-format.py` | `autostand-adapters::sources::local_git` | File scope |
| `gather-github.sh` + `gather-pr-reviews.sh` + `pr-review-state.jq` | `autostand-adapters::sources::github` | GitHub via `gh` CLI |
| `gather-conversation.py` | `autostand-adapters::sources::claude_code` (prompts+plans) | Claude Code transcripts |
| `gather-claude-files.py` | `autostand-adapters::sources::claude_code` (files) | Edited file attribution |
| `scrub-notes.py` | `autostand-core::scrub` | Anti-backdate scrub |
| `standup_meta.py` | `autostand-core::meta` | Meta-work filter |
| `standup-accumulate.py` | `autostand-core::accumulate` | Never-delete re-injection |
| `textsim.py` | `autostand-core::textsim` | Shared fuzzy match |
| `redact-secrets.py` | `autostand-core::redact` | Secrets scrub |
| `fileops.py` | `autostand-core::format` + file writer | Atomic file ops |
| `write-audit.py` | `autostand-core::audit` | Provenance sidecar |
| `audit-standup.sh` + `audit-match.py` | `autostand-core::audit` (phantom detect) | Read-only audit |
| `setup.sh` / `uninstall.sh` | `autostand-scheduler` + Tauri installer | Install via app |
| `com.miguel50flowers.daily-standup.plist.tmpl` | `autostand-scheduler` (platform-specific) | launchd/systemd/Task Scheduler |
| `add-item.sh` | Tauri IPC `add_manual_item` | MANUAL region append |
| `daily-standup.command.md` | Tauri app (manual trigger button) | Slash command → UI button |
| `add-to-daily-standup.SKILL.md` | Tauri app (Quick Add dialog) | Skill → UI feature |
| `render-prompt.txt` | `autostand-core::render_prompt` (`include_str!`) | System prompt |
| (deterministic render in `compile.sh`) | `autostand-core::deterministic` | Pure-Rust fallback |

## New in autostand (not in App Script)

| Feature | Where |
|---------|-------|
| OpenCode data source | `autostand-adapters::sources::opencode` |
| Codex data source | `autostand-adapters::sources::codex` |
| Gemini CLI data source | `autostand-adapters::sources::gemini_cli` |
| Grok CLI data source | `autostand-adapters::sources::grok_cli` |
| Ollama LLM provider | `autostand-adapters::llm::ollama` |
| OpenAI API LLM provider | `autostand-adapters::llm::openai` |
| Gemini API LLM provider | `autostand-adapters::llm::gemini` |
| Grok API LLM provider | `autostand-adapters::llm::grok` |
| Tauri desktop UI | `apps/autostand-app/` |
| Settings page | Tauri app |
| Audit viewer UI | Tauri app |
| Design system + Storybook | `design-system/` |

## Config migration

Env vars are backward-compatible. The App Script's config maps directly to autostand's settings.

| App Script env var | autostand config | Migration |
|--------------------|-------------------|-----------|
| `STANDUP_AUTHORS` (pipe-separated regex) | `Vec<String>` (split on `\|`) | Auto-split on `|` |
| `GITHUB_DIR` | `GITHUB_DIR` (same name) | Direct |
| `DAILIES_DIR` | `dailies_dir` | Default changes (see below) |
| `STANDUP_HOST_SLUG` | `host_slug` (override) | Direct |
| `LLM_CLI` | `preferred_provider` + provider config | Reconfigure via UI |
| `JIRA_BASE` | `jira_base_url` | Direct |

### `DAILIES_DIR` default change

| | App Script | autostand |
|---|-----------|-----------|
| Default | `~/Sync/Github_Dailies` | `<repo>/dailies/` |

autostand's default is safer for new users (no implicit dependency on a specific dir). **Migrating users should set `DAILIES_DIR=~/Sync/Github_Dailies`** (via Settings → Paths) to keep existing git history.

## File format compatibility

autostand produces the same file format as the App Script:

```markdown
<!-- AUTO:HOSTNAME 2026-08-03 -->
## AUTO
- **feat:** added X ([abc1234](url))
- reviewed PR #42
<!-- /AUTO:HOSTNAME -->

<!-- MANUAL -->
## MANUAL
- attended planning meeting at 14:00
<!-- /MANUAL -->
```

- Same AUTO/MANUAL block structure
- Same `.gitattributes` union merge driver (`20YY-MM-DD.md merge=union`)
- Same title/subtitle format
- A standup file written by autostand is readable by the App Script and vice versa
- Both can write to the same dailies repo on the same machine (not simultaneously — different host slugs)

## Output location

| Default | When to use |
|---------|-------------|
| `<repo>/dailies/` | New users (clean start) |
| `~/Sync/Github_Dailies/` | Migrating from App Script (keeps existing git history) |

The choice is made once at first-run setup (Settings → Paths → dailies dir). Configurable later.

## Invariants preserved

autostand preserves every invariant the App Script enforces. These are covered by regression tests (see `docs/dev/03-testing.md`):

- [x] **Host slug stability** — slug is persistent across runs, rejects numeric/IP
- [x] **`next_business_day` / `prev_business_day_before`** — same date math (skip weekends, honor holidays)
- [x] **AUTO/MANUAL blocks** — same structure, same delimiters
- [x] **Union merge driver** — `.gitattributes` rule, no conflict markers on two-machine writes
- [x] **Atomic writes** — write to temp file, rename; crash leaves file unchanged
- [x] **Anti-backdating** — CLAIM regex catches notes restating past work; FORBIDDEN/COVERED classification
- [x] **Accumulate never-delete** — prior MANUAL items re-injected on recompile, never removed
- [x] **Deterministic fallback** — pure-Rust renderer when no LLM available; same input → same output
- [x] **Secrets redaction** — regex-based scrub (API keys, tokens, passwords) before render
- [x] **Audit sidecar** — `.audit.json` written alongside each standup file; provenance per bullet
- [x] **No-coauthor commits** — git commits use single author, no `Co-Authored-By` trailers
- [x] **Self-heal** — stale locks auto-cleared (>10min), missing `.gitattributes` auto-added

## Migration steps (user)

1. Install autostand (see `docs/user/01-install.md`).
2. On first-run setup, set `DAILIES_DIR` to your existing `~/Sync/Github_Dailies`.
3. Set git authors (same as `STANDUP_AUTHORS`).
4. Configure LLM provider (same CLI, or new API provider).
5. Toggle data sources (enable what you had: local-git, github, claude-code, etc.).
6. Run a manual compile (Dashboard → "Compile now").
7. Verify the standup file in your dailies repo — it should match the App Script's format.
8. Run the audit — phantoms should match what the App Script's `audit-standup.sh` found.
9. Once verified, disable the App Script's scheduler (`uninstall.sh` or remove crontab/launchd entry).
10. Enable autostand's scheduler (Settings → Scheduler → Install).

Both can run in parallel during migration (different host slugs → different AUTO blocks → union merge). Once you're confident, disable the old one.