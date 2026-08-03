//! Standup domain types: `StandupFile`, `AutoBlock`, `ManualRegion`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A parsed standup file (`dailies/YYYY-MM-DD.md`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StandupFile {
    /// Filing date (next business day after the work day).
    pub date: chrono::NaiveDate,
    /// Human-readable title, e.g. "August 03, 2026".
    pub title: String,
    /// Italic subtitle, e.g. "_Work completed August 01–02, 2026._"
    pub subtitle: String,
    /// One AUTO block per host, ordered by first-write.
    pub auto_blocks: Vec<AutoBlock>,
    /// The single global MANUAL region.
    pub manual: ManualRegion,
}

/// One host's AUTO block (`<!-- AUTO:<host>:START --> ... END -->`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoBlock {
    /// Host slug (stable, persisted, never DHCP-derived).
    pub host: String,
    /// Rendered body (sections + bullets).
    pub body: String,
}

/// The global MANUAL region (`<!-- MANUAL:START --> ... END -->`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ManualRegion {
    /// Hand-added items (never touched by auto-compile).
    pub body: String,
}

impl StandupFile {
    /// Get the AUTO block for a specific host, if present.
    pub fn auto_for(&self, host: &str) -> Option<&AutoBlock> {
        self.auto_blocks.iter().find(|b| b.host == host)
    }

    /// Get a mutable reference to the AUTO block for a specific host.
    pub fn auto_mut(&mut self, host: &str) -> Option<&mut AutoBlock> {
        self.auto_blocks.iter_mut().find(|b| b.host == host)
    }

    /// Per-host AUTO bodies as a map (host → body).
    pub fn auto_map(&self) -> BTreeMap<String, String> {
        self.auto_blocks
            .iter()
            .map(|b| (b.host.clone(), b.body.clone()))
            .collect()
    }
}
