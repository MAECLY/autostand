//! `autostand-adapters` — LLM provider adapters + data source adapters.
//!
//! See `docs/llm-adapters/` and `docs/data-sources/` for full specs.

#![forbid(unsafe_code)]

pub mod llm;
pub mod sources;

/// Re-export core types for convenience.
pub use autostand_core as core;
