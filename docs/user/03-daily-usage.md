# Daily Usage (End User)

This guide covers the day-to-day workflow of using autostand. For setup, see `docs/user/01-install.md`. For configuration, see `docs/user/02-configuration.md`.

## Automatic mode (set and forget)

Once the scheduler is installed (Settings → Scheduler → Install), autostand runs automatically:

- **Schedule:** hourly at 07:00, 08:00, ..., 19:00 on weekdays (Mon–Fri). Default cron: `0 7-19 * * 1-5`.
- **The app doesn't need to be open.** The scheduler runs independently via:
  - macOS: `launchd`
  - Linux: `systemd`
  - Windows: Task Scheduler
- **What happens at each run:**
  1. Gather: scan git repos, GitHub PRs, Claude Code transcripts, etc. (only enabled sources).
  2. Scrub: anti-backdate filter, meta-work filter, secrets redaction.
  3. Render: send to LLM provider (or fall back to deterministic renderer).
  4. Write: atomic write to `DAILIES_DIR/<date>.md`, commit, push.
  5. Audit: write `.audit.json` sidecar.
- **Standup files appear** in your dailies dir automatically. Git commit + push happens automatically (if the dailies repo has a remote).

You'll find your standup in `DAILIES_DIR/2026-08-03.md` (today's date).

## Manual mode

If you want to trigger a compile on demand:

1. Open the app → **Dashboard**.
2. Click **Compile now**.
3. Watch the pipeline progress bar (Gathering → Scrubbing → Rendering → Writing → Done).
4. The standup preview updates when done.

Manual compile uses the same config as automatic — same sources, same provider, same window. It's not a "special" run.

## Quick Add

Quick Add lets you append a note to today's (or tomorrow's) MANUAL region **without opening the full app**.

### Open Quick Add

- **Global hotkey** (configurable in Settings → Advanced, default `Cmd/Ctrl+Shift+S`).
- **Menu bar / system tray** — click the autostand icon → "Quick Add".
- **In-app** — Dashboard → "Quick Add" button.

### Use it

1. Type your note (e.g., "attended meeting at 14:00").
2. Pick the date: **Today** or **Tomorrow** (Tomorrow is for things to mention in tomorrow's standup).
3. Click **Add** (or press `Enter`).

The note is appended to the MANUAL region of the selected date's standup file. If the file doesn't exist yet, it's created. The append is atomic (no risk of corrupting the file).

This replaces the old App Script's `add-item.sh` and the `add-to-daily-standup` skill — same behavior, cleaner UX.

## Dashboard

The Dashboard is the app's home screen.

| Section | What it shows |
|---------|---------------|
| **Today's preview** | Rendered Markdown of today's standup (auto-refreshes on compile). AUTO blocks per host + MANUAL region. |
| **Pipeline progress** | During compile: step indicator (Gathering → Scrubbing → Rendering → Writing → Done) with percentage. Idle otherwise. |
| **Status** | `idle` / `gathering` / `rendering` / `done` / `error`. Shows last run time + duration. |
| **Quick Add button** | Opens the Quick Add dialog. |
| **Compile now button** | Triggers a manual compile. |

The preview renders as the standup file will appear — same Markdown, same formatting. AUTO blocks are visually separated; MANUAL region is highlighted.

## History

Browse past standups:

1. Click **History** in the sidebar.
2. **Calendar picker** — click any date with a standup file (dots on dates with files).
3. The selected date's standup renders in the main panel.
4. **AUTO blocks** are grouped by host slug (collapsed/expanded).
5. **MANUAL region** is shown at the bottom.
6. **Raw view** toggle — switch between rendered Markdown and raw `.md` source.

Use this to review what you reported on past days, or to verify the union merge worked across two machines (you'll see two AUTO blocks for the same date).

## Audit

Verify the provenance of every bullet in your standup:

1. Click **Audit** in the sidebar.
2. **Date picker** — select a date (defaults to today).
3. The audit viewer loads the `.audit.json` sidecar for that date.
4. Each AUTO bullet is shown with a **classification badge**:

| Badge | Color | Meaning |
|-------|-------|---------|
| `commit` | green | Backed by a git commit in the window |
| `github` | blue | Backed by a GitHub PR or issue |
| `review` | purple | Backed by a PR review you wrote |
| `note` | amber | From a MANUAL note (no commit backing) |
| `phantom` | red | Claims work but no matching source — investigate |
| `unverified` | gray | Source not checked (e.g., a source was disabled) |

5. **Expand a bullet** to see the matching source (commit SHA, PR URL, review link, note text).
6. **Phantoms** (red) are the most important — they mean a note claims committed work that the audit can't verify. Either:
   - The note is wrong (you did the work on a different day → adjust the note).
   - The commit is in a repo not in `GITHUB_DIR` → add the repo.
   - The git author doesn't match your config → fix `STANDUP_AUTHORS`.

The audit is read-only — it doesn't modify the standup file. It's a verification tool.

## Settings changes

Changing providers or data sources **takes effect on the next compile** — no restart needed. The config is re-read at the start of each compile.

| Change | When it takes effect |
|--------|---------------------|
| Toggle a data source | Next compile |
| Change LLM provider | Next compile |
| Change LLM model | Next compile |
| Change `GITHUB_DIR` | Next compile |
| Change host slug | Next compile (new AUTO block uses new slug) |
| Change cron schedule | Next scheduler tick |
| Change API key | Next compile |

## Two-machine sync

If you use autostand on two machines (e.g., desktop + laptop):

1. **Both machines point `DAILIES_DIR` at the same git repo** (e.g., `~/Sync/Github_Dailies`, synced via git or a cloud service).
2. **Each machine has its own host slug** (e.g., `desk` and `laptop`). Set in Settings → Paths.
3. **Each machine writes its own AUTO block** — keyed by host slug:
   ```markdown
   <!-- AUTO:desk 2026-08-03 -->
   ## AUTO
   - (desk's commits + reviews)
   <!-- /AUTO:desk -->

   <!-- AUTO:laptop 2026-08-03 -->
   ## AUTO
   - (laptop's commits + reviews)
   <!-- /AUTO:laptop -->

   <!-- MANUAL -->
   - (manual notes, shared)
   <!-- /MANUAL -->
   ```
4. **Union merge driver** prevents conflicts — `.gitattributes` in the dailies repo has:
   ```
   20YY-MM-DD.md merge=union
   ```
   Both machines' AUTO blocks coexist; the MANUAL region unions both machines' notes.
5. **Before compiling, the app does a `git pull`** to get the other machine's latest AUTO block.
6. **After compiling, the app does a `git push`** so the other machine can pull it.

No conflict markers, no data loss. This is the same behavior as the App Script.

## Troubleshooting

See `docs/user/04-troubleshooting.md` for common problems and fixes.