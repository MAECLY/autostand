import type { StandupPreset, Verbosity } from "@/lib/types";

const PERSONA_REPO = "autostand-core";
const PERSONA_TICKET = "FIF-136";

function prefix(conventional: boolean, type: string, text: string): string {
  return conventional ? `${type}: ${text}` : text;
}

function bullets(
  conventional: boolean,
  verbosity: Verbosity,
  items: Array<[string, string]>,
): string {
  const selected = verbosity === "terse" ? items.slice(0, 1) : items;
  return selected
    .map(
      ([type, text]) =>
        `- ${prefix(conventional, type, text)}`,
    )
    .join("\n");
}

export function presetExample(
  preset: StandupPreset,
  verbosity: Verbosity,
  conventional: boolean,
): string {
  const v = verbosity;
  const c = conventional;
  switch (preset) {
    case "classic-scrum":
      return `**Yesterday**
${bullets(c, v, [
  ["feat", `landed the redact pass in ${PERSONA_REPO}`],
  ["fix", "rebased the union-merge driver onto main"],
])}

**Today**
${bullets(c, v, [
  ["feat", "wire the format toggles into output_section"],
  ["refactor", "split build_prompt into testable helpers"],
])}

**Blockers**
- None`;
    case "four-question":
      return `**Yesterday**
${bullets(c, v, [
  ["feat", `landed the redact pass in ${PERSONA_REPO}`],
])}

**Today**
${bullets(c, v, [
  ["feat", "wire the format toggles into output_section"],
])}

**Blockers**
- None

**Help needed**
- Need a review on the cron validator PR #42`;
    case "mad-sad-glad":
      return `**Mad**
- Flaky test on the scheduler lock under self-heal

**Sad**
- Missed the Tuesday self-heal window by 2 minutes

**Glad**
${bullets(c, v, [
  ["feat", "atomic write-then-rename landed cleanly"],
])}`;
    case "start-stop-continue":
      return `**Start**
- Pairing on the audit classifier

**Stop**
- Hand-editing sidecar JSON after a render

**Continue**
${bullets(c, v, [
  ["feat", "daily deterministic fallback as the safety net"],
])}`;
    case "keep-drop-create":
      return `**Keep**
- Host-slug stability invariant (persist, no DHCP)

**Drop**
- Probing dates beyond the 14-day window

**Create**
- A calendar view for History`;
    case "five-question":
      return `**Yesterday**
${bullets(c, v, [
  ["feat", `landed the redact pass in ${PERSONA_REPO}`],
])}

**Today**
${bullets(c, v, [
  ["feat", "wire the format toggles into output_section"],
])}

**Blockers**
- None

**Sprint Goal confidence**
- 4/5

**Team health**
- steady`;
    case "spotify-4q":
      return `**Did**
${bullets(c, v, [
  ["feat", "shipped the redact pass end-to-end"],
])}

**Doing**
${bullets(c, v, [
  ["feat", "wiring the TerminalPanel into __root"],
])}

**Blocking**
- Waiting on the UI calendar primitive from the design repo

**Need**
- Design review for the bottom-panel chrome`;
    case "async-status":
      return `**Done yesterday**
${bullets(c, v, [
  ["feat", "merged the union-merge driver"],
  ["fix", "closed the race in the scheduler lock"],
])}

**Doing today**
${bullets(c, v, [
  ["feat", "FormatTab realistic previews"],
])}

**Blockers**
- @design: calendar primitive still pending

**FYI**
- Out Friday`;
    case "walking-timebox":
      return `**Yesterday**
- redact pass merged

**Today**
- format previews

**Blockers**
- None`;
    case "walk-the-board":
      return `**In-flight cards**
- ${PERSONA_TICKET}: in review / next: merge / blocker: none
- FIF-141: doing / next: wire IPC / blocker: none

**Aging / WIP violations**
- FIF-104: aging 6 days

**Swarm needed?**
- ${PERSONA_TICKET}: who pairs with @alex on the merge`;
    case "ytbr":
      return `**Yesterday**
${bullets(c, v, [
  ["feat", `landed the redact pass in ${PERSONA_REPO}`],
])}

**Today**
${bullets(c, v, [
  ["feat", "wire the format toggles into output_section"],
])}

**Blockers**
- None

**Risks**
- Scheduler lock contention under self-heal on slow disks`;
    case "decisions-commitments":
      return `**Fulfilled**
${bullets(c, v, [
  ["feat", "atomic write-then-rename landed"],
])}

**Committing today**
- Calendar view by end of week

**Decisions**
- CLI-first stays the default provider mode

**Blockers**
- None`;
    case "okr-tied":
      return `**Key Result**
- KR1: zero phantom tickets in audit

**Yesterday's delta**
- +3 covered tickets

**Today**
${bullets(c, v, [
  ["feat", "wire the classifier over IPC"],
])}

**Blockers**
- None

**Confidence**
- 4/5`;
  }
}