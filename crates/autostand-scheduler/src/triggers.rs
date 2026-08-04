//! Triggers: what caused a run, and the anti-recursion guard.
//!
//! See `docs/architecture/03-data-flow.md` and `docs/tauri/02-ipc-contracts.md`.

use serde::{Deserialize, Serialize};

/// What triggered a run.
///
/// Serializes `kebab-case` (`"session-end"`, `"self-heal"`, …) to match the rest
/// of the wire vocabulary; [`TriggerSource::label`] returns exactly that string.
/// The IPC surface is coarser than this enum — see [`TriggerSource::wire_label`].
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum TriggerSource {
    /// Hourly system scheduler (launchd/systemd/Task Scheduler) or the in-process
    /// cron tick.
    Scheduler,
    /// AI coding CLI session ended (`Claude Code` / `OpenCode` / `Codex` etc.).
    SessionEnd,
    /// User clicked "Run now".
    Manual,
    /// App launched.
    AppStart,
    /// Backfill of a missed run for `F_PREV` (`docs/specs/pipeline.md` step 4).
    SelfHeal,
}

impl TriggerSource {
    /// Full-fidelity label; identical to this enum's serde representation.
    ///
    /// Used for logs, the run log filename, and the audit sidecar, where the
    /// distinction between a cron tick and a session-end hook still matters.
    pub fn label(self) -> &'static str {
        match self {
            Self::Scheduler => "scheduler",
            Self::SessionEnd => "session-end",
            Self::Manual => "manual",
            Self::AppStart => "app-start",
            Self::SelfHeal => "self-heal",
        }
    }

    /// The IPC-facing value: `"scheduled" | "manual" | "self-heal"`.
    ///
    /// The frontend contract (`PipelineStatus.last_trigger` and the
    /// `pipeline-started` event, `docs/tauri/02-ipc-contracts.md`) exposes only
    /// three buckets, so every machine-initiated source collapses into
    /// `"scheduled"`: from the UI's point of view "manual" means *the user
    /// pressed Run now*, and a session-end or app-start run is as automatic as a
    /// cron tick. These strings match `commands::types::LastTrigger`'s wire
    /// format exactly — never widen this mapping without changing that DTO.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Scheduler | Self::SessionEnd | Self::AppStart => "scheduled",
            Self::Manual => "manual",
            Self::SelfHeal => "self-heal",
        }
    }

    /// Did a human ask for this run?
    ///
    /// Manual runs bypass the "nothing changed" skip so that pressing Run now
    /// always produces visible feedback.
    pub fn is_manual(self) -> bool {
        matches!(self, Self::Manual)
    }
}

/// Is it safe to fire a run right now? `false` means: skip silently.
///
/// WHY: an LLM adapter renders by spawning an AI coding CLI, and those CLIs emit
/// the very session-end event that triggers `autostand`. Without a guard, one
/// render spawns a CLI, whose session end triggers another render, forever. The
/// adapters therefore set `AUTOSTAND_RENDER=1` (and `CLAUDE_STANDUP_RENDER=1`, for
/// backward compat with the original App Script's `SessionEnd` hook) on every CLI
/// subprocess; seeing either variable means "you are already inside a render".
///
/// Checked by session-end and scheduler triggers alike, since a scheduled tick can
/// also land inside a render subprocess. A manual run from the UI is not spawned
/// from a render and is unaffected.
pub fn safe_to_fire() -> bool {
    std::env::var_os("AUTOSTAND_RENDER").is_none()
        && std::env::var_os("CLAUDE_STANDUP_RENDER").is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wire_labels_match_the_ipc_contract() {
        assert_eq!(TriggerSource::Scheduler.wire_label(), "scheduled");
        assert_eq!(TriggerSource::SessionEnd.wire_label(), "scheduled");
        assert_eq!(TriggerSource::AppStart.wire_label(), "scheduled");
        assert_eq!(TriggerSource::Manual.wire_label(), "manual");
        assert_eq!(TriggerSource::SelfHeal.wire_label(), "self-heal");
    }

    #[test]
    fn wire_labels_are_only_the_three_contract_values() {
        for source in [
            TriggerSource::Scheduler,
            TriggerSource::SessionEnd,
            TriggerSource::Manual,
            TriggerSource::AppStart,
            TriggerSource::SelfHeal,
        ] {
            assert!(
                matches!(source.wire_label(), "scheduled" | "manual" | "self-heal"),
                "{source:?} produced an off-contract wire value"
            );
        }
    }

    #[test]
    fn label_matches_serde_representation() {
        for source in [
            TriggerSource::Scheduler,
            TriggerSource::SessionEnd,
            TriggerSource::Manual,
            TriggerSource::AppStart,
            TriggerSource::SelfHeal,
        ] {
            let json = serde_json::to_string(&source).expect("serialize");
            assert_eq!(json, format!("\"{}\"", source.label()));
            let back: TriggerSource = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, source);
        }
    }

    #[test]
    fn only_manual_is_human_initiated() {
        assert!(TriggerSource::Manual.is_manual());
        assert!(!TriggerSource::Scheduler.is_manual());
        assert!(!TriggerSource::SelfHeal.is_manual());
    }
}
