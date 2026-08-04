//! `autostand-scheduler` — cron, triggers, locks, self-heal.
//!
//! The primitives an in-process scheduler needs, each usable on its own:
//!
//! - [`cron`] — 5-field POSIX-subset parsing and `next_run` (UTC, bounded search).
//! - [`lock`] — the single-run lock: `mkdir` + PID + 10-minute stale reclaim,
//!   handed out as an RAII [`lock::LockGuard`].
//! - [`selfheal`] — `(F_TODAY, F_PREV)` target computation and the freeze check
//!   that keeps a populated AUTO block from being recompiled.
//! - [`triggers`] — what caused a run, and the anti-recursion env guard.
//!
//! See `docs/specs/pipeline.md`, `docs/architecture/04-state-machine.md` and
//! `docs/tauri/03-platform-targets.md`.

#![forbid(unsafe_code)]

pub mod cron;
pub mod lock;
pub mod selfheal;
pub mod triggers;
