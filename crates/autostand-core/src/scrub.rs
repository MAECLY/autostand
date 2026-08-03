//! Anti-backdate scrub (stub — see `docs/specs/anti-backdating.md`).
//!
//! Drops clauses asserting committed work (CLAIM regex) + FORBIDDEN/COVERED tickets.

/// Regex matching committed-work claims: `commits|committed|PR|merged|pushed`.
pub const CLAIM_REGEX: &str = r"\b(commits?|committed|PR|merged|pushed)\b";

/// Scrub notes: drop CLAIM clauses + FORBIDDEN/COVERED ticket clauses + meta-work.
pub fn scrub_notes(
    notes: &str,
    forbidden: &[String],
    covered: &[String],
    meta_extra: Option<&str>,
) -> String {
    let claim = regex::Regex::new(CLAIM_REGEX).expect("claim regex");
    let mut out = String::new();
    for line in notes.lines() {
        if claim.is_match(line) {
            continue;
        }
        if forbidden.iter().any(|t| line.contains(t.as_str())) {
            continue;
        }
        if covered.iter().any(|t| line.contains(t.as_str())) {
            continue;
        }
        if crate::meta::is_meta_work(line, meta_extra) {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drops_claim_clauses() {
        let notes = "committed FIF-133\nfixed bug\npushed code\nreviewed PR";
        let out = scrub_notes(notes, &[], &[], None);
        assert!(out.contains("fixed bug"));
        assert!(!out.contains("committed"));
        assert!(!out.contains("pushed"));
        assert!(!out.contains("PR"));
    }

    #[test]
    fn drops_forbidden_tickets() {
        let notes = "worked on FIF-133\nreviewed FIF-140";
        let out = scrub_notes(notes, &["FIF-133".into()], &[], None);
        assert!(!out.contains("FIF-133"));
        assert!(out.contains("FIF-140"));
    }
}
