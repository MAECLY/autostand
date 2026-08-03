//! Triggers: session-end, manual, launchd/systemd/Task Scheduler.

use serde::{Deserialize, Serialize};

/// What triggered a run.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum TriggerSource {
    /// Hourly system scheduler (launchd/systemd/Task Scheduler).
    Scheduler,
    /// AI coding CLI session ended (`Claude Code` / `OpenCode` / `Codex` etc.).
    SessionEnd,
    /// User clicked "Run now".
    Manual,
    /// App launched.
    AppStart,
}

impl TriggerSource {
    pub fn label(self) -> &'static str {
        match self {
            Self::Scheduler => "scheduler",
            Self::SessionEnd => "session-end",
            Self::Manual => "manual",
            Self::AppStart => "app-start",
        }
    }
}

/// Check the anti-recursion env guard. Returns true if it's safe to fire.
/// The `App Script`'s `SessionEnd` hook checks `CLAUDE_STANDUP_RENDER` /
/// `AUTOSTAND_RENDER`; if set, the hook skips to avoid re-triggering.
pub fn safe_to_fire() -> bool {
    std::env::var_os("AUTOSTAND_RENDER").is_none()
        && std::env::var_os("CLAUDE_STANDUP_RENDER").is_none()
}
