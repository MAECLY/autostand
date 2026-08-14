//! `autostand-adapters` — LLM provider adapters + data source adapters.
//!
//! See `docs/llm-adapters/` and `docs/data-sources/` for full specs.
//!
//! [`usage`] is a separate contract from [`llm`]: only a subset of providers
//! expose subscription quota, so probes live behind their own trait rather than
//! forcing every adapter to implement a no-op. See `docs/specs/provider-usage.md`.

#![forbid(unsafe_code)]

pub mod llm;
pub mod sources;
pub mod usage;

/// Re-export core types for convenience.
pub use autostand_core as core;
