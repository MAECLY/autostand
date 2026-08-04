//! `autostand-core` — domain model, standup file format, and business rules.
//!
//! This crate is the heart of autostand. It contains:
//! - [`standup`] — `StandupFile`, `AutoBlock`, `ManualRegion` domain types.
//! - [`format`] — parser/writer for the AUTO/MANUAL block file format.
//! - [`dates`] — `next_business_day`, `prev_business_day_before`.
//! - [`host`] — host slug derivation + persistence (platform-stable, never DHCP).
//! - [`provenance`] — FORBIDDEN/COVERED/SKEW classification for anti-backdating.
//! - [`hashes`] — input hashing for the dirty check.
//! - [`pipeline`] — gather → scrub → anti-backdate → render → accumulate → redact → write → audit.
//! - [`scrub`] — anti-backdate scrub (CLAIM regex, FORBIDDEN/COVERED).
//! - [`meta`] — meta-work filter (standup tooling self-references).
//! - [`accumulate`] — never-delete: re-inject uncovered PREV bullets.
//! - [`redact`] — secrets redaction (regex-based, pre-LLM + pre-write).
//! - [`textsim`] — fuzzy text similarity (shared by accumulate + audit).
//! - [`audit`] — provenance sidecar writer + phantom detector.
//! - [`deterministic`] — pure-Rust deterministic renderer (fallback).
//!
//! See `docs/specs/` and `docs/architecture/` for full specifications.

#![forbid(unsafe_code)]

pub mod accumulate;
pub mod audit;
pub mod dates;
pub mod deterministic;
pub mod fileops;
pub mod format;
pub mod hashes;
pub mod host;
pub mod meta;
pub mod pipeline;
pub mod provenance;
pub mod redact;
pub mod scrub;
pub mod standup;
pub mod textsim;

/// Crate-level result alias.
pub type Result<T> = std::result::Result<T, crate::Error>;

/// Crate-level error.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serde error: {0}")]
    Serde(#[from] serde_json::Error),

    #[error("date error: {0}")]
    Date(String),

    #[error("format error: {0}")]
    Format(String),

    #[error("pipeline error: {0}")]
    Pipeline(String),

    #[error("other: {0}")]
    Other(#[from] anyhow::Error),
}
