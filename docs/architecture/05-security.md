# 05 — Security

autostand reads git history, GitHub data, and multiple AI assistant session logs,
then sends scrubbed summaries to an LLM provider and writes files to a synced
repo. This page documents every layer of defense.

---

## Secrets redaction

Regex-based scrub applied at **two** points:

1. **Pre-LLM** — inputs are scrubbed before being sent to any provider. No secret
   ever leaves the local machine to a provider API.
2. **Pre-write** — the rendered body is scrubbed again before being written to
   disk, in case the LLM regurgitated a secret from its training data or context.

Implementation: `crates/autostand-core/src/redact.rs`. Patterns (non-exhaustive):

| Category | Pattern (illustrative) | Action |
| --- | --- | --- |
| SSH private keys | `-----BEGIN (RSA\|OPENSSH\|EC\|DSA\|PGP) PRIVATE KEY-----` | Replace block with `[REDACTED PRIVATE KEY]`. |
| PGP private keys | `-----BEGIN PGP PRIVATE KEY BLOCK-----` | Replace block. |
| GitHub tokens | `gh[pousr]_[A-Za-z0-9]{36,}` | Replace with `[REDACTED GH TOKEN]`. |
| GitHub fine-grained PATs | `github_pat_[A-Za-z0-9_]{22,}` | Replace. |
| Anthropic keys | `sk-ant-[A-Za-z0-9_-]{20,}` | Replace. |
| OpenAI keys | `sk-[A-Za-z0-9]{20,}` | Replace. |
| AWS access keys | `AKIA[0-9A-Z]{16}` | Replace. |
| Slack tokens | `xox[baprs]-[A-Za-z0-9-]{10,}` | Replace. |
| Google API keys | `AIza[0-9A-Za-z_-]{35}` | Replace. |
| JWTs | `eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}` | Replace. |
| `KEY=VALUE` env secrets | `^[A-Z_]{3,}_KEY=[^\s]{8,}$` / `^[A-Z_]{3,}_TOKEN=[^\s]{8,}$` / `^[A-Z_]{3,}_SECRET=[^\s]{8,}$` | Replace value. |
| `password:` values | `password:\s*[^\s]+` (case-insensitive) | Replace value. |
| Connection strings | `(mongodb\|postgres\|postgresql\|mysql\|redis)://[^\s]+:[^\s]+@` | Replace credentials. |

The redactor is **fail-closed**: if the regex engine errors, the input is treated
as unsanitized and the LLM call is aborted (deterministic fallback used).

---

## Never read sensitive content from AI session transcripts

Data source adapters for Claude Code, OpenCode, Codex, Gemini CLI, and Grok CLI
must **NOT** read:

- `tool_result` bodies (may contain secrets, large outputs, or user data).
- `tool_use` content bodies (the `input` payload of tool calls).
- `old_string` / `new_string` fields from Edit tool calls.
- `document`, `image`, or `attachment` blocks from transcripts.

Adapters **may** read:

- User-typed text prompts (the human's messages).
- Plan titles and summaries (Claude Code plan mode).
- File path **keys** from `tool_use` blocks (just the path string, not the
  content).

This is enforced by a shared helper in `autostand-adapters/src/sources/mod.rs` that
all session readers use; raw transcript JSON is never exposed past that helper.

---

## Anti-recursion

When autostand invokes any LLM CLI (Claude, Ollama, Codex, Gemini, Grok, or the built-in local sidecar), it sets the
environment variable `AUTOSTAND_RENDER=1` on the subprocess. Each provider's
session-end hook checks this env var and skips re-triggering a render if it is
set. In the Tauri scheduler, the trigger logic also checks `AUTOSTAND_RENDER`
before firing, so a render invoked from inside an AI session cannot recurse into
another render.

---

## File permissions

| Artifact | Permission | Rationale |
| --- | --- | --- |
| Standup file on the current machine (just written) | `0600` | Prevent other local users from reading before sync. |
| Committed standup file in the repo | `0644` | Git normalizes; needed for cross-machine sync. |
| Audit sidecar `audit/<F>-<HOST>.json` | `0600` | Contains provenance; not for other local users. |
| `state/host-id`, `state/last-*` | `0600` | Internal state. |
| `state/cache/*` | `0600` | Cached enrichment may contain summaries. |

Files are created with `OpenOptions::new().mode(0o600)` (Unix) or equivalent ACL
on Windows. Permissions are not relied upon as the only defense (redaction runs
first), but they reduce blast radius.

---

## Atomic writes

`fileops.set-auto` (in `autostand-core/src/format.rs`):

1. Write content to `<F>.tmp`.
2. `fsync(<F>.tmp)`.
3. `rename(<F>.tmp, <F>)` (atomic on POSIX; on Windows, `MoveFileExW` with
   `MOVEFILE_REPLACE_EXISTING`).
4. `fsync` the parent directory.

Never a partial write. A crash at any point leaves either the old file or the
new file, never a half-written mix.

---

## Lock

`mkdir`-based lock at `state/.lock/` with a `pid` file inside. Stale timeout is
10 minutes. See `docs/architecture/04-state-machine.md` for the state machine.

Prevents:

- Two concurrent renders on the same machine.
- A render firing while a previous render is mid-write.
- Two-machine races are handled by the union merge driver (below), not the lock
  (the lock is per-machine).

---

## Git safety

| Rule | Why |
| --- | --- |
| No `Co-authored-by` trailer | Avoids leaking the agent's identity into the user's commit history. |
| Skip files with conflict markers (`<<<<<<<`, `=======`, `>>>>>>>`) | Never commit a broken merge. |
| Abort on unmerged paths in `git status` | Same as above, earlier check. |
| Union merge driver for `*.md` in `dailies/` | Two machines editing different AUTO blocks (per-host) never produce conflict markers; the union driver concatenates both sides. |
| `git pull --rebase --autostash` before render | Pick up the other machine's commits first; autostash local uncommitted edits. |
| Push failures are non-fatal | A rejected push (non-fast-forward) just means the next run will re-sync and retry. |

`.gitattributes` in the `dailies/` repo:

```
*.md merge=union
```

---

## API key storage

In the Tauri app, API keys are stored in the **OS keychain**, never in plaintext
config files:

| Platform | Backend |
| --- | --- |
| macOS | Keychain (`Security` framework) via `tauri-plugin-stronghold` or the `keyring` crate. |
| Windows | Credential Manager via `keyring` crate. |
| Linux | libsecret / GNOME Keyring via `keyring` crate; fallback to encrypted file with `age` if no keyring daemon. |

**CLI auth is preferred.** When a provider CLI is installed and authenticated
(`claude`, `codex`, `gemini`, `gh`, `ollama`), the adapter uses the CLI's own
session and performs **no** key management. API keys are only used when the CLI
is unavailable and the user has explicitly entered a key in Settings.

Config files (`autostand.toml`) store **only** non-secret configuration: provider
selection, model name, CLI path, source toggles, scheduler cron. Never API keys.

---

## Network

- LLM API calls (fallback path) are over **HTTPS only**. The HTTP client rejects
  non-200 responses and does not follow redirects to non-HTTPS.
- CLI subprocess calls are local process invocations; the only network happens
  inside the CLI's own logic (which is the user's existing authenticated session).
- GitHub access is via the `gh` CLI using the user's OAuth token — no GitHub API
  key lives inside autostand.
- No telemetry is sent. No crash reports are sent. All logs stay local.

### Built-in local models

Model downloads are the only network operation performed by the built-in local provider. The IPC accepts a catalog id rather than an arbitrary URL, and each catalog item pins an immutable revision, exact byte size, and SHA-256. Downloads remain `.part` files until verification succeeds, then are atomically renamed under `<state_dir>/models/local`.

Inference is isolated in `autostand-local-llm`, which communicates over JSONL stdin/stdout and delegates to a sibling llama.cpp runtime. Neither process opens a listening port. Model weights are not executable by the Tauri process, and raw llama.cpp stderr is reduced to stable provider classifiers before logs, health state, notifications, or audit telemetry. See `docs/llm-adapters/06-built-in-local.md`.

---

## Supply chain

| Layer | Tool | Frequency |
| --- | --- | --- |
| Rust deps | `cargo audit` | Every CI run + weekly scheduled. |
| Rust deps | `cargo deny` (licenses + advisories + bans) | Every PR. |
| npm deps | `pnpm audit` | Every CI run. |
| npm deps | `pnpm audit --prod` + license check | Every PR. |
| Lock files | `Cargo.lock` + `pnpm-lock.yaml` committed | All dep versions pinned. |

CI fails on any advisory with a fix available. Direct git dependencies are
forbidden (only crates.io / npm registry deps allowed).

---

## Threat model

| Threat | Mitigation | Implemented in |
| --- | --- | --- |
| Secret leakage to LLM provider | Pre-LLM redaction (regex); fail-closed. | `autostand-core/src/redact.rs` |
| Secret leakage to committed file | Pre-write redaction. | `autostand-core/src/redact.rs` |
| Secret leakage via transcript read | Session reader helper blocks `tool_result`/`tool_use`/`old_string`/`new_string`/attachments. | `autostand-adapters/src/sources/mod.rs` |
| Backdated standup (notes claim committed work) | FORBIDDEN/COVERED scrub + SKEW detector + CLAIM regex drop. | `autostand-core/src/scrub.rs`, `specs/anti-backdating.md` |
| Phantom bullets (LLM invents work) | Validation: every bullet must trace to a FACT/GITHUB/NOTE; audit phantom detector. | `autostand-core/src/audit.rs`, `autostand-core/src/pipeline.rs` |
| "No work" from transient git failure | Anti-regression guard: empty FACTS after non-empty prior run → skip. | `autostand-core/src/pipeline.rs` |
| File corruption on crash | Atomic write-then-rename + fsync. | `autostand-core/src/format.rs` |
| Concurrent-write race (one machine) | mkdir lock + PID + 10min stale timeout. | `autostand-scheduler/src/lock.rs` |
| Concurrent-write race (two machines) | Union merge driver for `*.md`; per-host AUTO blocks. | `dailies/.gitattributes`, file format spec. |
| Host-slug instability (DHCP rename) | Persist once to `state/host-id`; never re-derive. | `autostand-core/src/host.rs` |
| API key theft from config file | Keys in OS keychain only; config has no secrets. | Tauri app + `keyring` crate. |
| Corrupted or replaced model download | Immutable catalog revision, exact size, SHA-256 verification, and atomic install. | `commands/local_models.rs` |
| Local inference expands GUI attack surface | Process-isolated JSONL sidecar; no listening socket; bounded generation parameters. | `autostand-local-llm`, `llm/builtin_local.rs` |
| Recursion (render triggers render) | `AUTOSTAND_RENDER=1` env guard on CLI subprocess + checked by hooks. | `autostand-adapters/src/llm/*`, `autostand-scheduler/src/triggers.rs` |
| LLM hallucination of "no work" | Deterministic render always computed; `auto` mode falls back on validation failure. | `autostand-core/src/deterministic.rs`, `pipeline.rs` |
| Stale lock blocks all runs | 10min stale timeout; reclaimed by next run. | `autostand-scheduler/src/lock.rs` |
| Push of broken merge | Skip files with conflict markers; abort on unmerged paths. | `autostand-scheduler/src/triggers.rs` |
| Supply-chain compromise | Pinned lockfiles, `cargo audit`, `pnpm audit`, no git deps. | CI (`docs/dev/04-ci-cd.md`) |
