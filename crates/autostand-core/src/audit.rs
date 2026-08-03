//! Provenance audit sidecar writer + phantom detector.
//!
//! See `docs/specs/audit.md`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Classify a bullet against audit data (stub — full impl uses textsim).
pub fn classify(_bullet: &str, _audit: &AuditData) -> Classification {
    Classification::Unverified
}
