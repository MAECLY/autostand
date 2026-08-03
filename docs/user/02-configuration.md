# Configuration (End User)

autostand is configured via the **Settings** page in the app. All settings persist to a config file in your state dir and can be overridden by env vars (for headless/CLI mode).

## Settings page walkthrough

Open Settings from the sidebar (gear icon) or `Cmd/Ctrl + ,`.

### Providers tab

Five cards — one per LLM provider:

| Card | CLI detected | API key | Models | Mode | Test |
|------|--------------|---------|--------|------|------|
| **Claude** | `claude` CLI path + version | "Set" / "Not set" | `sonnet`, `opus`, `haiku` (from `claude --help`) | CLI-first / CLI-only / API-only | ✓ / ✗ |
| **Ollama** | `ollama` CLI path + version | N/A (local) | `llama3.1`, `qwen2.5`, etc. (from `ollama list`) | CLI-only (no API) | ✓ / ✗ |
| **OpenAI/Codex** | `codex` CLI path + version | "Set" / "Not set" | `gpt-4o`, `o1`, etc. | CLI-first / CLI-only / API-only | ✓ / ✗ |
| **Gemini** | `gemini` CLI path + version | "Set" / "Not set" | `gemini-2.0-flash`, `gemini-1.5-pro` | CLI-first / CLI-only / API-only | ✓ / ✗ |
| **Grok** | `grok` CLI path + version | "Set" / "Not set" | `grok-2`, `grok-3` | CLI-first / CLI-only / API-only | ✓ / ✗ |

Per-card fields:
- **CLI detected?** — auto-detected via `which`. Shows path + `--version` output.
- **API key status** — "Set in keychain" or "Not set". Click to enter.
- **Model dropdown** — populated from the CLI (or a static list for API mode).
- **Mode toggle** — CLI-first (default), CLI-only, API-only.
- **Timeout** — seconds before falling back to deterministic render.
- **Test button** — sends a ping, shows "OK" or the error.

Set one provider as your **preferred provider** (radio at top of tab). This is the one the compile uses.

### Data Sources tab

Eight toggles:

| Source | Toggle | Config (when on) |
|--------|--------|------------------|
| **local-git** | Always on (disabled toggle) | None (uses `GITHUB_DIR`) |
| **github** | On/Off | Reviewer login, org, max PRs (default 50), comment length (default 200 chars), include self-reviews (default off) |
| **claude-code** | On/Off | Path to `.claude/projects/` (default `~/.claude/projects/`) |
| **remember** | On/Off | Path to `.remember/` (default `~/.remember/`) |
| **opencode** | On/Off | Path to OpenCode DB (default `~/.config/opencode/`) |
| **codex** | On/Off | Path to `.codex/` (default `~/.codex/`) |
| **gemini-cli** | On/Off | Path to Gemini history |
| **grok-cli** | On/Off | Path to Grok history |

**GitHub section** (when enabled):
- **Reviewer login** — your GitHub username (for filtering reviews you wrote).
- **Org** — optional, filter to one org's repos.
- **Max PRs** — how many recent PRs to fetch (default 50).
- **Comment length** — truncate review comments longer than this (default 200 chars).
- **Include self-reviews** — if on, reviews on your own PRs are included (default off).

### Paths tab

| Field | Default | Description |
|-------|---------|-------------|
| `GITHUB_DIR` | `~/Documents/Github` | Where your work repos live |
| Dailies dir | `<install>/dailies/` | Where standup `.md` files are written |
| Jira base URL | (empty) | Prefix for Jira ticket links |
| Host slug override | (auto-detected) | Override the auto-detected host slug |

**Validate button** — checks each path exists and is readable. Shows green ✓ or red ✗ with the error.

**Host slug** — auto-detected on first run from the machine hostname. Can override manually. Must be stable across runs (persisted in config). Rejects numeric or IP-like slugs (e.g., `192.168.1.5` → rejected).

### Scheduler tab

| Field | Default | Description |
|-------|---------|-------------|
| Enable scheduler | Off | Master toggle |
| Cron expression | `0 7-19 * * 1-5` | Hourly 07:00–19:00, weekdays |
| Self-heal | On | Auto-clear stale locks, fix missing `.gitattributes` |
| Next run | (computed) | Shows the next scheduled compile |
| Install system scheduler | — | Button: installs launchd/systemd/Task Scheduler entry |
| Uninstall system scheduler | — | Button: removes the scheduler entry |
| Run now | — | Button: triggers an immediate compile (same as Dashboard → Compile now) |

### Scrub tab

Advanced settings for the anti-backdate scrub:

| Field | Default | Description |
|-------|---------|-------------|
| Alias scrub | On | Detect notes that restate committed work under a different wording |
| Alias scrub min tokens | 3 | Minimum tokens to match (lower = more aggressive, more false positives) |
| Meta-extra regex | (empty) | Advanced: custom regex for additional meta-work patterns to filter |

Most users leave these at defaults. The Scrub tab is collapsed under "Advanced" by default.

## API key storage

API keys are stored in the **OS keychain** — never written to config files or logs.

| Platform | Keychain |
|----------|----------|
| macOS | Keychain (Service: `autostand`, Account: `<provider>_api_key`) |
| Windows | Credential Manager |
| Linux | libsecret / GNOME Keyring / KWallet (via `keyring` crate) |

To set a key:
1. Settings → Providers → pick a card.
2. Click the API key field.
3. Type the key.
4. Click **Save to keychain**.

To clear a key:
1. Click the field.
2. Delete the text.
3. Click **Save to keychain** (saves empty → key removed).

Keys are read at compile time and passed to the LLM provider in-memory. They never appear in logs, audit sidecars, or standup files.

## Env var overrides

All config can be overridden by env vars (for headless/CLI mode, or CI). Env vars take precedence over the config file.

| Env var | Overrides | Type |
|---------|-----------|------|
| `GITHUB_DIR` | Paths → `GITHUB_DIR` | path |
| `DAILIES_DIR` | Paths → dailies dir | path |
| `JIRA_BASE_URL` | Paths → Jira base URL | URL |
| `STANDUP_HOST_SLUG` | Paths → host slug override | string |
| `STANDUP_AUTHORS` | git authors (pipe-separated) | regex |
| `PREFERRED_PROVIDER` | preferred LLM provider | `claude`/`ollama`/`openai`/`gemini`/`grok` |
| `LLM_TIMEOUT_SECS` | LLM timeout | int |
| `GITHUB_REVIEWER` | GitHub reviewer login | string |
| `GITHUB_ORG` | GitHub org filter | string |
| `GITHUB_MAX_PRS` | Max PRs to fetch | int |
| `SCHEDULER_CRON` | Scheduler cron expression | cron string |
| `LOG_LEVEL` | Tracing log level | `error`/`warn`/`info`/`debug`/`trace` |

Example (headless compile):
```bash
GITHUB_DIR=~/work DAILIES_DIR=~/dailies PREFERRED_PROVIDER=ollama \
  autostand --compile
```

## Host slug

The host slug identifies which machine wrote an AUTO block. It appears in the standup file as:
```markdown
<!-- AUTO:HOSTNAME 2026-08-03 -->
```

- **Auto-detected** on first run from `hostname` (sanitized: lowercase, non-alphanumeric stripped).
- **Persisted** in config — stable across runs even if hostname changes.
- **Override** in Settings → Paths → host slug override.
- **Rejected if numeric or IP-like** (e.g., `192`, `192.168.1.5`) — falls back to override or prompts user to set one.
- **Used for two-machine sync** — each machine has its own slug; union merge prevents conflicts.

If you run autostand on two machines, give each a distinct, memorable slug (e.g., `desk` and `laptop`).

## Config file location

The config file (JSON or TOML) lives in your state dir:

| Platform | Path |
|----------|------|
| macOS | `~/Library/Application Support/autostand/config.json` |
| Linux | `~/.config/autostand/config.json` |
| Windows | `%APPDATA%\autostand\config.json` |

You normally don't edit this directly — use Settings. But for headless/CLI mode, you can pre-create it or use env vars.