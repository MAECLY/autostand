//! Self-heal: recompile the current filing date plus the previous one.
//!
//! See `docs/specs/pipeline.md` steps 2 and 4 and `docs/specs/anti-backdating.md`
//! § Freeze.

use autostand_core::dates::{prev_business_day_before, ArchiveMode};
use chrono::NaiveDate;

/// Compute the two targets for a run: `(F_TODAY, F_PREV)`.
///
/// `F_TODAY` is the file today's work belongs to. `F_PREV` is the file the
/// *previous run day* wrote — that is, the filing date of the last business day
/// strictly before today. Both go through [`ArchiveMode::filing_date`], so the
/// pair follows whichever filing policy is configured.
///
/// The two can be equal, and that is meaningful rather than a degenerate case:
/// in `NextBusinessDay` mode a Saturday and a Sunday both file into Monday, and
/// so does the preceding Friday — there is no *previous* file that this run
/// could still be filling, so `compile.sh:544` skips self-heal outright. The
/// caller compares them and drops the second target when they match.
///
/// Examples, `NextBusinessDay` (spec §2): Monday → `(Tue, Mon)`;
/// Friday → `(Mon, Fri)`; Saturday and Sunday both → `(Mon, Mon)` (no self-heal).
/// `SameDay`: Monday → `(Mon, Fri)`; Saturday and Sunday → `(Mon, Fri)`.
pub fn compute_targets(today: NaiveDate, mode: ArchiveMode) -> (NaiveDate, NaiveDate) {
    let f_today = mode.filing_date(today);
    // WHY not `prev_business_day_before(f_today)`: that walks back from the
    // *file*, which on a weekend lands on Friday and re-opens a day the Friday
    // run already closed. The App Script walks back from *today* and then files
    // that day, which is the only formula that names the file the previous run
    // actually wrote.
    let f_prev = mode.filing_date(prev_business_day_before(today));
    (f_today, f_prev)
}

/// Is `F_PREV`'s AUTO block already populated, i.e. **frozen**?
///
/// `existing_auto_body` is the AUTO block body for *this host* parsed out of the
/// existing `dailies/<F_PREV>.md`, or `None` when the file — or this host's block
/// inside it — does not exist. A body that is only whitespace counts as empty:
/// the renderer writes an empty block into the skeleton before it has content.
///
/// WHY never recompile a filled block: `accumulate` is a never-delete merge, but
/// it can only re-inject what the *new* render plus the previous body contain.
/// Self-heal runs from durable disk data only (git log + notes files still on
/// disk), which is a strictly narrower input set than the original run had — its
/// enrichment sources are volatile. Recompiling a populated day would therefore
/// risk rewriting a complete standup from partial facts, and would churn the
/// dailies repo with a commit per run. Once the block has content, the day is final.
///
/// Callers must only apply this to a rolled-over day (`F_PREV != F_TODAY`); the
/// day still being written is never frozen. [`compute_targets`] can return the
/// same date twice — a weekend run in `NextBusinessDay` mode does — which is the
/// caller's signal to skip self-heal entirely rather than to freeze-check the
/// file it is currently filling.
pub fn is_frozen(existing_auto_body: Option<&str>) -> bool {
    existing_auto_body.is_some_and(|body| !body.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a date, panicking on an invalid one (test-only convenience).
    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    #[test]
    fn next_business_day_targets_for_every_weekday() {
        // (today, F_TODAY, F_PREV). Sat and Sun collapse onto Monday for both,
        // which is the App Script's "no self-heal at the weekend" rule.
        let table = [
            (d(2026, 8, 3), d(2026, 8, 4), d(2026, 8, 3)), // Mon → (Tue, Mon)
            (d(2026, 8, 4), d(2026, 8, 5), d(2026, 8, 4)), // Tue → (Wed, Tue)
            (d(2026, 8, 5), d(2026, 8, 6), d(2026, 8, 5)), // Wed → (Thu, Wed)
            (d(2026, 8, 6), d(2026, 8, 7), d(2026, 8, 6)), // Thu → (Fri, Thu)
            (d(2026, 8, 7), d(2026, 8, 10), d(2026, 8, 7)), // Fri → (Mon, Fri)
            (d(2026, 8, 8), d(2026, 8, 10), d(2026, 8, 10)), // Sat → (Mon, Mon)
            (d(2026, 8, 9), d(2026, 8, 10), d(2026, 8, 10)), // Sun → (Mon, Mon)
        ];
        for (today, f_today, f_prev) in table {
            assert_eq!(
                compute_targets(today, ArchiveMode::NextBusinessDay),
                (f_today, f_prev),
                "{today}"
            );
        }
    }

    #[test]
    fn same_day_targets_for_every_weekday() {
        let table = [
            (d(2026, 8, 3), d(2026, 8, 3), d(2026, 7, 31)), // Mon → (Mon, Fri)
            (d(2026, 8, 4), d(2026, 8, 4), d(2026, 8, 3)),  // Tue → (Tue, Mon)
            (d(2026, 8, 5), d(2026, 8, 5), d(2026, 8, 4)),  // Wed → (Wed, Tue)
            (d(2026, 8, 6), d(2026, 8, 6), d(2026, 8, 5)),  // Thu → (Thu, Wed)
            (d(2026, 8, 7), d(2026, 8, 7), d(2026, 8, 6)),  // Fri → (Fri, Thu)
            (d(2026, 8, 8), d(2026, 8, 10), d(2026, 8, 7)), // Sat → (Mon, Fri)
            (d(2026, 8, 9), d(2026, 8, 10), d(2026, 8, 7)), // Sun → (Mon, Fri)
        ];
        for (today, f_today, f_prev) in table {
            assert_eq!(
                compute_targets(today, ArchiveMode::SameDay),
                (f_today, f_prev),
                "{today}"
            );
        }
    }

    #[test]
    fn f_prev_never_runs_ahead_of_f_today() {
        // Equal is legal (weekend, next-business-day mode) and means "no
        // self-heal"; *greater* would make a run reopen a future file.
        for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
            let mut day = d(2026, 1, 1);
            let end = d(2027, 1, 1);
            while day < end {
                let (f_today, f_prev) = compute_targets(day, mode);
                assert!(
                    f_prev <= f_today,
                    "{mode:?} {day}: {f_prev} must not follow {f_today}"
                );
                day = day.succ_opt().expect("next day");
            }
        }
    }

    #[test]
    fn f_prev_is_the_file_the_previous_run_day_wrote() {
        // The property the formula exists for: whatever `compute_targets` names
        // as F_PREV today is exactly what it named as F_TODAY on the previous
        // business day.
        for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
            let mut day = d(2026, 1, 5);
            let end = d(2027, 1, 1);
            while day < end {
                let (_, f_prev) = compute_targets(day, mode);
                let previous_run = prev_business_day_before(day);
                let (previous_f_today, _) = compute_targets(previous_run, mode);
                assert_eq!(f_prev, previous_f_today, "{mode:?} {day}");
                day = day.succ_opt().expect("next day");
            }
        }
    }

    #[test]
    fn frozen_only_when_the_block_has_content() {
        assert!(!is_frozen(None), "missing file is not frozen");
        assert!(!is_frozen(Some("")), "empty block is not frozen");
        assert!(
            !is_frozen(Some("\n  \n\t")),
            "whitespace block is not frozen"
        );
        assert!(is_frozen(Some("- Shipped the parser")));
        assert!(is_frozen(Some("\n- Shipped the parser\n")));
    }
}
