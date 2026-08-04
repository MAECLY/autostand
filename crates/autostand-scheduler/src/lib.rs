//! `autostand-scheduler` — cron, triggers, locks, self-heal.
//!
//! The primitives an in-process scheduler needs, each usable on its own:
//!
//! - [`cron`] — 5-field POSIX-subset parsing and `next_run` (UTC, bounded search).
//! - [`install`] — the **real** OS scheduler: `launchd` / `systemd --user` /
//!   Task Scheduler unit text, detection, install and uninstall.
//! - [`lock`] — the single-run lock: `mkdir` + PID + 10-minute stale reclaim,
//!   handed out as an RAII [`lock::LockGuard`].
//! - [`selfheal`] — `(F_TODAY, F_PREV)` target computation and the freeze check
//!   that keeps a populated AUTO block from being recompiled.
//! - [`triggers`] — what caused a run, and the anti-recursion env guard.
//!
//! [`cron`] and [`install`] are two halves of one story: the former is what the
//! in-process tick evaluates while the app is open, the latter translates the
//! same expression into a unit the OS runs when it is closed.
//!
//! See `docs/specs/pipeline.md`, `docs/architecture/04-state-machine.md` and
//! `docs/tauri/03-platform-targets.md`.

#![forbid(unsafe_code)]

pub mod cron;
pub mod install;
pub mod lock;
pub mod selfheal;
pub mod triggers;
