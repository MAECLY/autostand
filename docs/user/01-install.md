# Installation (End User)

This guide is for end users installing autostand on their machine. For developer setup, see `docs/dev/01-setup.md`.

## Download

Download the latest release from the [GitHub Releases page](https://github.com/MAECLY/autostand/releases).

Choose the file for your platform:

| Platform | File | Notes |
|----------|------|-------|
| macOS (Apple Silicon) | `autostand_<version>_aarch64.dmg` | M1/M2/M3/M4 Macs |
| macOS (Intel) | `autostand_<version>_x64.dmg` | Older Intel Macs |
| Linux (Debian/Ubuntu) | `autostand_<version>_amd64.deb` | apt-based distros |
| Linux (universal) | `autostand_<version>_amd64.AppImage` | Any distro with FUSE |
| Windows | `autostand_<version>_x64.msi` | Windows 10/11 x64 |
| Windows (portable) | `autostand_<version>_x64-setup.exe` | NSIS installer |

Always download from the **latest** release to get auto-update support.

## macOS

1. Open the `.dmg` file.
2. Drag **autostand** into the **Applications** folder.
3. First launch: right-click the app → **Open** → confirm in the dialog (Gatekeeper bypass — required while the app is unsigned; future releases will be notarized).
4. macOS may prompt for **Full Disk Access** — this is needed to read:
   - `~/Documents/Github/` (your work repos)
   - `~/.claude/` (Claude Code transcripts)
   - `~/.codex/` (Codex transcripts)
   - `~/.config/opencode/` (OpenCode DB)
   - `~/Sync/Github_Dailies/` (if you point `DAILIES_DIR` here)

   Grant it: **System Settings → Privacy & Security → Full Disk Access → add autostand**.

## Linux

### Debian/Ubuntu (.deb)

```bash
sudo dpkg -i autostand_<version>_amd64.deb
# If missing deps:
sudo apt -f install
```

Launch from your app menu, or run `autostand` in a terminal.

### AppImage

```bash
chmod +x autostand_<version>_amd64.AppImage
./autostand_<version>_amd64.AppImage
```

AppImage requires **FUSE** (preinstalled on most distros). If you see "dlopen(): error loading libfuse.so.2":
```bash
sudo apt install libfuse2
```

Optional: integrate with your app menu:
```bash
sudo ./autostand_<version>_amd64.AppImage --appimage-extract
```

## Windows

1. Run `autostand_<version>_x64.msi`.
2. Follow the installer wizard.
3. **WebView2 runtime** is required (preinstalled on Windows 10/11). If missing, the installer bundles it.
4. Launch autostand from the Start menu.

If SmartScreen warns "Windows protected your PC" (because the app is unsigned): click **More info** → **Run anyway**. Future releases will be codesigned.

## First-run setup

When you launch autostand for the first time, the **Setup wizard** guides you through:

### Step 1: Set `GITHUB_DIR`

Where your work repos live (the app scans these for git commits).

- Default: `~/Documents/Github`
- Click **Browse** to pick a folder, or type the path.
- Click **Validate** to confirm the path exists and contains `.git` repos.

### Step 2: Set dailies output dir

Where daily standup `.md` files are written.

- Default: `<install>/dailies/` (a new folder created by the app)
- **Migrating from App Script?** Set this to your existing `~/Sync/Github_Dailies` to keep git history.
- The folder should be a git repo (the app commits + pushes daily).

### Step 3: Set git authors

Your email + GitHub username (pipe-separated, regex-compatible).

- Example: `miguel@example.com|miguel50flowers`
- Used to filter commits — only your commits appear in the standup.

### Step 4: Configure LLM provider

**Settings → Providers** shows 5 cards: Claude, Ollama, OpenAI/Codex, Gemini, Grok.

For each:
- The app auto-detects if the CLI is installed (shows path + version).
- Enter an API key (stored in OS keychain, never written to disk).
- Pick a model from the dropdown.
- Set mode: CLI-first (try CLI, fall back to API), CLI-only, or API-only.
- Click **Test** to verify.
- Set one as your **preferred provider**.

You only need ONE provider. The app falls back to the deterministic renderer if all providers fail.

### Step 5: Toggle data sources

**Settings → Data Sources** shows 8 toggles:

| Source | What it reads | Default |
|--------|---------------|---------|
| local-git | Git commits in `GITHUB_DIR` | On (always) |
| github | PRs, reviews via `gh` CLI | Off |
| claude-code | `~/.claude/projects/` transcripts | Off |
| remember | `.remember/today-*.md` notes | Off |
| opencode | OpenCode SQLite DB | Off |
| codex | `~/.codex/` JSONL transcripts | Off |
| gemini-cli | Gemini CLI history | Off |
| grok-cli | Grok CLI history | Off |

Enable what you have installed. The app skips sources whose files aren't found (no error).

### Step 6: Set Jira base URL (optional)

If you use Jira, set the base URL so ticket IDs in commits become links (e.g., `https://yourcompany.atlassian.net/browse/`).

### Step 7: Enable scheduler

The scheduler runs the compile hourly on weekdays (07:00–19:00 by default). The app offers to install the platform scheduler:

| Platform | Scheduler |
|----------|-----------|
| macOS | `launchd` (`.plist` in `~/Library/LaunchAgents/`) |
| Linux | `systemd` (`.service` in `~/.config/systemd/user/`) |
| Windows | Task Scheduler |

You can do this later from **Settings → Scheduler → Install system scheduler**.

## Scheduler install

The scheduler runs independently of the app — the app doesn't need to be open for compiles to happen.

- **First run:** the setup wizard offers to install it.
- **Later:** Settings → Scheduler → **Install system scheduler**.
- **Uninstall:** Settings → Scheduler → **Uninstall system scheduler**.

The scheduler calls the autostand binary (the same one the app runs) with a `--compile` flag. No daemon process is kept running — each compile is a one-shot invocation.

## Verifying the install

After setup:
1. Open the app → **Dashboard**.
2. Click **Compile now**.
3. A standup `.md` file should appear in your dailies dir within 30–60 seconds.
4. The dashboard preview should show today's standup.
5. The status bar should show "done" + last run time.

If something fails, see `docs/user/04-troubleshooting.md`.

## Updating

autostand auto-updates on launch when a new release is available (via Tauri's updater plugin). You'll see a notification with a **Restart to update** button. Click it, and the app downloads + installs the update + restarts.

To manually check for updates: **Help → Check for updates**.

To disable auto-update: **Settings → Advanced → Auto-update: off** (then download manually from GitHub Releases).