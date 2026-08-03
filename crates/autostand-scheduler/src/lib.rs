//! `autostand-scheduler` — cron, triggers, locks, self-heal.
//!
//! See `docs/architecture/04-state-machine.md` and `docs/tauri/03-platform-targets.md`.

#![forbid(unsafe_code)]

pub mod cron;
pub mod lock;
pub mod selfheal;
pub mod triggers;
