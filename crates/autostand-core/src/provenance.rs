//! Provenance: `FORBIDDEN` / `COVERED` / `SKEW` classification — the anti-backdating heart.
//!
//! See `docs/specs/anti-backdating.md` and step (f) of `docs/specs/pipeline.md`.
//!
//! A standup is a claim that work happened inside a specific window. Narrative notes
//! (`today-*.md`, `now.md`) roll forward for days, so they routinely restate work git
//! already recorded on a *different* day. [`compute`] turns the gathered inputs into the
//! four provenance artefacts the rest of the pipeline consumes:
//!
//! - `range_tickets` — tickets appearing in the in-window GIT FACTS.
//! - `FORBIDDEN` — tickets named in the notes whose commits ALL fall outside the window
//!   (cross-day backdating); [`crate::scrub::scrub_notes`] drops those clauses.
//! - `COVERED` — tickets git (in window) or the GitHub enrichment already owns, so a note
//!   must not restate them (git/github own the narrative).
//! - `SKEW` — the per-ticket signal recorded in the audit sidecar and logged as a warning.
//!
//! `FORBIDDEN`/`COVERED` are *rules* (they drive the scrub); `SKEW` is a *signal* (it never
//! drops anything on its own — a note may legitimately describe non-commit work on a ticket
//! whose code landed earlier).

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::OnceLock;

use chrono::NaiveDate;
use regex::Regex;

use crate::audit::SkewRecord;

/// Jira-style ticket key pattern, identical for FACTS, NOTES and `all_git_tickets`.
///
/// Requires at least two leading uppercase alphanumerics, so `A-1`, `v1.2-3`, `1.2.3-4`
/// and `#123` (PR references) can never match.
pub const TICKET_REGEX: &str = r"\b[A-Z][A-Z0-9]+-\d+\b";

/// Key prefixes that have the shape of a ticket but never are one.
///
/// WHY: technical-standard and version identifiers (`UTF-8`, `ISO-8601`, `SHA-256`,
/// `RFC-7231`, `CVE-2026-1`) satisfy [`TICKET_REGEX`]. Treating them as tickets would let a
/// note mentioning `UTF-8` be scrubbed as "covered", or worse, mark a standard as
/// `FORBIDDEN`. `PR` is denied because `PR-45` is a pull-request reference, not a ticket.
const NON_TICKET_PREFIXES: &[&str] = &[
    "AES", "ANSI", "ASCII", "BASE", "CVE", "CWE", "ECMA", "HMAC", "HTTP", "HTTPS", "IPV", "ISO",
    "MD", "PR", "RFC", "RSA", "SHA", "SSL", "TLS", "UTF", "WCAG",
];

/// The provenance verdict for one compile window.
///
/// Consumed by [`crate::scrub::scrub_notes`] (forbidden/covered) and by the audit sidecar
/// ([`crate::audit::AuditData`]) for `skew` + `ticket_days`.
#[derive(Debug, Clone, Default)]
pub struct Provenance {
    /// Tickets named in the notes whose commits ALL fall outside the window, and which the
    /// in-window FACTS do not already own. Their note clauses are dropped by the scrub.
    pub forbidden_tickets: Vec<String>,
    /// Tickets authoritatively owned by the in-window GIT FACTS or by the GitHub
    /// enrichment. Note restatements of these are dropped (no duplication).
    pub covered_tickets: Vec<String>,
    /// Per-(ticket, note date) records where a note dated in-range names a ticket committed
    /// entirely outside the window. A signal only — nothing is dropped because of it.
    pub skew: Vec<SkewRecord>,
    /// Ticket → the days it has commits on, for every ticket relevant to this window.
    /// Carried into the audit sidecar so `audit::classify` can re-check provenance.
    pub ticket_days: HashMap<String, Vec<NaiveDate>>,
    /// Tickets appearing in the in-window GIT FACTS (committed inside the window).
    pub range_tickets: Vec<String>,
}

/// Extract Jira-style ticket keys from free text.
///
/// Returns unique keys in first-appearance order (stable across runs for the same input).
/// Keys whose prefix is a known non-ticket identifier ([`NON_TICKET_PREFIXES`]) are skipped.
pub fn extract_tickets(text: &str) -> Vec<String> {
    let mut seen: HashSet<&str> = HashSet::new();
    let mut out: Vec<String> = Vec::new();
    for m in ticket_re().find_iter(text) {
        let key = m.as_str();
        if is_non_ticket(key) {
            continue;
        }
        if seen.insert(key) {
            out.push(key.to_string());
        }
    }
    out
}

/// Compute the provenance verdict for one window.
///
/// - `facts` — the GIT FACTS block for the window (authoritative record of commits).
/// - `notes` — the raw narrative notes, pre-scrub. Sections are delimited by markdown
///   headings; a heading carrying an ISO date (`## 2026-08-03`, `### source: repo
///   (today-2026-08-03.md)`) dates every clause beneath it.
/// - `github` — the GitHub enrichment block, if gathered.
/// - `all_git_tickets` — ticket → commit days from `git log --all` (every ticket ever
///   touched). A ticket absent from this map has no git footprint at all, so it cannot be
///   backdating anything and is never `FORBIDDEN`.
/// - `window_dates` — the business days the standup is allowed to claim.
///
/// The two rules are deliberately asymmetric: `COVERED` is the union of FACTS + GitHub
/// tickets (not just the ones the notes mention) because the audit classifier and the LLM
/// render validation both need the full in-window ticket set.
// `implicit_hasher`: the caller is `pipeline::CompileInputs`, whose `ticket_days` field is a
// plain `HashMap` with the default hasher; generalizing over `S: BuildHasher` would buy
// nothing and would churn the signature every downstream agent codes against.
#[allow(clippy::implicit_hasher)]
#[tracing::instrument(skip_all, fields(window_days = window_dates.len()))]
pub fn compute(
    facts: &str,
    notes: &str,
    github: Option<&str>,
    all_git_tickets: &HashMap<String, Vec<NaiveDate>>,
    window_dates: &[NaiveDate],
) -> Provenance {
    let mut range_tickets = extract_tickets(facts);
    let note_tickets = extract_tickets(notes);
    let github_tickets = github.map(extract_tickets).unwrap_or_default();

    let window: HashSet<NaiveDate> = window_dates.iter().copied().collect();
    let range_set: HashSet<&str> = range_tickets.iter().map(String::as_str).collect();

    // COVERED = range_tickets ∪ github_tickets (git/github own the narrative for these).
    let mut covered: BTreeSet<&str> = range_tickets.iter().map(String::as_str).collect();
    covered.extend(github_tickets.iter().map(String::as_str));

    // FORBIDDEN + SKEW walk the notes section by section so each `SkewRecord` carries the
    // date of the note that named the ticket.
    let mut forbidden: BTreeSet<String> = BTreeSet::new();
    let mut skew: Vec<SkewRecord> = Vec::new();
    let mut skew_seen: HashSet<(String, NaiveDate)> = HashSet::new();
    let fallback_note_date = window_dates.last().copied();

    for section in split_note_sections(notes) {
        let note_date = section.date.or(fallback_note_date);
        for ticket in extract_tickets(&section.text) {
            let Some(days) = commit_days(all_git_tickets, &ticket) else {
                // No git footprint → nothing to backdate (e.g. a ticket tracked only in Jira).
                continue;
            };
            if days.iter().any(|d| window.contains(d)) {
                // At least one commit lands inside the window (boundary-straddling ticket):
                // the FACTS already cover it, so the note is not backdating.
                continue;
            }

            // Every commit for this ticket is outside the window.
            if !range_set.contains(ticket.as_str()) {
                forbidden.insert(ticket.clone());
            }

            // SKEW is defined for notes dated IN-range. A section with an explicit
            // out-of-window date is a stale carry-over, already handled by FORBIDDEN.
            let Some(note_date) = note_date.filter(|d| window.contains(d)) else {
                continue;
            };
            if skew_seen.insert((ticket.clone(), note_date)) {
                tracing::warn!(
                    ticket = %ticket,
                    note_date = %note_date,
                    commit_days = ?days,
                    "SKEW: note dated in-range names a ticket committed entirely outside the window"
                );
                skew.push(SkewRecord {
                    ticket,
                    note_date,
                    commit_days: days,
                });
            }
        }
    }

    let covered_tickets: Vec<String> = covered.into_iter().map(String::from).collect();
    let forbidden_tickets: Vec<String> = forbidden.into_iter().collect();

    // `ticket_days` is scoped to the tickets this window actually talks about; the full
    // `all_git_tickets` map would bloat every audit sidecar with the whole repo history.
    let mut ticket_days: HashMap<String, Vec<NaiveDate>> = HashMap::new();
    for ticket in range_tickets
        .iter()
        .chain(note_tickets.iter())
        .chain(github_tickets.iter())
    {
        if ticket_days.contains_key(ticket) {
            continue;
        }
        if let Some(days) = commit_days(all_git_tickets, ticket) {
            ticket_days.insert(ticket.clone(), days);
        }
    }

    range_tickets.sort();

    let provenance = Provenance {
        forbidden_tickets,
        covered_tickets,
        skew,
        ticket_days,
        range_tickets,
    };

    tracing::info!(
        range = provenance.range_tickets.len(),
        forbidden = provenance.forbidden_tickets.len(),
        covered = provenance.covered_tickets.len(),
        skew = provenance.skew.len(),
        "provenance computed"
    );
    provenance
}

/// One markdown section of the notes blob, with the date of its heading (if any).
struct NoteSection {
    date: Option<NaiveDate>,
    text: String,
}

/// Split notes into heading-delimited sections so each clause can be dated.
///
/// Content before the first heading becomes an undated leading section.
fn split_note_sections(notes: &str) -> Vec<NoteSection> {
    let mut sections: Vec<NoteSection> = Vec::new();
    for line in notes.lines() {
        if line.trim_start().starts_with('#') || sections.is_empty() {
            sections.push(NoteSection {
                date: if line.trim_start().starts_with('#') {
                    first_iso_date(line)
                } else {
                    None
                },
                text: String::new(),
            });
        }
        if let Some(section) = sections.last_mut() {
            section.text.push_str(line);
            section.text.push('\n');
        }
    }
    sections
}

/// First `YYYY-MM-DD` date in a line, if it parses as a real calendar date.
fn first_iso_date(line: &str) -> Option<NaiveDate> {
    static RE: OnceLock<Regex> = OnceLock::new();
    let re = RE.get_or_init(|| Regex::new(r"\b\d{4}-\d{2}-\d{2}\b").expect("iso date regex"));
    let m = re.find(line)?;
    NaiveDate::parse_from_str(m.as_str(), "%Y-%m-%d").ok()
}

/// Sorted, de-duplicated commit days for a ticket; `None` when it has no git footprint.
fn commit_days(
    all_git_tickets: &HashMap<String, Vec<NaiveDate>>,
    ticket: &str,
) -> Option<Vec<NaiveDate>> {
    let days = all_git_tickets.get(ticket)?;
    if days.is_empty() {
        return None;
    }
    Some(
        days.iter()
            .copied()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect(),
    )
}

/// Compiled [`TICKET_REGEX`], built once per process.
fn ticket_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(TICKET_REGEX).expect("ticket regex"))
}

/// True when a ticket-shaped match is a known non-ticket identifier (`UTF-8`, `PR-45`, …).
fn is_non_ticket(key: &str) -> bool {
    key.split_once('-')
        .is_some_and(|(prefix, _)| NON_TICKET_PREFIXES.contains(&prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    /// Window used by every test: Mon 2026-08-03 .. Tue 2026-08-04.
    fn window() -> Vec<NaiveDate> {
        vec![d(2026, 8, 3), d(2026, 8, 4)]
    }

    fn git_map(entries: &[(&str, &[NaiveDate])]) -> HashMap<String, Vec<NaiveDate>> {
        entries
            .iter()
            .map(|(t, days)| ((*t).to_string(), days.to_vec()))
            .collect()
    }

    // ── extract_tickets ────────────────────────────────────────────────────────

    #[test]
    fn extracts_jira_keys_in_first_appearance_order() {
        let text = "fixed FIF-133 and ABC1-9, then FIF-133 again";
        assert_eq!(extract_tickets(text), vec!["FIF-133", "ABC1-9"]);
    }

    #[test]
    fn extraction_rejects_near_miss_strings() {
        for near_miss in [
            "fif-133",      // lowercase
            "FIF133",       // no separator
            "FIF-",         // no number
            "F-1",          // single-letter prefix
            "v1.2-3",       // version number
            "1.2.3-4",      // version number
            "#123",         // PR reference
            "owner/repo#7", // PR reference
            "FIF-133a",     // trailing word char
            "release-2",    // lowercase project word
        ] {
            assert!(
                extract_tickets(near_miss).is_empty(),
                "should not extract a ticket from {near_miss:?}"
            );
        }
    }

    #[test]
    fn extraction_rejects_technical_standards_and_pr_refs() {
        let text = "encoded UTF-8, hashed SHA-256, per RFC-7231, reviewed PR-45";
        assert!(extract_tickets(text).is_empty());
    }

    #[test]
    fn extraction_keeps_tickets_next_to_punctuation() {
        assert_eq!(extract_tickets("(FIF-133)"), vec!["FIF-133"]);
        assert_eq!(extract_tickets("see FIF-133."), vec!["FIF-133"]);
        assert_eq!(
            extract_tickets("FIF-133/FIF-140"),
            vec!["FIF-133", "FIF-140"]
        );
    }

    // ── COVERED vs FORBIDDEN ───────────────────────────────────────────────────

    #[test]
    fn ticket_committed_inside_window_is_covered_not_forbidden() {
        let facts = "### repo: autostand\ntickets: FIF-133\n- (FIF-133) Implemented parser\n";
        let notes = "## 2026-08-03\n- polished FIF-133 error messages\n";
        let map = git_map(&[("FIF-133", &[d(2026, 8, 3)])]);

        let p = compute(facts, notes, None, &map, &window());

        assert_eq!(p.covered_tickets, vec!["FIF-133"]);
        assert!(p.forbidden_tickets.is_empty());
        assert!(p.skew.is_empty());
        assert_eq!(p.range_tickets, vec!["FIF-133"]);
    }

    #[test]
    fn note_about_last_weeks_ticket_is_forbidden() {
        let facts = "### repo: autostand\ntickets: FIF-133\n- (FIF-133) Implemented parser\n";
        let notes = "## 2026-08-03\n- pushed FIF-140 yesterday\n";
        let map = git_map(&[
            ("FIF-133", &[d(2026, 8, 3)]),
            ("FIF-140", &[d(2026, 7, 27), d(2026, 7, 28)]),
        ]);

        let p = compute(facts, notes, None, &map, &window());

        assert_eq!(p.forbidden_tickets, vec!["FIF-140"]);
        assert!(!p.covered_tickets.contains(&"FIF-140".to_string()));
    }

    #[test]
    fn ticket_with_commits_on_both_sides_of_the_boundary_is_not_forbidden() {
        // FIF-150 has commits before the window AND inside it: legitimate ongoing work.
        let facts = "### repo: autostand\n- (FIF-150) Continued work\n";
        let notes = "## 2026-08-04\n- debugged FIF-150 flakiness\n";
        let map = git_map(&[("FIF-150", &[d(2026, 7, 30), d(2026, 8, 4)])]);

        let p = compute(facts, notes, None, &map, &window());

        assert!(p.forbidden_tickets.is_empty());
        assert!(p.skew.is_empty(), "in-window commit means no skew");
        assert_eq!(p.covered_tickets, vec!["FIF-150"]);
    }

    #[test]
    fn ticket_without_git_footprint_is_neither_forbidden_nor_skewed() {
        let notes = "## 2026-08-03\n- ran discovery for FIF-999 with design\n";
        let p = compute("", notes, None, &HashMap::new(), &window());

        assert!(p.forbidden_tickets.is_empty());
        assert!(p.skew.is_empty());
        assert!(p.ticket_days.is_empty());
    }

    #[test]
    fn github_enrichment_tickets_are_covered() {
        let facts = "### repo: autostand\n- (FIF-133) Implemented parser\n";
        let github = "## GITHUB ACTIVITY\n- reviewed PR #42 on FIF-150\n";
        let notes = "## 2026-08-03\n- reviewed FIF-150 design doc\n";
        let map = git_map(&[("FIF-133", &[d(2026, 8, 3)])]);

        let p = compute(facts, notes, Some(github), &map, &window());

        assert_eq!(p.covered_tickets, vec!["FIF-133", "FIF-150"]);
        // FIF-150 has no git footprint at all → not forbidden, only covered.
        assert!(p.forbidden_tickets.is_empty());
    }

    // ── SKEW ───────────────────────────────────────────────────────────────────

    #[test]
    fn skew_is_recorded_with_the_note_date_and_commit_days() {
        let notes = "## 2026-08-04\n- worked on FIF-200 design\n";
        let map = git_map(&[("FIF-200", &[d(2026, 7, 31), d(2026, 7, 30), d(2026, 7, 31)])]);

        let p = compute("", notes, None, &map, &window());

        assert_eq!(p.skew.len(), 1);
        let record = &p.skew[0];
        assert_eq!(record.ticket, "FIF-200");
        assert_eq!(record.note_date, d(2026, 8, 4));
        // sorted + de-duplicated
        assert_eq!(record.commit_days, vec![d(2026, 7, 30), d(2026, 7, 31)]);
    }

    #[test]
    fn skew_without_forbidding_when_facts_already_own_the_ticket() {
        // FIF-168 reaches FACTS through the checked-out branch name, so it is a range
        // ticket (COVERED) even though every commit landed before the window. The spec
        // wants the SKEW signal recorded, but the ticket must NOT be forbidden.
        let facts = "### repo: autostand\ntickets: FIF-168\ncommits (0):\n";
        let notes = "## 2026-08-03\n- kept iterating on FIF-168 rollout plan\n";
        let map = git_map(&[("FIF-168", &[d(2026, 7, 17), d(2026, 7, 20)])]);

        let p = compute(facts, notes, None, &map, &window());

        assert_eq!(p.skew.len(), 1);
        assert_eq!(p.skew[0].ticket, "FIF-168");
        assert!(
            p.forbidden_tickets.is_empty(),
            "a range ticket is covered, never forbidden"
        );
        assert_eq!(p.covered_tickets, vec!["FIF-168"]);
    }

    #[test]
    fn skew_falls_back_to_the_window_end_when_the_note_has_no_heading() {
        let notes = "- carried FIF-200 investigation forward\n";
        let map = git_map(&[("FIF-200", &[d(2026, 7, 31)])]);

        let p = compute("", notes, None, &map, &window());

        assert_eq!(p.skew.len(), 1);
        assert_eq!(p.skew[0].note_date, d(2026, 8, 4));
        assert_eq!(p.forbidden_tickets, vec!["FIF-200"]);
    }

    #[test]
    fn out_of_window_note_section_forbids_without_skew() {
        // A stale carry-over dated before the window: still forbidden (its clause must go),
        // but it is not the "note dated in-range" case SKEW describes.
        let notes = "## 2026-07-20\n- pushed FIF-200 to staging\n";
        let map = git_map(&[("FIF-200", &[d(2026, 7, 20)])]);

        let p = compute("", notes, None, &map, &window());

        assert_eq!(p.forbidden_tickets, vec!["FIF-200"]);
        assert!(p.skew.is_empty());
    }

    #[test]
    fn each_section_dates_its_own_clauses() {
        let notes = "### source: repo-a (today-2026-08-03.md)\n- touched FIF-200\n\n\
                     ### source: repo-b (today-2026-08-04.md)\n- touched FIF-201\n";
        let map = git_map(&[
            ("FIF-200", &[d(2026, 7, 30)]),
            ("FIF-201", &[d(2026, 7, 31)]),
        ]);

        let p = compute("", notes, None, &map, &window());

        let mut dated: Vec<(String, NaiveDate)> = p
            .skew
            .iter()
            .map(|s| (s.ticket.clone(), s.note_date))
            .collect();
        dated.sort();
        assert_eq!(
            dated,
            vec![
                ("FIF-200".to_string(), d(2026, 8, 3)),
                ("FIF-201".to_string(), d(2026, 8, 4)),
            ]
        );
        assert_eq!(p.forbidden_tickets, vec!["FIF-200", "FIF-201"]);
    }

    #[test]
    fn repeated_mentions_produce_one_skew_record_per_ticket_and_date() {
        let notes = "## 2026-08-03\n- FIF-200 triage\n- more FIF-200 triage\n";
        let map = git_map(&[("FIF-200", &[d(2026, 7, 31)])]);

        let p = compute("", notes, None, &map, &window());

        assert_eq!(p.skew.len(), 1);
    }

    // ── ticket_days + ordering ─────────────────────────────────────────────────

    #[test]
    fn ticket_days_covers_relevant_tickets_only() {
        let facts = "- (FIF-133) parser\n";
        let notes = "## 2026-08-03\n- FIF-140 follow-up\n";
        let github = "- PR for FIF-150\n";
        let map = git_map(&[
            ("FIF-133", &[d(2026, 8, 3)]),
            ("FIF-140", &[d(2026, 7, 27)]),
            ("FIF-150", &[d(2026, 8, 4)]),
            ("FIF-900", &[d(2020, 1, 1)]), // irrelevant to this window
        ]);

        let p = compute(facts, notes, Some(github), &map, &window());

        let mut keys: Vec<&String> = p.ticket_days.keys().collect();
        keys.sort();
        assert_eq!(keys, vec!["FIF-133", "FIF-140", "FIF-150"]);
        assert_eq!(p.ticket_days["FIF-140"], vec![d(2026, 7, 27)]);
    }

    #[test]
    fn output_lists_are_sorted_and_deduplicated() {
        let facts = "- (ZZZ-9) later\n- (AAA-1) earlier\n- (ZZZ-9) again\n";
        let p = compute(facts, "", None, &HashMap::new(), &window());

        assert_eq!(p.range_tickets, vec!["AAA-1", "ZZZ-9"]);
        assert_eq!(p.covered_tickets, vec!["AAA-1", "ZZZ-9"]);
    }

    #[test]
    fn empty_inputs_produce_an_empty_verdict() {
        let p = compute("", "", None, &HashMap::new(), &window());
        assert!(p.range_tickets.is_empty());
        assert!(p.covered_tickets.is_empty());
        assert!(p.forbidden_tickets.is_empty());
        assert!(p.skew.is_empty());
        assert!(p.ticket_days.is_empty());
    }

    // ── integration with the scrub the verdict feeds ───────────────────────────

    #[test]
    fn verdict_drives_the_scrub() {
        let facts = "### repo: autostand\n- (FIF-133) Implemented parser\n";
        let notes = "## 2026-08-03\n\
                     - pushed FIF-140 yesterday\n\
                     - polished FIF-133 error messages\n\
                     - paired with design on the onboarding flow\n";
        let map = git_map(&[
            ("FIF-133", &[d(2026, 8, 3)]),
            ("FIF-140", &[d(2026, 7, 27)]),
        ]);

        let p = compute(facts, notes, None, &map, &window());
        let scrubbed =
            crate::scrub::scrub_notes(notes, &p.forbidden_tickets, &p.covered_tickets, None);

        assert!(
            !scrubbed.contains("FIF-140"),
            "forbidden clause must be dropped"
        );
        assert!(
            !scrubbed.contains("FIF-133"),
            "covered clause must be dropped"
        );
        assert!(
            scrubbed.contains("paired with design"),
            "real note work survives"
        );
    }
}
