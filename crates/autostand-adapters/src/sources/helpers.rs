//! Shared helpers for data source adapters.
//!
//! Pure utility functions used across multiple sources: repo scanning,
//! ticket-key extraction, subprocess execution with timeout, JSONL discovery,
//! and window-membership checks.

use std::borrow::Cow;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use autostand_runlog::proc::{run_process, ProcError, ProcSpec, StreamPolicy};
use chrono::NaiveDate;
use regex::Regex;

use super::DateWindow;

/// Step every data-source subprocess and summary line is filed under.
///
/// Matches `RunKind::GatherPreview::step()` in the app crate so the Terminal
/// colours a source's `git`/`gh` calls the same as the gather step that owns them.
pub const GATHER_STEP: &str = "gather";

static TICKET_RE: OnceLock<Regex> = OnceLock::new();

fn ticket_re() -> &'static Regex {
    TICKET_RE.get_or_init(|| {
        Regex::new(r"\b([A-Z]+-\d+)\b").unwrap_or_else(|_| Regex::new(r"[A-Z]+-\d+").unwrap())
    })
}

/// Scan `github_dir` at maxdepth 1, returning every child dir that contains a `.git` dir.
pub fn scan_repos(github_dir: &Path) -> Vec<PathBuf> {
    let mut repos = Vec::new();
    let Ok(entries) = std::fs::read_dir(github_dir) else {
        return repos;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() && p.join(".git").is_dir() {
            repos.push(p);
        }
    }
    repos.sort();
    repos
}

/// Extract deduplicated ticket keys (`[A-Z]+-\\d+`) from `text`, preserving first-seen order.
pub fn extract_ticket_keys(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    for cap in ticket_re().captures_iter(text) {
        if let Some(m) = cap.get(1) {
            let key = m.as_str().to_string();
            if !out.contains(&key) {
                out.push(key);
            }
        }
    }
    out
}

/// Run a subprocess with a timeout, returning trimmed stdout on success.
///
/// [`StreamPolicy::Silent`]: every caller of this function runs it **inside a
/// per-repository or per-pull-request loop**, so one summary line per spawn
/// would push the rest of the run out of the Terminal's ring buffer. The source
/// itself emits one line for the whole sweep — see [`log_sweep`].
///
/// Errors are returned as `String` for caller-side resilience decisions. The
/// child's stderr is deliberately **not** part of that string: these messages
/// travel into `Gathered::failures`, which `pipeline_runner` renders straight
/// into the Terminal panel, and `docs/specs/audit.md` keeps stderr out of it.
pub async fn run_cmd(cmd: &str, args: &[&str], timeout_secs: u64) -> Result<String, String> {
    run_cmd_with(cmd, args, timeout_secs, StreamPolicy::Silent, None).await
}

/// [`run_cmd`] for a call the user should see: one start line and one end line.
///
/// `label` is the display name; it must name the *command*, never a path, a
/// branch or an organisation (`"gh search prs"`, not `"gh search prs --owner=…"`).
pub async fn run_cmd_visible(
    cmd: &str,
    args: &[&str],
    timeout_secs: u64,
    label: &'static str,
) -> Result<String, String> {
    run_cmd_with(cmd, args, timeout_secs, StreamPolicy::Summary, Some(label)).await
}

/// One line summarising a whole [`StreamPolicy::Silent`] sweep.
///
/// The counterpart of the silent policy above: the individual spawns stay out of
/// the panel, but the sweep itself must not.
pub fn log_sweep(message: impl Into<String>, detail: impl Into<String>) {
    autostand_runlog::log(
        autostand_runlog::LogLine::done(GATHER_STEP, message).with_detail(detail),
    );
}

/// Shared body of [`run_cmd`] and [`run_cmd_visible`].
async fn run_cmd_with(
    cmd: &str,
    args: &[&str],
    timeout_secs: u64,
    stream: StreamPolicy,
    label: Option<&'static str>,
) -> Result<String, String> {
    let mut spec = ProcSpec::new(cmd, Cow::Borrowed(GATHER_STEP))
        .args(args)
        .timeout_secs(timeout_secs)
        .stream(stream);
    if let Some(label) = label {
        spec = spec.label(label);
    }
    let display = spec.display_name();
    let out = run_process(spec).await.map_err(|err| match err {
        ProcError::Timeout { secs, .. } => format!("timeout after {secs}s: {display}"),
        ProcError::Spawn { kind, .. } => format!("spawn {display}: {kind}"),
        ProcError::Io { kind, .. } => format!("io error running {display}: {kind}"),
    })?;
    if !out.success {
        return Err(match out.code {
            Some(code) => format!("{display} exited {code}"),
            None => format!("{display} was terminated"),
        });
    }
    Ok(out.stdout_trimmed().to_string())
}

/// Locate `name` on `PATH` without spawning anything.
///
/// WHY a filesystem probe instead of `<program> --version`: this answers
/// [`super::DataSource::is_available`], which is **synchronous** and is called
/// from inside the async gather. Spawning there blocked a Tokio worker, and
/// routing it through the async spawner would mean an async trait method for
/// all eight sources. A `PATH` walk is both cheaper and truthful enough: a
/// binary that exists but refuses to run fails in `gather`, where the failure is
/// recorded and reported.
pub fn find_on_path(name: &str) -> Option<PathBuf> {
    // WHY not `PATH` alone: the same reason the provider CLIs went missing in
    // 1.0.0 — a GUI-launched app does not have the user's PATH. `gh` lives in
    // exactly the directories launchd omits.
    autostand_runlog::which::find_program(name)
}

/// Recursively collect every `*.jsonl` file under `dir` (sorted).
pub fn read_jsonl_paths(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    walk_jsonl(dir, &mut out);
    out.sort();
    out
}

fn walk_jsonl(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            walk_jsonl(&p, out);
        } else if p.extension().and_then(|e| e.to_str()) == Some("jsonl") {
            out.push(p);
        }
    }
}

/// True if `date` falls within `[window.start, window.end]`.
pub fn window_contains(window: &DateWindow, date: NaiveDate) -> bool {
    date >= window.start && date <= window.end
}

/// Truncate `s` to at most `max` chars, appending `…` if truncated.
pub fn truncate_str(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Collapse whitespace and lowercase for dedup purposes.
pub fn normalize_text(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

/// Strip a leading/trailing code fence and detect code-paste lines.
pub fn looks_like_code_paste(s: &str) -> bool {
    let lines: Vec<&str> = s.lines().collect();
    let fence_count = lines.iter().filter(|l| l.starts_with("```")).count();
    if fence_count >= 2 {
        return true;
    }
    let codey = lines
        .iter()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("fn ")
                || t.starts_with("def ")
                || t.starts_with("class ")
                || t.starts_with("pub ")
                || t.starts_with("let ")
                || t.starts_with("const ")
                || t.starts_with("import ")
                || t.starts_with("use ")
        })
        .count();
    codey >= 2
}

/// Decide whether a user-prompt snippet is keepable (passes filters).
pub fn is_keepable_prompt(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    if trimmed.starts_with('/') {
        return false;
    }
    if trimmed.starts_with('<') && trimmed.ends_with('>') {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("you are") {
        return false;
    }
    if lower.starts_with("established facts") {
        return false;
    }
    if looks_like_code_paste(trimmed) {
        return false;
    }
    if looks_like_agent_dump_line(trimmed) {
        return false;
    }
    if autostand_core::meta::is_meta_work(trimmed, None) {
        return false;
    }
    // Second anti-recursion layer: a fragment of our own prompt that survived
    // re-chunking by the CLI's logger, so the message-level guard never saw it whole.
    if autostand_core::prompt_echo::is_scaffolding_line(trimmed) {
        return false;
    }
    true
}

/// Lines that come from a Claude Code / Codex review dump, not from a human standup note.
pub fn looks_like_agent_dump_line(s: &str) -> bool {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        return false;
    }
    let lower = trimmed.to_lowercase();
    if lower.starts_with("changed files")
        || lower.starts_with("unified diff")
        || lower.starts_with("=== diff")
        || lower.starts_with("@@ ")
        || lower == "prompts:"
        || lower == "plans:"
        || lower == "files:"
        || lower == "## context"
        || trimmed.starts_with("## CONTEXT")
        || trimmed.starts_with("===")
    {
        return true;
    }
    // Review-command file lists: `- src/foo.ts` or `- - package.json`.
    let bullet = trimmed
        .trim_start_matches('-')
        .trim_start_matches('-')
        .trim();
    looks_like_source_path(bullet)
}

/// A path-looking token (`src/foo.ts`, `package.json`, `=== DIFF: x ===`).
fn looks_like_source_path(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(' ') && !s.contains('/') {
        return false;
    }
    let last = s.rsplit(['/', '\\']).next().unwrap_or(s);
    last.contains('.')
        && last
            .rsplit('.')
            .next()
            .is_some_and(|ext| ext.len() <= 5 && ext.chars().all(|c| c.is_ascii_alphanumeric()))
}

/// Index of the first line that starts a review-diff dump, if any.
pub fn agent_dump_cut_line(raw: &str) -> Option<usize> {
    raw.lines().position(|line| {
        let lower = line.trim().to_lowercase();
        lower.starts_with("changed files")
            || lower.starts_with("unified diff")
            || lower.starts_with("=== diff")
            || lower.starts_with("@@ ")
            || lower.starts_with("## context")
    })
}

/// Strip Claude Code preamble blocks that are not real user prompts: XML-like
/// wrappers (`<system-reminder>…</system-reminder>`, `<context>`, `<env>`,
/// `<dcp-system-reminder>`, etc.) and `ESTABLISHED FACTS:` preambles. Returns the
/// surviving lines joined with `\n`.
pub fn strip_preamble(s: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    let mut skip_until: Option<String> = None;
    for line in s.lines() {
        let trimmed = line.trim();
        if let Some(ref close) = skip_until {
            if trimmed.starts_with(close.as_str()) {
                skip_until = None;
            }
            continue;
        }
        if let Some(tag) = open_xml_block(trimmed) {
            skip_until = Some(tag);
            continue;
        }
        if trimmed.eq_ignore_ascii_case("ESTABLISHED FACTS (do not re-derive, build on these):")
            || trimmed.eq_ignore_ascii_case("ESTABLISHED FACTS:")
            || trimmed.eq_ignore_ascii_case("ESTABLISHED FACTS")
        {
            continue;
        }
        if trimmed.starts_with('-') {
            let bullet = trimmed.trim_start_matches('-').trim();
            if bullet.starts_with("Repo A")
                || bullet.starts_with("Repo B")
                || bullet.contains("ESTABLISHED FACTS")
            {
                continue;
            }
        }
        out.push(line);
    }
    out.join("\n")
}

/// Detect a `<tag>` opener and return its matching `</tag>` closer prefix.
fn open_xml_block(trimmed: &str) -> Option<String> {
    if !(trimmed.starts_with('<') && trimmed.ends_with('>')) {
        return None;
    }
    if !trimmed.starts_with("</") && !trimmed.starts_with("</") {
        let inner = trimmed
            .trim_start_matches('<')
            .trim_end_matches('>')
            .split([' ', '/'])
            .next()?
            .to_string();
        if inner.is_empty()
            || inner
                .chars()
                .any(|c| !c.is_alphanumeric() && c != '-' && c != '_')
        {
            return None;
        }
        return Some(format!("</{inner}>"));
    }
    None
}

/// Collector for deduped, filtered user-prompt snippets. Dedup is line-level so
/// a recurring multi-line preamble embedded in different surrounding text is
/// collapsed to one occurrence. Caps at 40 unique lines (≤ 200 chars each).
#[derive(Debug, Default)]
pub struct PromptCollector {
    seen_lines: std::collections::HashSet<String>,
    snippets: Vec<String>,
}

const MAX_SNIPPETS: usize = 40;

impl PromptCollector {
    /// Add a raw user-typed text. It is split into lines, each line is
    /// filtered, deduped against all prior lines, and truncated.
    pub fn add(&mut self, raw: &str) {
        // Anti-recursion: autostand drives `claude`/`codex`/`grok`/`gemini` to render a
        // standup, and those CLIs log the invocation into the very session files this
        // collector reads. Drop the whole message rather than filtering it line by line —
        // a half-kept render prompt still teaches the model to emit our own scaffolding.
        if autostand_core::prompt_echo::is_render_prompt_echo(raw) {
            return;
        }
        let stripped = strip_preamble(raw);
        // A Claude Code "review this change" dump is one user message: a short
        // intent, then `Changed files` + a unified diff. Keep only the intent.
        let usable = match agent_dump_cut_line(&stripped) {
            Some(0) => return,
            Some(cut) => stripped.lines().take(cut).collect::<Vec<_>>().join("\n"),
            None => stripped,
        };
        for line in usable.lines() {
            if self.snippets.len() >= MAX_SNIPPETS {
                return;
            }
            if !is_keepable_prompt(line) {
                continue;
            }
            let norm = normalize_text(line);
            if self.seen_lines.contains(&norm) {
                continue;
            }
            self.seen_lines.insert(norm);
            self.snippets.push(truncate_str(line.trim(), 200));
        }
    }

    /// Returns true if the collector is full.
    pub fn is_full(&self) -> bool {
        self.snippets.len() >= MAX_SNIPPETS
    }

    /// Render the collected snippets as a markdown CONTEXT block.
    pub fn render_context(&self, plans: &[String]) -> String {
        let mut s = String::from("## CONTEXT\n");
        if !plans.is_empty() {
            s.push_str("plans:\n");
            for p in plans {
                let _ = writeln!(s, "- {p}");
            }
        }
        s.push_str("prompts:\n");
        if self.snippets.is_empty() {
            s.push_str("_(none)_\n");
        } else {
            for snip in &self.snippets {
                let _ = writeln!(s, "- {snip}");
            }
        }
        s
    }

    /// Borrowed snippets list.
    pub fn snippets(&self) -> &[String] {
        &self.snippets
    }
}

/// Attribute a full file path to a repo by prefix match against `github_dir` children.
/// Returns `(repo_name, basename)` if a repo matches; else `None`.
pub fn attribute_file(path_str: &str, github_dir: &std::path::Path) -> Option<(String, String)> {
    if path_str.is_empty() {
        return None;
    }
    let p = std::path::Path::new(path_str);
    let basename = p
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path_str)
        .to_string();
    let repos = scan_repos(github_dir);
    for repo in repos {
        if let Some(repo_str) = repo.to_str() {
            if path_str.starts_with(repo_str) {
                let name = repo
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("?")
                    .to_string();
                return Some((name, basename));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unnecessary_wraps)]
    use super::*;
    use autostand_runlog::{scoped, RecordingSink, SinkRef};
    use chrono::NaiveDate;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::Arc;
    use tempfile::TempDir;

    fn window(start: (i32, u32, u32), end: (i32, u32, u32)) -> DateWindow {
        DateWindow {
            start: NaiveDate::from_ymd_opt(start.0, start.1, start.2).unwrap(),
            end: NaiveDate::from_ymd_opt(end.0, end.1, end.2).unwrap(),
            dates: Vec::new(),
        }
    }

    #[test]
    fn extract_ticket_keys_finds_multiple_keys() {
        let keys = extract_ticket_keys("worked on FIF-133 and PROJ-42 then FIF-133 again");
        assert_eq!(keys, vec!["FIF-133", "PROJ-42"]);
    }

    #[test]
    fn extract_ticket_keys_preserves_first_seen_order() {
        let keys = extract_ticket_keys("PROJ-42 before FIF-133 before ABC-1");
        assert_eq!(keys, vec!["PROJ-42", "FIF-133", "ABC-1"]);
    }

    #[test]
    fn extract_ticket_keys_ignores_lowercase() {
        let keys = extract_ticket_keys("fixed fif-133 bug and proj-42");
        assert!(keys.is_empty());
    }

    #[test]
    fn extract_ticket_keys_matches_uppercase_word_plus_digit() {
        let keys = extract_ticket_keys("FORTY-2 is matched by the regex");
        assert_eq!(keys, vec!["FORTY-2"]);
    }

    #[test]
    fn extract_ticket_keys_handles_empty_input() {
        assert!(extract_ticket_keys("").is_empty());
        assert!(extract_ticket_keys("no tickets here at all").is_empty());
    }

    #[test]
    fn truncate_str_short_unchanged() {
        assert_eq!(truncate_str("abc", 10), "abc");
        assert_eq!(truncate_str("exactly", 7), "exactly");
    }

    #[test]
    fn truncate_str_truncates_with_ellipsis() {
        let s = "abcdefghij";
        assert_eq!(truncate_str(s, 5), "abcd…");
        assert_eq!(truncate_str(s, 5).chars().count(), 5);
    }

    #[test]
    fn truncate_str_handles_multibyte() {
        let s = "héllo 世界 world";
        let out = truncate_str(s, 4);
        assert_eq!(out.chars().count(), 4);
        assert!(out.ends_with('…'));
    }

    #[test]
    fn normalize_text_collapses_whitespace_and_lowercases() {
        assert_eq!(normalize_text("  Hello   WORLD  "), "hello world");
        assert_eq!(normalize_text("\tFoo\nBar\t"), "foo bar");
    }

    #[test]
    fn normalize_text_empty_input() {
        assert_eq!(normalize_text(""), "");
        assert_eq!(normalize_text("   "), "");
    }

    #[test]
    fn looks_like_code_paste_detects_code_fences() {
        let s = "```\nfn main() {}\n```";
        assert!(looks_like_code_paste(s));
    }

    #[test]
    fn looks_like_code_paste_detects_codey_lines() {
        let s = "def foo():\n    pass\nclass Bar:\n    pass";
        assert!(looks_like_code_paste(s));
    }

    #[test]
    fn looks_like_code_paste_detects_rust_lines() {
        let s = "pub fn thing() -> u32 { 1 }\nuse std::io::Read;\nlet x = 3;";
        assert!(looks_like_code_paste(s));
    }

    #[test]
    fn looks_like_code_paste_rejects_prose() {
        assert!(!looks_like_code_paste("worked on the auth module today"));
        assert!(!looks_like_code_paste(
            "fixed a bug\nreviewed a PR\nmerged the branch"
        ));
    }

    #[test]
    fn looks_like_code_paste_single_codey_line_is_not_enough() {
        let s = "def only one line";
        assert!(!looks_like_code_paste(s));
    }

    #[test]
    fn is_keepable_prompt_rejects_slash_commands() {
        assert!(!is_keepable_prompt("/daily-standup"));
        assert!(!is_keepable_prompt("   /clear  "));
    }

    #[test]
    fn is_keepable_prompt_rejects_xml_wrappers() {
        assert!(!is_keepable_prompt("<system>You are an assistant</system>"));
        assert!(!is_keepable_prompt("<context>info</context>"));
    }

    #[test]
    fn is_keepable_prompt_rejects_you_are_prefix() {
        assert!(!is_keepable_prompt("You are a helpful assistant"));
        assert!(!is_keepable_prompt("you are the world expert in rust"));
    }

    #[test]
    fn is_keepable_prompt_rejects_code_paste() {
        let s = "```\nfn main() {}\n```";
        assert!(!is_keepable_prompt(s));
        let py = "def foo():\n    pass\nclass Bar:\n    pass";
        assert!(!is_keepable_prompt(py));
    }

    #[test]
    fn is_keepable_prompt_rejects_empty_and_whitespace() {
        assert!(!is_keepable_prompt(""));
        assert!(!is_keepable_prompt("   \n\t  "));
    }

    #[test]
    fn is_keepable_prompt_rejects_meta_work() {
        assert!(!is_keepable_prompt("added scrub logic to the pipeline"));
        assert!(!is_keepable_prompt("fixed compile.sh bug"));
    }

    #[test]
    fn is_keepable_prompt_keeps_normal_prose() {
        assert!(is_keepable_prompt("fixed the login redirect bug"));
        assert!(is_keepable_prompt("reviewed PR #42 for the auth module"));
    }

    #[test]
    fn is_keepable_prompt_rejects_review_diff_dumps() {
        assert!(!is_keepable_prompt(
            "Changed files (you may Read these and any other file in the repo):"
        ));
        assert!(!is_keepable_prompt("Unified diff (only + lines are new):"));
        assert!(!is_keepable_prompt("=== DIFF: package.json ==="));
        assert!(!is_keepable_prompt("@@ -54,7 +54,7 @@"));
        assert!(!is_keepable_prompt("- package.json"));
        assert!(!is_keepable_prompt(
            "- src/core/application/gorgias/use-cases/create-ticket.ts"
        ));
    }

    #[test]
    fn prompt_collector_keeps_only_the_intent_before_a_diff_dump() {
        let mut pc = PromptCollector::default();
        pc.add(
            "Review this change for security vulnerabilities.\n\
             Changed files (you may Read these and any other file in the repo):\n\
             - package.json\n\
             Unified diff (only + lines are new):\n\
             === DIFF: package.json ===\n\
             @@ -54,7 +54,7 @@\n\
             +    \"@fifty-git/shared\": \"0.53.40\",\n",
        );
        let snips = pc.snippets();
        assert_eq!(snips, ["Review this change for security vulnerabilities."]);
        assert!(snips
            .iter()
            .all(|s| !s.contains("DIFF") && !s.contains("@@")));
    }

    /// Regression for the anti-recursion hole found in the 2026-08-13 audit sidecar:
    /// `grok_sessions` carried autostand's own render prompt — system prompt, context
    /// labels, section headings, the fence, and the preset skeleton — and every line was
    /// fed back to the model as work the user had done.
    #[test]
    fn prompt_collector_drops_our_own_render_prompt() {
        let mut pc = PromptCollector::default();
        pc.add(
            "## Source hierarchy (most authoritative first)\n\
             1. GIT FACTS — committed work. Authoritative for what was committed and when.\n\
             ## Rules\n\
             - Past tense, concrete, English.\n\
             - NEVER attribute to AI. Write as if the human did the work.\n\
             Filing date: 2026-08-03\n\
             Subtitle: _Work completed August 01–02, 2026._\n\
             ## GIT FACTS\n\
             ## OUTPUT\n\
             Format your output using this structure:\n\
             ```\n\
             **Yesterday**\n\
             - <what you did>\n",
        );
        assert!(
            pc.snippets().is_empty(),
            "render prompt leaked back as work: {:?}",
            pc.snippets()
        );
    }

    #[test]
    fn prompt_collector_drops_a_render_request_by_its_sentinel() {
        let mut pc = PromptCollector::default();
        pc.add("# Standup render request\n\nFiling date: 2026-08-13\n");
        assert!(pc.snippets().is_empty());
    }

    /// The guard must not swallow real work that merely mentions a standup.
    #[test]
    fn prompt_collector_keeps_real_work_alongside_the_guard() {
        let mut pc = PromptCollector::default();
        pc.add("Implementing August 13th fix for POST /api/v1/external/gorgias/tickets/claim");
        assert_eq!(
            pc.snippets(),
            ["Implementing August 13th fix for POST /api/v1/external/gorgias/tickets/claim"]
        );
    }

    #[test]
    fn window_contains_inside_inclusive() {
        let w = window((2026, 8, 3), (2026, 8, 7));
        assert!(window_contains(
            &w,
            NaiveDate::from_ymd_opt(2026, 8, 3).unwrap()
        ));
        assert!(window_contains(
            &w,
            NaiveDate::from_ymd_opt(2026, 8, 7).unwrap()
        ));
        assert!(window_contains(
            &w,
            NaiveDate::from_ymd_opt(2026, 8, 5).unwrap()
        ));
    }

    #[test]
    fn window_contains_outside() {
        let w = window((2026, 8, 3), (2026, 8, 7));
        assert!(!window_contains(
            &w,
            NaiveDate::from_ymd_opt(2026, 8, 2).unwrap()
        ));
        assert!(!window_contains(
            &w,
            NaiveDate::from_ymd_opt(2026, 8, 8).unwrap()
        ));
    }

    #[test]
    fn prompt_collector_dedups_and_filters() {
        let mut pc = PromptCollector::default();
        pc.add("fixed the login redirect bug");
        pc.add("fixed the login   redirect   bug");
        pc.add("/clear");
        pc.add("reviewed PR #42 for the auth module");
        assert_eq!(pc.snippets().len(), 2);
        assert_eq!(pc.snippets()[0], "fixed the login redirect bug");
        assert_eq!(pc.snippets()[1], "reviewed PR #42 for the auth module");
    }

    #[test]
    fn prompt_collector_caps_at_forty() {
        let mut pc = PromptCollector::default();
        for i in 0..60 {
            pc.add(&format!("unique prompt number {i}"));
        }
        assert_eq!(pc.snippets().len(), 40);
        assert!(pc.is_full());
        assert_eq!(pc.snippets().last().unwrap(), "unique prompt number 39");
    }

    #[test]
    fn prompt_collector_dedups_across_snippets_at_line_level() {
        let mut pc = PromptCollector::default();
        pc.add("ESTABLISHED FACTS (do not re-derive, build on these):\n- Repo A: /tmp/a\n- Repo B: /tmp/b\nwrote the login redirect bug");
        pc.add("ESTABLISHED FACTS (do not re-derive, build on these):\n- Repo A: /tmp/a\n- Repo B: /tmp/b\nreviewed PR #42 for the auth module");
        pc.add("ESTABLISHED FACTS (do not re-derive, build on these):\n- Repo A: /tmp/a\n- Repo B: /tmp/b\nwrote the login redirect bug");
        let snips = pc.snippets();
        assert_eq!(snips.len(), 2);
        assert!(snips.iter().all(|s| !s.contains("ESTABLISHED FACTS")));
        assert!(snips.iter().all(|s| !s.contains("Repo A")));
    }

    #[test]
    fn prompt_collector_dedups_repeated_lines_across_adds() {
        let mut pc = PromptCollector::default();
        pc.add("wired the pipeline");
        pc.add("wired the pipeline");
        pc.add("wired the pipeline");
        assert_eq!(pc.snippets().len(), 1);
        assert_eq!(pc.snippets()[0], "wired the pipeline");
    }

    #[test]
    fn prompt_collector_truncates_long_snippets() {
        let mut pc = PromptCollector::default();
        let long = "a".repeat(500);
        pc.add(&long);
        assert_eq!(pc.snippets().len(), 1);
        assert_eq!(pc.snippets()[0].chars().count(), 200);
        assert!(pc.snippets()[0].ends_with('…'));
    }

    #[test]
    fn prompt_collector_render_context_with_plans_and_snippets() {
        let mut pc = PromptCollector::default();
        pc.add("worked on feature X");
        let out = pc.render_context(&["plan alpha".to_string(), "plan beta".to_string()]);
        assert!(out.starts_with("## CONTEXT\n"));
        assert!(out.contains("plans:\n"));
        assert!(out.contains("- plan alpha"));
        assert!(out.contains("- plan beta"));
        assert!(out.contains("prompts:\n"));
        assert!(out.contains("- worked on feature X"));
    }

    #[test]
    fn prompt_collector_render_context_empty() {
        let pc = PromptCollector::default();
        let out = pc.render_context(&[]);
        assert!(out.contains("## CONTEXT"));
        assert!(out.contains("prompts:\n_(none)_"));
    }

    #[test]
    fn attribute_file_matches_repo_under_github_dir() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("myrepo");
        fs::create_dir_all(repo.join(".git")).unwrap();

        let file_path = tmp
            .path()
            .join("myrepo")
            .join("src")
            .join("main.rs")
            .to_string_lossy()
            .to_string();
        let (name, basename) = attribute_file(&file_path, tmp.path()).unwrap();
        assert_eq!(name, "myrepo");
        assert_eq!(basename, "main.rs");
    }

    #[test]
    fn attribute_file_returns_none_when_not_under_any_repo() {
        let tmp = TempDir::new().unwrap();
        let repo = tmp.path().join("myrepo");
        fs::create_dir_all(repo.join(".git")).unwrap();

        let outside = tmp.path().join("elsewhere").join("file.txt");
        assert!(attribute_file(&outside.to_string_lossy(), tmp.path()).is_none());
    }

    #[test]
    fn attribute_file_empty_path_returns_none() {
        let tmp = TempDir::new().unwrap();
        assert!(attribute_file("", tmp.path()).is_none());
    }

    #[test]
    fn attribute_file_requires_git_dir() {
        let tmp = TempDir::new().unwrap();

        let dir_without_git = tmp.path().join("notarepo");
        fs::create_dir(&dir_without_git).unwrap();
        let file_path = dir_without_git
            .join("file.txt")
            .to_string_lossy()
            .to_string();
        assert!(attribute_file(&file_path, tmp.path()).is_none());
    }

    #[test]
    fn read_jsonl_paths_finds_files_recursively_sorted() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.jsonl"), "{}\n{}\n").unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("b.jsonl"), "{}\n").unwrap();
        fs::write(tmp.path().join("ignore.txt"), "nope").unwrap();

        let paths = read_jsonl_paths(tmp.path());
        assert_eq!(paths.len(), 2);
        let names: Vec<String> = paths
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["a.jsonl", "b.jsonl"]);
    }

    #[test]
    fn read_jsonl_paths_empty_when_no_files() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("readme.md"), "hi").unwrap();
        assert!(read_jsonl_paths(tmp.path()).is_empty());
    }

    #[tokio::test]
    async fn run_cmd_times_out_on_long_running_command() {
        let start = std::time::Instant::now();
        let res = run_cmd("sleep", &["10"], 1).await;
        assert!(res.is_err());
        let err = res.unwrap_err();
        assert!(
            err.contains("timeout"),
            "expected timeout message, got: {err}"
        );
        assert!(start.elapsed().as_secs() < 3);
    }

    #[tokio::test]
    async fn run_cmd_returns_stdout_on_success() {
        let res = run_cmd("echo", &["hello"], 5).await;
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "hello");
    }

    #[tokio::test]
    async fn run_cmd_errors_when_binary_missing() {
        let res = run_cmd("definitely-not-a-real-binary-xyz", &[], 5).await;
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("spawn"));
    }

    /// Every `run_cmd` caller loops over repositories; a per-spawn line would
    /// bury the run, so the sweep summary is the only thing that may show up.
    #[tokio::test]
    async fn run_cmd_stays_out_of_the_terminal() {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        let out = scoped(as_ref, async { run_cmd("echo", &["quiet"], 5).await }).await;
        assert_eq!(out.unwrap(), "quiet");
        assert!(sink.lines().is_empty(), "{:?}", sink.messages());
    }

    #[tokio::test]
    async fn a_visible_call_reaches_the_terminal_without_its_argv() {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        let out = scoped(as_ref, async {
            run_cmd_visible(
                "echo",
                &["--owner=acme-private", "/Users/someone/work"],
                5,
                "gh search prs",
            )
            .await
        })
        .await;
        assert!(out.unwrap().contains("acme-private"), "stdout is returned");
        let rendered = sink
            .lines()
            .iter()
            .map(|line| format!("{} {}", line.step, line.message))
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("gh search prs"), "{rendered}");
        assert!(rendered.contains(GATHER_STEP), "{rendered}");
        assert!(
            !rendered.contains("acme-private"),
            "argv leaked: {rendered}"
        );
        assert!(!rendered.contains("/Users/"), "path leaked: {rendered}");
    }

    /// `Gathered::failures` is rendered straight into the panel, so a data
    /// source's error string may never carry the child's stderr.
    #[tokio::test]
    async fn a_failing_command_reports_its_exit_code_and_no_stderr() {
        let err = run_cmd("sh", &["-c", "echo fatal-credential-hint 1>&2; exit 7"], 5)
            .await
            .expect_err("non-zero exit");
        assert!(err.contains("exited 7"), "{err}");
        assert!(!err.contains("fatal-credential-hint"), "{err}");
    }

    #[tokio::test]
    async fn a_sweep_summary_is_one_done_line() {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        scoped(as_ref, async {
            log_sweep("local-git scanned", "3 repo(s)");
        })
        .await;
        let lines = sink.lines();
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].step, GATHER_STEP);
        assert_eq!(lines[0].detail.as_deref(), Some("3 repo(s)"));
    }

    /// `DataSource::is_available` is synchronous and is called from inside the
    /// async gather, so it must answer without spawning anything.
    #[test]
    fn a_program_is_located_on_path_without_spawning_it() {
        let dir = TempDir::new().unwrap();
        let program = dir.path().join("pretend-cli");
        fs::write(&program, "#!/bin/sh\nexit 1\n").unwrap();
        let search = vec![dir.path().to_path_buf()];

        // A file that would exit non-zero is still "available": whether it works
        // is the first real call's answer, not the lookup's.
        assert_eq!(
            autostand_runlog::which::find_program_in("pretend-cli", &search),
            Some(program),
        );
        assert!(autostand_runlog::which::find_program_in(
            "definitely-not-a-real-binary-xyz",
            &search
        )
        .is_none(),);
    }

    #[test]
    fn scan_repos_finds_dirs_with_git() {
        let tmp = TempDir::new().unwrap();
        for name in ["alpha", "beta", "gamma"] {
            fs::create_dir_all(tmp.path().join(name).join(".git")).unwrap();
        }
        fs::create_dir(tmp.path().join("delta")).unwrap();

        let repos = scan_repos(tmp.path());
        let names: Vec<String> = repos
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
    }

    #[test]
    fn scan_repos_empty_when_dir_missing() {
        let missing = PathBuf::from("/this/does/not/exist/at/all");
        let repos = scan_repos(&missing);
        assert!(repos.is_empty());
    }
}
