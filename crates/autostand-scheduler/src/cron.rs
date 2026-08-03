//! Cron schedule parsing + next-run computation (stub).
//! Default: `0 7-19 * * 1-5` (hourly 07-19 weekdays).

use chrono::{DateTime, Utc};

/// Parse a cron expression and compute the next run after `from`.
/// TODO: use a cron crate (e.g. `croner`) in full impl.
pub fn next_run(_cron: &str, from: DateTime<Utc>) -> DateTime<Utc> {
    // Stub: return `from + 1 hour`
    from + chrono::Duration::hours(1)
}
