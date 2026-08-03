//! Pure-Rust deterministic renderer (always-on fallback).
//!
//! See `docs/specs/pipeline.md` step 3l. The LLM is an enhancement; this is the
//! dependency-free renderer that always produces a standup from FACTS + NOTES.

use crate::Result;
use regex::Regex;
use std::fmt::Write as _;

/// Render a deterministic standup body from structured inputs (no LLM).
///
/// Builds section headers from the FACTS block, appends notes, GitHub activity,
/// Claude Code context, and PR review sections as available. Returns just the
/// AUTO block body (no `<!-- AUTO -->` markers).
pub fn render_det(
    facts: &str,
    github: Option<&str>,
    notes: &str,
    conv: Option<&str>,
    prrev: Option<&str>,
    jira_base: &str,
) -> Result<String> {
    let mut out = String::new();

    for section in parse_facts(facts) {
        for (ticket, title, subjects) in group_by_ticket(&section.commits) {
            push_section_break(&mut out);
            match ticket {
                Some(t) => {
                    let _ = writeln!(
                        out,
                        "**{} — [{}]({}/{}) — {}**",
                        section.repo, t, jira_base, t, title
                    );
                }
                None => {
                    let _ = writeln!(out, "**{}**", section.repo);
                }
            }
            for subject in &subjects {
                out.push_str("- ");
                out.push_str(subject);
                out.push('\n');
            }
        }
    }

    let note_clauses = extract_clauses(notes);
    if !note_clauses.is_empty() {
        push_section_break(&mut out);
        out.push_str("**General — Notes**\n");
        for clause in &note_clauses {
            out.push_str("- ");
            out.push_str(clause);
            out.push('\n');
        }
    }

    if let Some(gh) = github.map(str::trim).filter(|s| !s.is_empty()) {
        push_section_break(&mut out);
        out.push_str("## GITHUB ACTIVITY\n");
        out.push_str(gh);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }

    if let Some(c) = conv.map(str::trim).filter(|s| !s.is_empty()) {
        push_section_break(&mut out);
        out.push_str("**Claude Code context**\n");
        out.push_str(c);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }

    if let Some(pr) = prrev.map(str::trim).filter(|s| !s.is_empty()) {
        push_section_break(&mut out);
        out.push_str("**PR Review**\n");
        out.push_str(pr);
        if !out.ends_with('\n') {
            out.push('\n');
        }
    }

    Ok(out)
}

struct ParsedRepo {
    repo: String,
    commits: Vec<(Option<String>, String)>,
}

fn parse_facts(facts: &str) -> Vec<ParsedRepo> {
    let mut repos: Vec<ParsedRepo> = Vec::new();
    let mut current: Option<usize> = None;

    for line in facts.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("### repo: ") {
            let repo_name = rest.split(" / ").next().unwrap_or(rest).trim().to_string();
            repos.push(ParsedRepo {
                repo: repo_name,
                commits: Vec::new(),
            });
            current = Some(repos.len() - 1);
        } else if let Some(idx) = current {
            if let Some(rest) = trimmed.strip_prefix("- ") {
                let (ticket, subject) = parse_bullet(rest);
                repos[idx].commits.push((ticket, subject));
            }
        }
    }

    repos
}

fn parse_bullet(rest: &str) -> (Option<String>, String) {
    if let Some(after_paren) = rest.strip_prefix('(') {
        if let Some(close) = after_paren.find(')') {
            let ticket = after_paren[..close].trim().to_string();
            let subject = after_paren[close + 1..].trim().to_string();
            if is_ticket(&ticket) {
                return (Some(ticket), subject);
            }
        }
    }
    (None, rest.trim().to_string())
}

fn is_ticket(s: &str) -> bool {
    let re = Regex::new(r"^[A-Z][A-Z0-9]+-\d+$").expect("ticket regex");
    re.is_match(s)
}

fn group_by_ticket(
    commits: &[(Option<String>, String)],
) -> Vec<(Option<String>, String, Vec<String>)> {
    let mut groups: Vec<(Option<String>, String, Vec<String>)> = Vec::new();
    for (ticket, subject) in commits {
        if let Some(group) = groups.iter_mut().find(|(t, _, _)| t == ticket) {
            group.2.push(subject.clone());
        } else {
            let title = subject.clone();
            groups.push((ticket.clone(), title, vec![subject.clone()]));
        }
    }
    groups
}

fn extract_clauses(notes: &str) -> Vec<String> {
    notes
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("- ") {
                let clause = rest.trim().to_string();
                if !clause.is_empty() {
                    return Some(clause);
                }
            }
            if let Some(rest) = trimmed.strip_prefix("* ") {
                let clause = rest.trim().to_string();
                if !clause.is_empty() {
                    return Some(clause);
                }
            }
            None
        })
        .collect()
}

fn push_section_break(out: &mut String) {
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    if !out.is_empty() {
        out.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_repos_from_facts() {
        let facts = "\
### repo: autostand / tickets: FIF-133 / commits (2):
- (FIF-133) Implemented core domain model
- (FIF-133) Wrote architecture docs
### repo: my-repo / tickets: FIF-140 / commits (1):
- (FIF-140) Fixed queue race condition
";
        let body = render_det(
            facts,
            None,
            "",
            None,
            None,
            "https://jira.example.com/browse",
        )
        .expect("render");
        assert!(body.contains("**autostand — [FIF-133](https://jira.example.com/browse/FIF-133) — Implemented core domain model**"));
        assert!(body.contains("- Implemented core domain model"));
        assert!(body.contains("- Wrote architecture docs"));
        assert!(body.contains("**my-repo — [FIF-140](https://jira.example.com/browse/FIF-140) — Fixed queue race condition**"));
        assert!(body.contains("- Fixed queue race condition"));
    }

    #[test]
    fn renders_general_from_notes() {
        let notes = "- Drafted design system spec\n* Discussed API contract\nnot a bullet";
        let body = render_det("", None, notes, None, None, "").expect("render");
        assert!(body.contains("**General — Notes**"));
        assert!(body.contains("- Drafted design system spec"));
        assert!(body.contains("- Discussed API contract"));
        assert!(!body.contains("not a bullet"));
    }

    #[test]
    fn empty_facts_returns_empty() {
        let body = render_det("", None, "", None, None, "").expect("render");
        assert!(body.is_empty());
    }

    #[test]
    fn renders_github_and_pr_review() {
        let github = "PR #12 opened in autostand";
        let prrev = "- autostand #12 — \"Add IPC\" (by @dev) — Approved";
        let body = render_det("", Some(github), "", None, Some(prrev), "").expect("render");
        assert!(body.contains("## GITHUB ACTIVITY"));
        assert!(body.contains("PR #12 opened in autostand"));
        assert!(body.contains("**PR Review**"));
        assert!(body.contains("Approved"));
    }
}
