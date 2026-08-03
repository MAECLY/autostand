//! Pure-Rust deterministic renderer (always-on fallback).
//!
//! See `docs/specs/pipeline.md`. The LLM is an enhancement; this is the dependency-free
//! renderer that always produces a standup from FACTS + NOTES.

use crate::Result;

/// Render a deterministic standup body from structured inputs.
///
/// TODO: full impl per `docs/specs/pipeline.md` step 3l.
pub fn render_det(
    _facts: &str,
    _github: Option<&str>,
    _notes: &str,
    _conv: Option<&str>,
    _prrev: Option<&str>,
) -> Result<String> {
    tracing::info!("render_det stub");
    Ok(String::new())
}
