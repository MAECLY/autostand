# Troubleshooting (End User)

Common problems and their fixes. If your issue isn't here, check the [GitHub Issues](https://github.com/MAECLY/autostand/issues) or open a new one.

## Problem / solution table

| Problem | Cause | Fix |
|---------|-------|-----|
| **No standup generated** | Scheduler not installed / not running | Settings → Scheduler → Install. Or run manually via Dashboard → "Compile now". Check the scheduler isn't disabled. |
| **Empty standup** | No git commits + no notes in the compile window | Check `GITHUB_DIR` is correct and contains repos with `.git`. Verify your git author matches `STANDUP_AUTHORS` (Settings). Confirm the time window covers your work. |
| **LLM render falls back to deterministic** | LLM CLI not found / timeout / API error | Settings → Providers → Test. Check the CLI path or API key. Increase timeout. If using Ollama, ensure the model is pulled (`ollama pull llama3.1`). |
| **Phantom bullets in audit** | Note claims committed work on the wrong day | Expected behavior — the audit catches it. Review the note for date SKEW. If the commit exists but on another day, the note is misdated. |
| **Two-machine conflict markers** | Union merge driver not configured | Ensure `.gitattributes` in the dailies repo has `20YY-MM-DD.md merge=union`. The app auto-adds this (self-heal) if missing. |
| **Host slug is numeric/IP** | DHCP-assigned hostname | Settings → Paths → set host slug override manually (e.g., `desk`, `laptop`). |
| **TCC permission denied (macOS)** | App can't read `~/Documents/` or `~/.claude/` | System Settings → Privacy & Security → Full Disk Access → add autostand. Restart the app. |
| **`gh` CLI auth error** | Not logged in or token expired | Open a terminal and run `gh auth login`. Verify with `gh auth status`. |
| **Secrets in standup** | Redaction missed a pattern | Report the pattern (open an issue with the redacted example). Redaction is defense-in-depth — also avoid typing secrets in notes. |
| **Stale lock** | Previous run crashed mid-compile | App auto-clears locks older than 10 minutes. If it doesn't, delete `<state_dir>/.lock/` manually (see log location below). |
| **OpenCode SQLite locked** | OpenCode is running | Normal — the cache will retry on the next compile. Or close OpenCode before compiling. |
| **Dailies repo push fails** | Network down / no remote / auth | Run `git push` in the dailies dir from a terminal to see the error. The dailies repo must have a remote configured (`git remote add origin <url>`). |
| **App won't launch (macOS)** | Gatekeeper blocking unsigned app | Right-click the app → Open → confirm. Or System Settings → Privacy & Security → "Open Anyway". |
| **App won't launch (Windows)** | SmartScreen warning | Click "More info" → "Run anyway". Future releases will be codesigned. |
| **Compile takes > 60s** | Large repo or slow LLM | Normal for big repos or slow CLIs. Increase LLM timeout (Settings → Providers). The app falls back to deterministic render after timeout. |
| **Duplicate bullets** | Accumulate re-injection + new commit | Expected — accumulate preserves prior MANUAL notes. If a note duplicates an AUTO bullet, the scrub phase de-dupes via textsim. |
| **Audit sidecar missing** | Compile failed before audit step | Check logs (see below). The audit is written last; a failure mid-compile means no sidecar. |
| **Auto-update fails** | Network / signature mismatch | Download the latest release manually from GitHub Releases. Check the updater pubkey in `tauri.conf.json` matches the release signing key. |

## Log location

Every compile writes a log file. Check these for detailed error messages.

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/autostand/state/logs/` |
| Linux | `~/.local/share/autostand/state/logs/` |
| Windows | `%LOCALAPPDATA%\autostand\state\logs\` |

Log file naming: `run-<timestamp>-<source>-<pid>.log`

Example: `run-20260803T140000-scheduler-12345.log`

Each log contains:
- Config snapshot (with secrets redacted)
- Per-source gather results (counts, durations, errors)
- Scrub decisions (what was filtered and why)
- Render attempt (provider, model, prompt size, duration, success/fallback)
- Write result (file path, commit SHA, push status)
- Audit summary (bullet count, phantom count)

## Debug mode

For deeper visibility:

1. **Settings → Advanced → Debug logging: on** (sets tracing level to `DEBUG`).
2. **Dashboard → Debug page** (appears when debug is on):
   - **Gather preview** — shows raw facts/notes/enrichment per source **before** render. Inspect what each source contributed.
   - **Scrub log** — what the scrub filter removed and why.
   - **Render prompt** — the full prompt sent to the LLM (truncated if huge).
   - **Render response** — the LLM's raw output before parsing.
   - **Audit trace** — per-bullet provenance resolution.

Debug logs are verbose — turn off when not needed to save disk.

## Common fixes

### Re-detect providers

If a CLI was installed after autostand was launched:
- Settings → Providers → click **Re-detect** (or restart the app).
- The app scans `PATH` for CLIs at launch and on re-detect.

### Reset host slug

If your host slug is wrong or you want to change it:
1. Settings → Paths → host slug override → type new slug.
2. Click **Save**.
3. Next compile uses the new slug. Old AUTO blocks keep their old slug (they're history).

### Clear cache

If gather results seem stale (e.g., a commit isn't showing):
1. Settings → Advanced → **Clear cache**.
2. Next compile re-gathers from scratch (slower, but fresh).

Cache TTL is 1 hour by default. Stale cache is rarely the issue, but clearing it is a safe troubleshooting step.

### Reinstall scheduler

If the scheduler isn't firing:
1. Settings → Scheduler → **Uninstall system scheduler**.
2. Restart the app.
3. Settings → Scheduler → **Install system scheduler**.
4. Verify "Next run" shows a future time.

On macOS, verify with `launchctl list | grep autostand`.
On Linux, verify with `systemctl --user status autostand`.
On Windows, verify in Task Scheduler.

## Support

- **GitHub Issues:** [https://github.com/MAECLY/autostand/issues](https://github.com/MAECLY/autostand/issues)
- **Bug reports:** include the log file (or its last 100 lines), your OS, autostand version (Help → About), and the config (with secrets redacted).
- **Feature requests:** welcome — describe the workflow you want.

## Data safety

autostand is designed to never lose data:
- **Atomic writes** — standup files are written to a temp file, then renamed. A crash mid-write leaves the old file intact.
- **Accumulate never-delete** — prior MANUAL notes are re-injected on every recompile. Nothing you've added is ever removed.
- **Union merge** — two-machine writes coexist; no AUTO block is overwritten.
- **Audit sidecar** — every bullet's provenance is recorded; you can always verify what backed each claim.

If you ever suspect data loss:
1. Check `git log` in the dailies repo — all compiles are committed.
2. Check the audit sidecar for the date in question.
3. Check the log file for that compile.