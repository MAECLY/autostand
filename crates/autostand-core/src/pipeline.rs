//! Pipeline orchestrator (stub — see `docs/specs/pipeline.md`).
//!
//! gather → scrub → anti-backdate → render → accumulate → redact → write → audit.

use crate::Result;

/// Run the full pipeline for one filing date `F`.
///
/// TODO: implement per `docs/specs/pipeline.md` and `docs/architecture/03-data-flow.md`.
pub fn compile_file(_f: chrono::NaiveDate) -> Result<()> {
    tracing::info!("compile_file stub");
    Ok(())
}
