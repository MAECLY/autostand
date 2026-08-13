//! Anti-recursion: detect autostand's own render prompt echoed back by a data source.
//!
//! autostand invokes CLI providers (`claude`, `codex`, `grok`, `gemini`) to render a
//! standup. Those CLIs write the invocation into their own session logs — and autostand
//! then reads those same logs as data sources. Without a guard, the render prompt comes
//! back on the next run disguised as "work the user did", and the model is shown its own
//! output format as if it were activity. Observed in the wild: a session whose prompts
//! were `## Source hierarchy`, `- Past tense, concrete, English.`, `## GIT FACTS`,
//! `## OUTPUT`, a bare ```` ``` ````, and the preset skeleton `**Yesterday**` /
//! `- <what you did>`.
//!
//! Two layers, because a CLI may log the system prompt and the user prompt as separate
//! messages:
//!
//! 1. [`is_render_prompt_echo`] — message level. Drops a whole message that is one of
//!    our prompts. This is the primary guard.
//! 2. [`is_scaffolding_line`] — line level. Catches fragments that survive re-chunking,
//!    and is deliberately conservative so it can never eat a real note.

/// Opening line of every render request built by `render::build_prompt`.
///
/// The prompt builder and this filter share the constant so they cannot drift: change
/// the heading and the guard follows automatically.
pub const RENDER_REQUEST_SENTINEL: &str = "# Standup render request";

/// Lowercase fragments unique to autostand's own system prompt.
const SYSTEM_PROMPT_MARKERS: &[&str] = &[
    "you are a daily standup compiler",
    "## source hierarchy",
    "git facts — committed work",
    "never attribute to ai",
    "never say \"no work done\"",
];

/// Lowercase prefixes of headings and context labels emitted by `build_prompt`.
///
/// Every entry is a line autostand writes verbatim; none is plausible prose a developer
/// would type into a coding agent.
const SCAFFOLDING_PREFIXES: &[&str] = &[
    "# standup render request",
    "## git facts",
    "## github",
    "## pr reviews",
    "## edited files",
    "## previous render",
    "## output",
    "## source hierarchy",
    "## rules",
    "format your output using this structure",
    "every bold section header shown in the selected structure",
];

/// Lowercase exact lines from the output-preset skeletons.
const SKELETON_LINES: &[&str] = &[
    "**yesterday**",
    "**today**",
    "**blockers**",
    "**pr review**",
    "**confidence**",
    "**risks**",
    "- none",
];

/// How many distinct markers make a message our own prompt rather than a coincidence.
const ECHO_MARKER_THRESHOLD: usize = 2;

/// True when `raw` is (or embeds) an autostand render prompt.
///
/// A message carrying the sentinel is ours outright. Otherwise it takes
/// [`ECHO_MARKER_THRESHOLD`] distinct scaffolding markers, so a developer who happens to
/// type `## Rules` into a coding agent keeps their note.
#[must_use]
pub fn is_render_prompt_echo(raw: &str) -> bool {
    if raw.contains(RENDER_REQUEST_SENTINEL) {
        return true;
    }
    let lower = raw.to_lowercase();
    let mut hits = 0usize;
    for marker in SYSTEM_PROMPT_MARKERS {
        if lower.contains(marker) {
            hits += 1;
        }
    }
    for prefix in SCAFFOLDING_PREFIXES {
        if lower.lines().any(|line| line.trim_start().starts_with(prefix)) {
            hits += 1;
        }
    }
    hits >= ECHO_MARKER_THRESHOLD
}

/// True when a single line is autostand's own prompt scaffolding.
///
/// Conservative by construction: only lines autostand itself writes. Generic labels such
/// as `Title:` are excluded, because a developer may legitimately type them — those are
/// caught at message level by [`is_render_prompt_echo`] instead.
#[must_use]
pub fn is_scaffolding_line(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    // A bare fence: the preset templates are wrapped in one, so it leaks on its own.
    if trimmed == "```" {
        return true;
    }
    // AUTO/MANUAL block markers from a standup file that reached a session log.
    if trimmed.starts_with("<!-- AUTO:") || trimmed.starts_with("<!-- MANUAL:") {
        return true;
    }
    // Preset placeholder bullets: `- <what you did>`, `- <blocker or None>`.
    if let Some(rest) = trimmed.strip_prefix("- ") {
        let rest = rest.trim();
        if rest.starts_with('<') && rest.ends_with('>') {
            return true;
        }
    }
    // The subtitle autostand generates: `_Work completed August 01–02, 2026._`
    if trimmed.starts_with("_Work completed ") && trimmed.ends_with('_') {
        return true;
    }
    let lower = trimmed.to_lowercase();
    if SCAFFOLDING_PREFIXES
        .iter()
        .any(|prefix| lower.starts_with(prefix))
    {
        return true;
    }
    SKELETON_LINES.iter().any(|skeleton| lower == *skeleton)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verbatim from the 2026-08-13 audit sidecar's `grok_sessions`, which is how the
    /// bug was found. Every one of these was fed to the model as "work you did".
    const OBSERVED_ECHO: &str = "## Source hierarchy (most authoritative first)\n\
1. GIT FACTS — committed work. Authoritative for what was committed and when.\n\
## Rules\n\
- Past tense, concrete, English.\n\
- NEVER attribute to AI. Write as if the human did the work.\n\
Filing date: 2026-08-03\n\
Work window: 2026-08-01 .. 2026-08-02\n\
Subtitle: _Work completed August 01–02, 2026._\n\
## GIT FACTS\n\
## NOTES\n\
## OUTPUT\n\
Format your output using this structure:\n\
```\n\
**Yesterday**\n\
- <what you did>\n\
**Today**";

    #[test]
    fn detects_the_observed_grok_session_echo() {
        assert!(is_render_prompt_echo(OBSERVED_ECHO));
    }

    #[test]
    fn detects_a_prompt_by_its_sentinel_alone() {
        assert!(is_render_prompt_echo(
            "# Standup render request\n\nFiling date: 2026-08-13\n"
        ));
    }

    #[test]
    fn keeps_a_message_with_a_single_incidental_marker() {
        // A developer writing docs may well type one of these; one hit is not an echo.
        assert!(!is_render_prompt_echo(
            "Updated the ## Rules section of the contributing guide"
        ));
    }

    #[test]
    fn keeps_ordinary_developer_prompts() {
        assert!(!is_render_prompt_echo("fixed the login redirect bug"));
        assert!(!is_render_prompt_echo(
            "Review this change for security vulnerabilities."
        ));
        assert!(!is_render_prompt_echo(""));
    }

    #[test]
    fn scaffolding_lines_cover_every_observed_leak() {
        for line in [
            "## Source hierarchy (most authoritative first)",
            "## GIT FACTS",
            "## OUTPUT",
            "## Rules",
            "```",
            "**Yesterday**",
            "**Today**",
            "- <what you did>",
            "_Work completed August 01–02, 2026._",
            "<!-- AUTO:MacStudio-de-Miguel:START -->",
            "<!-- MANUAL:START -->",
            "Format your output using this structure:",
        ] {
            assert!(is_scaffolding_line(line), "should reject: {line}");
        }
    }

    #[test]
    fn scaffolding_lines_keep_real_notes() {
        for line in [
            "fixed the login redirect bug",
            "- reviewed PR #42 for the auth module",
            "Title: the new onboarding flow",
            "- Implementing August 13th fix for POST /api/v1/external/gorgias/tickets/claim",
            "## Context for the migration",
            "",
        ] {
            assert!(!is_scaffolding_line(line), "should keep: {line}");
        }
    }

    #[test]
    fn a_bare_none_bullet_is_skeleton_but_a_sentence_is_not() {
        assert!(is_scaffolding_line("- None"));
        assert!(!is_scaffolding_line("- None of the tests were flaky"));
    }
}
