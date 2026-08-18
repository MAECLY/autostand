<!--
  A research memo, not a specification. It records what was true when it was
  written; vendor APIs, pricing tiers and consent policies move, and several
  findings here already contradict documentation that was current a year ago.
  Re-verify before acting on anything with a cost attached.

  Produced 2026-08-18 by a fan-out of 18 research agents over 521 web searches,
  followed by an adversarial pass that tried to refute every blocking claim.
  That pass overturned two of them; those corrections are marked inline.
-->

# Meeting transcripts in autostand — decision memo

**Date:** 2026-08-18 · **Scope:** letting a user fold "what was discussed in my meetings" into a daily standup, with all processing local.
**Bottom line:** ship the local-file paths first. Zoom OAuth is the only vendor integration worth the calendar time, and it is second, not first. Microsoft Graph transcripts are architecturally closed to our user. Google Meet is reachable, but only via `drive.file` + desktop Picker — never via `meetings.space.readonly`.

Where the adversarial pass contradicted the initial research, this memo follows the refutation and says so inline.

---

## 1. Feasibility table

| Provider | Verdict | What the USER must do | What WE must do | Hardest blocker |
|---|---|---|---|---|
| **Meetily** (local recorder) | **Build now** | Install Meetily, press record | Read one SQLite file, read-only, WAL-safe | No speaker attribution in the free build — cannot say "you said X" |
| **Zoom — local files** | **Build now** | Paid plan; turn on "Meeting transcript" + "Automatically generate transcripts for → All meetings"; click *Save transcript* (or have the host allow all participants to save) | Glob `~/Documents/Zoom/*/*.txt` + `closed_caption.vtt`, read-only | Free/Basic plans produce nothing at all since 18 May 2026 |
| **Teams — watched folder** | **Build now** | Be organizer/co-organizer; download transcript from Recap → `.vtt`/`.docx` | WebVTT parser + folder watcher (reused) | Manual, per meeting; attendees are view-only by default |
| **Google Meet — Drive Picker** | **Build second/third** | Paid Workspace (Business Standard+) or Workspace Individual / AI Pro; transcription on; pick the transcript Doc | `drive.file` + `trigger_onepick=true` + loopback + `files.export` | Picker re-run per batch; folder-grant cascade unproven |
| **Zoom — PKCE OAuth API** | **Build second** | Paid plan; one settings checkbox; consent in browser | Public-client PKCE + loopback + Marketplace **Unlisted** listing & review | Only covers meetings the user **hosted**; Marketplace review is weeks of calendar time |
| **Google Meet — Meet REST API** | **Do not build** | Paid Workspace | Sensitive-scope verification: verified domain, hosted privacy policy, demo video | Verification forces web infrastructure onto a no-backend app, for no extra data |
| **Microsoft Graph `callTranscript`** | **Do not build** | Be a tenant admin, or beg IT for two changes | Public-client PKCE + per-tenant support docs | `EnableGraphTranscriptAccess` is **off by default in every tenant** since ~29 Jul 2026 → `403 GraphAccessToTranscriptsDisabled`, "no request-side workaround" |
| **Local capture + local ASR** | **Strategic option** | Grant mic/system-audio permission | ScreenCaptureKit/CoreAudio tap + WASAPI loopback + whisper.cpp | Consent/legal UX; ~500 MB model; real build, not plumbing |

---

## 2. Mechanisms, one paragraph each

### Meetily — pure filesystem, zero auth
Open `~/Library/Application Support/com.meetily.ai/meeting_minutes.sqlite` read-only (`file:...?mode=ro`, **never** `immutable=1` — the DB is WAL and recent meetings live in the `-wal`). Windows `%APPDATA%\com.meetily.ai\`, Linux `~/.local/share/com.meetily.ai/`. Select explicit columns — the same directory holds plaintext third-party API keys in `settings`/`transcript_settings`, so `SELECT *` is banned. `meetings(id, title, created_at, folder_path)` gives the window (`created_at` is full ISO-8601 UTC); `transcript_chunks.transcript_text` gives the whole meeting in one row; `summary_processes.result` is JSON with a `markdown` key holding an already-local-LLM-generated summary — the best standup input in the whole memo. **The refutation corrected the original research here:** the folder-vs-DB join is *exact*, not fuzzy — `meetings.folder_path` is populated and matches on-disk folders 8/8; `metadata.json`'s null `meeting_id` is irrelevant because you invert the map you already have. Orphan folders are deleted recordings, not missing data — do not surface them. Precedent already in the repo: `crates/autostand-adapters/src/sources/opencode.rs` opens a third-party SQLite with `SQLITE_OPEN_READ_ONLY` and a time-ranged `query_map`; `rusqlite` is already a workspace dep.

### Zoom — local files (primary)
Zoom's post-May-2026 "Meeting transcript" feature writes a plain `.txt` into the same per-meeting folder tree local recordings use: `~/Documents/Zoom/<date> <meeting name>/` on macOS and Linux, `C:\Users\<user>\Documents\Zoom\` on Windows ([KB0085682](https://support.zoom.com/hc/en/article?id=zm_kb&sysparm_article=KB0085682)). Crucially — and this refutes the original "host-only" conjunct — the host-side setting *"Allow saving of transcripts to computer by"* goes up to **all meeting participants**, so a dev who merely attends their lead's standup can end up with the file on their own disk. Separately, `Settings > Recording > Local recording > Save closed caption as a VTT file` still produces `closed_caption.vtt` (timestamped, speaker-attributed) next to local recordings. **What genuinely died on 18 May 2026** is the old in-meeting *Save Captions* button and `meeting_saved_closed_caption.txt` ([KB0063899](https://support.zoom.com/hc/en/article?id=zm_kb&sysparm_article=KB0063899), [KB0085668](https://support.zoom.com/hc/en/article?id=zm_kb&sysparm_article=KB0085668)) — so the original claim "the local path is gone" is wrong, but any code keying on that specific `.txt` is dead. Glob by extension; the new filename is not documented.

### Zoom — PKCE OAuth (secondary)
Toggle **Use Public Client OAuth** in Marketplace app credentials; Zoom mints a *separate public client ID with no secret*, and the token exchange sends only `client_id` + `code_verifier` with **no Authorization header** ([public-pkce](https://developers.zoom.us/blog/public-pkce/), [OAuth docs](https://developers.zoom.us/docs/integrations/oauth/)). Always send `code_challenge_method=S256` — it defaults to `plain`. Loopback `http://127.0.0.1:<port>/callback` is allowed **only** on this flow (Zoom staff, devforum June 2026 — not in the reference docs). Then: `GET /v2/past_meetings/{meetingId}/instances` → past-instance UUID (double-URL-encode) → `GET /v2/meetings/{meetingUUID}/transcript` (scope `cloud_recording:read:meeting_transcript`) → fetch `download_url` with the same Bearer token; plus `GET /v2/meetings/{meetingId}/meeting_summary` (scope `meeting:read:summary`) for the AI Companion summary, which is a better and cheaper standup input than raw VTT. **The refutation corrected the research twice:** cloud recording is *not* required (this endpoint reads the transcript retention store, not `/recordings`), and Marketplace publication is *not* required if you use BYO Server-to-Server credentials instead. Refresh tokens rotate on every use and expire at 90 days; consent is forced on every auth-code exchange for public clients (not on refresh).

### Google Meet — `drive.file` + desktop Picker (the only version worth building)
Since the July 2026 rollout, Meet files notes/transcripts/recordings into a `Google Meet` folder in Drive, and attendees with file access get shortcuts in their own ([Workspace Updates](https://workspaceupdates.googleblog.com/2026/07/google-meet-now-organizes-your-meeting-notes-transcripts-and-recordings-in-your-Google-Drive.html)). Hit `https://accounts.google.com/o/oauth2/v2/auth` with `scope=https://www.googleapis.com/auth/drive.file`, `redirect_uri=http://127.0.0.1:PORT/oauth2callback`, `prompt=consent`, `trigger_onepick=true`, `allow_multiple=true`, `mimetypes=application/vnd.google-apps.document` ([desktop Picker](https://developers.google.com/workspace/drive/picker/guides/overview-desktop) — "The Google Picker imposes no additional restrictions" on redirect URI). Google opens the Picker in the default browser and redirects back with `picked_file_ids` + `code`. Then `GET https://www.googleapis.com/drive/v3/files/{fileId}/export?mimeType=text/plain`. Grants persist, so `files.list` afterwards returns exactly the app's granted set. `drive.file` is **non-sensitive** — per [support.google.com/cloud/answer/7454865](https://support.google.com/cloud/answer/7454865) an "unverified app" is one requesting sensitive or restricted scopes, so this path has no interstitial, no 100-user cap, no 7-day refresh expiry, no review. **This directly refutes the original research's headline finding** that sensitive-scope verification is the largest cost — it is avoidable entirely. The scope cannot be combined with any other, which is precisely what keeps you out of review.

### Google Meet — Meet REST API (rejected)
`conferenceRecords.list` → `transcripts.list` → `transcripts.entries.list` returns speaker-attributed JSON (`participant`, `text`, `startTime`) under `https://www.googleapis.com/auth/meetings.space.readonly`, no Drive scope, no CASA. The refutation is right that the original "organiser-only" blocker is stale — participant access went GA 2025-02-07 and `spaces.get` is documented as participant-accessible, so `spaces.get(meeting_code)` → `filter=space.name="spaces/{id}"` closes the chain. But `meetings.space.readonly` is **sensitive**, and sensitive means Search Console domain + homepage + same-domain privacy policy + demo video + per-scope justification + ~10 days review, re-triggered on scope change. It buys us nothing the Picker path doesn't already give us, plus a 30-day entry-retention cliff. Skip it.

### Microsoft Teams — Graph (rejected) and the fallbacks
`GET /me/onlineMeetings/{id}/transcripts` → `/transcripts/{id}/content` with `Accept: text/vtt` is GA and *does* support delegated access, including non-organizer invitees. It is still unreachable: `OnlineMeetingTranscript.Read.All` is `AdminConsentRequired: Yes` with no lower-privileged sibling and no personal-account support, and since ~29–31 Jul 2026 the tenant control `Set-CsTeamsMeetingConfiguration -EnableGraphTranscriptAccess $true` is **off by default everywhere**, returning `403 GraphAccessToTranscriptsDisabled` with, in Microsoft's words, no request-side workaround ([list-transcripts](https://learn.microsoft.com/en-us/graph/api/onlinemeeting-list-transcripts?view=graph-rest-1.0), [meeting-transcript-api-access](https://learn.microsoft.com/en-us/microsoftteams/meeting-transcript-api-access)). The OneDrive fallback is also dead as a *local file* read: transcripts are an alternate content stream on the `.mp4`, so the synced `~/Library/CloudStorage/OneDrive-<Tenant>/Recordings/*.mp4` contains no text, and `Files.Read.All`/`Sites.Read.All` were excluded from user self-consent by the July 2025 default-policy migration (MC1097272). The undocumented SharePoint route (`_api/v2.1/drives/{driveId}/items/{itemId}/media/transcripts`) works today with an SPO-audience token but is unversioned and a Microsoft engineer has publicly denied it exists — do not ship on it. **What ships for Teams is the manual export**: Recap → Transcript → Download → `.vtt`, into a watched folder.

---

## 3. What makes this hard

### (a) OAuth from a binary with no backend and no safe secret

This is the *smaller* of the two problems, and only Google turns it into a real one.

- **Zoom:** solved. Public Client OAuth issues a client ID with **no secret at all**. Shipping it in the `.dmg` is the sanctioned design. Loopback works — but only on the PKCE flow, and that is forum-confirmed rather than documented, so register a throwaway app and test a loopback redirect on day one before writing any code.
- **Microsoft:** solved mechanically. Entra public clients never have a secret, "Allow public client flows" plus `http://localhost` works, and Entra **ignores the port** when matching localhost redirects. PKCE proves the client wasn't intercepted; it cannot manufacture consent. The client model is not our blocker here.
- **Google:** desktop clients still require `client_secret` at the token endpoint — omit it and you get `client_secret is missing.` Google's own docs mark the secret "not applicable" only for Android/iOS/Chrome, and simultaneously state it "is obviously not treated as a secret" for installed apps. Meanwhile the [API ToS §4b](https://developers.google.com/terms) says flatly: *"Developer credentials may not be embedded in open source projects."* That contradiction has never been resolved ([google-auth-library-nodejs#959](https://github.com/googleapis/google-auth-library-nodejs/issues/959), closed as not planned). Thunderbird, rclone and Google's own gcloud/gsutil ship secrets in open source anyway. The realistic risk is not revocation — it is shared per-client-ID quota, where one abusive user degrades everyone. OOB is dead (all clients blocked 2023-01-31), so there is no copy-paste escape.

### (b) Verification, review and admin consent — the gates that actually decide this

| Gate | Cost | Who it blocks |
|---|---|---|
| Zoom Marketplace review (Public **or Unlisted**) | TDD, per-scope justification, OWASP security review, no SLA (days → ~4 weeks), re-review on scope change | Every non-us user, until we pass |
| Google sensitive-scope verification | Verified domain + homepage + same-domain privacy policy + demo video, ~10 days + round-trips | Everyone, if we touch `meetings.space.readonly` |
| Google restricted scopes + CASA | ~$540–$6,000, annual re-assessment | Everyone, if we touch `drive.readonly`/`drive.meet.readonly` — **never do this** |
| Google `drive.file` only | **Nothing** | Nobody |
| Microsoft admin consent | Not purchasable, not self-servable | Every non-admin |
| Microsoft `EnableGraphTranscriptAccess` | PowerShell, per tenant, off by default | ~100% of tenants today |

**Plainly: which providers can a non-admin individual actually reach?**

- **Meetily — yes, unconditionally.** No vendor exists to gate us.
- **Zoom — yes, if they pay.** Every setting the local path needs is user-level on a personal/Pro account. Only a *managed corporate tenant* can lock it, and a corporate admin can also require Marketplace pre-approval for the OAuth path.
- **Google Meet — yes via `drive.file`, no via the Meet API.** The entitlement wall (Business Standard+ / Workspace Individual / consumer AI Pro) excludes plain free Gmail, but that is a paywall, not an identity exclusion — Workspace Individual attaches to an existing `@gmail.com` with no domain and no admin.
- **Microsoft Teams — no.** Not "hard", *closed*. Two admin-only gates, both defaulting shut. Manual export and local capture are the only things that work.

The strategic point that outranks all of the above: **the OAuth work buys access to nothing for most users.** Every vendor path is gated on a paid tier *and* on somebody having remembered to turn transcription on for that specific meeting. Weeks of verification to serve a minority of a minority is a bad trade against a two-day local-file adapter.

---

## 4. Recommended order

**The honest answer is yes: start with the local-file paths and Meetily, because the OAuth ones are gated.** This is also the answer that matches what autostand already is — `crates/autostand-adapters/src/sources/` reads git, `~/.claude`, `~/.codex` read-only, and every phase-1 item below is the same shape.

1. **Meetily adapter — 1–2 days.** New `sources/meetily.rs` alongside the existing eight; no new dependency, no new permission model, no vendor. Emit the same normalised struct. Feed `summary_processes.result.markdown` preferentially, fall back to `transcript_chunks.transcript_text`, and prompt defensively — surface content as *"discussed in \<meeting title\>"*, never *"you committed to"*. Opt-in, off by default: this is recorded conversation involving third parties, a materially larger privacy blast radius than git data.
2. **Generic WebVTT/transcript folder watcher — 1–2 days.** One parser, three payoffs: Zoom `~/Documents/Zoom/**/*.txt` and `closed_caption.vtt`, Teams Recap `.vtt`/`.docx` exports, and any other vendor's manual export. This is the entire Teams story and half the Zoom story, for zero auth and zero vendor risk. Ship it before anything that needs a browser.
3. **Zoom PKCE OAuth — 3–5 days code, plus unbounded review.** Do it only after (1) and (2) are shipped, and start the Unlisted Marketplace submission *in parallel* with the build since calendar time dominates. Day-zero spike: register a throwaway app, confirm a loopback redirect is accepted on the public-PKCE flow. Be explicit in the UI that coverage is "meetings you hosted".
4. **Google `drive.file` + desktop Picker — 2–3 days.** No review, no domain, no privacy-policy hosting. Spike first (30 min): does `allow_folder_selection=true` cascade `drive.file` grants to descendants? If not, fall back to `allow_multiple=true` and a "Sync meetings" button that re-opens the Picker — friction, not a blocker.
5. **Local capture + on-device ASR — the strategic bet, deliberately last.** ScreenCaptureKit/CoreAudio process taps on macOS 14.4+, WASAPI loopback on Windows, PipeWire monitor on Linux, into whisper-rs. Prior art in our exact stack: Meetily and anarlog (ex-Hyprnote). It is the only path that covers Zoom, Teams, Meet, Discord and in-person with one codebase, needs no vendor, no consent screen and no entitlement — and it turns "what was discussed" into a first-class autostand feature instead of a tenant-dependent one. It is also a genuine product/legal decision (two-party-consent recording law, participant notice UX), not a plumbing task. Don't start it casually; do plan for it.
6. **Never build:** Google Meet REST API with `meetings.space.readonly`; any Google `drive.readonly`/`drive.meet.readonly` scope; Microsoft Graph `callTranscript` as a headline feature; Zoom RTMS; any webhook-based design (`recording.transcript_completed`, Graph change notifications) — all require a public HTTPS endpoint we will never have. Poll on app launch.

**Design constraint that applies to all of it:** the empty case is the *default*, not the exception. On the reference machine Meetily shows 8 meetings across 3.5 months with 6-week gaps. The standup must be complete and unembarrassing with zero meeting data; meeting content is additive garnish. And every failure mode needs a distinct, honest message — "your plan doesn't include transcripts", "nobody transcribed this meeting", "your admin blocked this" all look identical to a user and each needs different wording.

---

## 5. What I could not confirm

Listed rather than smoothed over. Items 1–4 are cheap spikes that should happen before any commitment.

1. **Zoom loopback on public PKCE.** Sanctioned only by a Zoom staff forum post (June 2026), not by the reference docs, which elsewhere say localhost is rejected and custom schemes are Meeting-SDK-only. If it's still blocked, the whole Zoom OAuth path needs `autostand://` instead. *30-minute spike, day zero.*
2. **The new Zoom save-transcript filename.** Undocumented. Glob by extension, never hardcode. Also unverified: whether an *attendee*'s saved transcript lands in the same `~/Documents/Zoom/<date> <topic>/` folder as a host's.
3. **Google Picker folder-grant cascade.** `allow_folder_selection=true` exists; whether picking a folder grants `drive.file` on its descendants is undocumented and community reports say no. *30-minute spike.*
4. **Google Meet `filter=space.meeting_code=...` for non-organisers** — whether it returns rows directly or requires resolving via `spaces.get` first. Moot if we take the Picker path, which we should.
5. **Meetily on Windows and Linux.** The macOS path is empirically verified; `%APPDATA%\com.meetily.ai\` and `~/.local/share/com.meetily.ai/` are derived from the Tauri v2 identifier, not observed.
6. **Meetily's forward compatibility.** Private schema, no contract, demonstrated churn (Python→Rust rewrite, `.db`→`.sqlite`, 10 migrations), `main` untouched since 2026-06-05 with 349 open issues. Tolerate unknown JSON keys, gate on the `version` field, degrade to "no meeting data" on any parse failure.
7. **Whether Microsoft's `EnableGraphTranscriptAccess` truly blocks delegated calls** as well as app-only. The docs say "all transcript requests" and field reports of broken integrations support it, but I have no first-hand test against a tenant with the flag off. It does not change the recommendation — admin consent alone already closes the path.
8. **The SharePoint `_api/v2.1/.../media/transcripts` route.** Working prior art exists (jameswh3's PnP script, mingnz/teams-cli), and `AllSites.Read` is *probably* still user-consentable — but the endpoint is undocumented, a Microsoft engineer has denied it exists, and the July-2025 consent-policy migration may gate the scope anyway. Not shippable.
9. **Whether Google's CASA "third-party server" trigger exempts a purely local app.** Plausibly yes; the wording is about *capability*, not implementation, and a reviewer decides. Irrelevant if we stay on `drive.file`, which is the point.
10. **Real-world exposure to Google's ToS §4b** on embedded credentials. Tolerated for a decade in gcloud, Thunderbird and rclone; enforceable at Google's discretion with no appeal. The practical risk is shared-quota degradation, not revocation. The Picker path sidesteps it entirely only if we still ship a client ID — which we do, so the residual risk is nonzero.