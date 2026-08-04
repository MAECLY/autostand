//! Self-heal: recompile the current filing date plus the previous one.
//!
//! See `docs/specs/pipeline.md` steps 2 and 4 and `docs/specs/anti-backdating.md`
//! § Freeze.

use chrono::NaiveDate;

/// Compute the two targets for a run: `(F_TODAY, F_PREV)`.
///
/// `F_TODAY` is the filing date for the work done today — work on day `D` is
/// reported in `next_business_day(D)`. `F_PREV` is the *previous target file*:
/// the last business day strictly before `F_TODAY`, i.e. the file the previous
/// run wrote. `F_PREV == F_TODAY` never happens for a business-day `F_TODAY`, so
/// step 4 always has a distinct file to check for self-heal.
///
/// Examples (spec §2): Monday → `(Tue, Mon)`; Friday → `(Mon, Fri)`;
/// Saturday and Sunday both → `(Mon, Fri)`.
pub fn compute_targets(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let f_today = autostand_core::dates::next_business_day(today);
    let f_prev = autostand_core::dates::prev_business_day_before(f_today);
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
/// day still being written is never frozen. With the `compute_targets` above the
/// two are always distinct, so in practice this is a straight check.
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
    fn targets_on_a_midweek_day() {
        // Mon 2026-08-03 → file today's work on Tue, previous target is Mon.
        assert_eq!(
            compute_targets(d(2026, 8, 3)),
            (d(2026, 8, 4), d(2026, 8, 3))
        );
        // Wed → (Thu, Wed).
        assert_eq!(
            compute_targets(d(2026, 8, 5)),
            (d(2026, 8, 6), d(2026, 8, 5))
        );
    }

    #[test]
    fn targets_roll_over_the_weekend() {
        // Fri 2026-08-07 → (Mon 08-10, Fri 08-07).
        assert_eq!(
            compute_targets(d(2026, 8, 7)),
            (d(2026, 8, 10), d(2026, 8, 7))
        );
        // Sat and Sun both file on Monday, with Friday as the previous target.
        assert_eq!(
            compute_targets(d(2026, 8, 8)),
            (d(2026, 8, 10), d(2026, 8, 7))
        );
        assert_eq!(
            compute_targets(d(2026, 8, 9)),
            (d(2026, 8, 10), d(2026, 8, 7))
        );
    }

    #[test]
    fn targets_are_always_distinct_business_days() {
        let mut day = d(2026, 1, 1);
        let end = d(2027, 1, 1);
        while day < end {
            let (f_today, f_prev) = compute_targets(day);
            assert!(f_prev < f_today, "{day}: {f_prev} must precede {f_today}");
            day = day.succ_opt().expect("next day");
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
