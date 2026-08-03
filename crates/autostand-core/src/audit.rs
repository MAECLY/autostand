//! Provenance audit sidecar writer + phantom detector.
//!
//! See `docs/specs/audit.md`.

use crate::scrub::CLAIM_REGEX;
use crate::textsim;
use chrono::{DateTime, Utc};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};

use crate::Result;

/// Audit sidecar written per render to `state/audit/<F>-<HOST>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditData {
    pub file: String,
    pub host: String,
    pub rendered_at: DateTime<Utc>,
    pub window: DateRange,
    pub forbidden_tickets: Vec<String>,
    pub covered_tickets: Vec<String>,
    pub skew: Vec<SkewRecord>,
    pub ticket_days: HashMap<String, Vec<chrono::NaiveDate>>,
    pub render_mode: String,
    pub render_used: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub fellback: bool,
    pub hash: String,
    pub accumulated_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DateRange {
    pub start: chrono::NaiveDate,
    pub end: chrono::NaiveDate,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkewRecord {
    pub ticket: String,
    pub note_date: chrono::NaiveDate,
    pub commit_days: Vec<chrono::NaiveDate>,
}

/// Classification of an AUTO bullet for phantom detection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Classification {
    Commit,
    Github,
    Review,
    Note,
    Phantom,
    Unverified,
}

impl Classification {
    pub fn label(self) -> &'static str {
        match self {
            Self::Commit => "commit",
            Self::Github => "github",
            Self::Review => "review",
            Self::Note => "note",
            Self::Phantom => "phantom",
            Self::Unverified => "unverified",
        }
    }
}

/// Classify a bullet against audit data using provenance + textsim matching.
///
/// Order: Commit → Github → Review → Note → Phantom → Unverified.
pub fn classify(bullet: &str, audit: &AuditData) -> Classification {
    let tickets = parse_tickets(bullet);
    let claim = Regex::new(CLAIM_REGEX).expect("claim regex");

    for ticket in &tickets {
        if audit.covered_tickets.iter().any(|t| t == ticket) {
            if let Some(days) = audit.ticket_days.get(ticket) {
                let in_window = days
                    .iter()
                    .any(|d| *d >= audit.window.start && *d <= audit.window.end);
                if in_window {
                    return Classification::Commit;
                }
            }
        }
    }

    let has_pr_marker = Regex::new(r"#\d+").expect("pr marker regex");
    if has_pr_marker.is_match(bullet) {
        let pr_ticket_seen = tickets
            .iter()
            .any(|t| audit.covered_tickets.iter().any(|c| c == t));
        if pr_ticket_seen || bullet.contains("PR") {
            return Classification::Github;
        }
    }

    if looks_like_review(bullet) {
        return Classification::Review;
    }

    if looks_like_note(bullet) {
        if textsim::covered(bullet, &audit.covered_tickets.join(" "))
            || audit
                .covered_tickets
                .iter()
                .any(|t| textsim::covered(bullet, t))
        {
            return Classification::Note;
        }
        if tickets.is_empty() {
            return Classification::Note;
        }
    }

    if claim.is_match(bullet) {
        for ticket in &tickets {
            if audit.forbidden_tickets.iter().any(|t| t == ticket) {
                return Classification::Phantom;
            }
        }
    }

    Classification::Unverified
}

fn looks_like_review(bullet: &str) -> bool {
    let lower = bullet.to_lowercase();
    lower.contains("reviewed") || lower.contains("approved") || lower.contains("changes requested")
}

fn looks_like_note(bullet: &str) -> bool {
    bullet.contains("General") || bullet.contains("Notes")
}

fn parse_tickets(text: &str) -> Vec<String> {
    let re = Regex::new(r"\b([A-Z][A-Z0-9]+-\d+)\b").expect("ticket regex");
    re.find_iter(text).map(|m| m.as_str().to_string()).collect()
}

/// Write the audit sidecar atomically to `state_dir/audit/<file>-<host>.json`.
///
/// Creates the `audit/` directory if missing. Writes to `<path>.tmp`, fsyncs,
/// renames into place, then fsyncs the parent directory. Returns the final path.
pub fn write_sidecar(audit: &AuditData, state_dir: &Path) -> Result<PathBuf> {
    let audit_dir = state_dir.join("audit");
    std::fs::create_dir_all(&audit_dir)?;

    let filename = format!("{}-{}.json", audit.file, audit.host);
    let final_path = audit_dir.join(&filename);
    let tmp_path = audit_dir.join(format!("{filename}.tmp"));

    let bytes = serde_json::to_vec_pretty(audit)?;
    write_atomic_inner(&tmp_path, &final_path, &bytes)?;

    if let Some(parent) = final_path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }

    Ok(final_path)
}

/// Read + deserialize an audit sidecar.
pub fn read_sidecar(path: &Path) -> Result<AuditData> {
    let bytes = std::fs::read(path)?;
    Ok(serde_json::from_slice(&bytes)?)
}

fn write_atomic_inner(tmp: &Path, final_path: &Path, bytes: &[u8]) -> Result<()> {
    {
        let mut file = std::fs::File::create(tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(tmp, final_path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn sample_audit() -> AuditData {
        let mut ticket_days = HashMap::new();
        ticket_days.insert(
            "FIF-133".to_string(),
            vec![NaiveDate::from_ymd_opt(2026, 8, 1).unwrap()],
        );
        AuditData {
            file: "2026-08-03".into(),
            host: "test-host".into(),
            rendered_at: Utc::now(),
            window: DateRange {
                start: NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
                end: NaiveDate::from_ymd_opt(2026, 8, 2).unwrap(),
            },
            forbidden_tickets: vec!["FIF-666".into()],
            covered_tickets: vec!["FIF-133".into()],
            skew: vec![],
            ticket_days,
            render_mode: "det".into(),
            render_used: "det".into(),
            provider: None,
            model: None,
            fellback: false,
            hash: "abc".into(),
            accumulated_count: 0,
        }
    }

    #[test]
    fn classifies_commit_bullet() {
        let audit = sample_audit();
        let bullet = "fixed FIF-133 parser bug";
        assert_eq!(classify(bullet, &audit), Classification::Commit);
    }

    #[test]
    fn classifies_phantom_bullet() {
        let audit = sample_audit();
        let bullet = "merged FIF-666 into main";
        assert_eq!(classify(bullet, &audit), Classification::Phantom);
    }

    #[test]
    fn classifies_unverified_when_no_match() {
        let audit = sample_audit();
        let bullet = "attended architecture review";
        assert_eq!(classify(bullet, &audit), Classification::Unverified);
    }

    #[test]
    fn write_read_roundtrip() {
        let dir = std::env::temp_dir().join(format!("autostand-audit-test-{}", std::process::id()));
        let subdir = dir.join(format!(
            "audit-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let audit = sample_audit();
        let path = write_sidecar(&audit, &subdir).expect("write");
        let read_back = read_sidecar(&path).expect("read");
        assert_eq!(read_back.file, audit.file);
        assert_eq!(read_back.host, audit.host);
        assert_eq!(read_back.forbidden_tickets, audit.forbidden_tickets);
        let _ = std::fs::remove_dir_all(&subdir);
    }
}
