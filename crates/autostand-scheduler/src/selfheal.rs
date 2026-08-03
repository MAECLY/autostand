//! Self-heal: recompile today + previous business day.
//! See `docs/specs/pipeline.md` step 4-5.

use chrono::NaiveDate;

/// Compute the two targets for a run: (`F_TODAY`, `F_PREV`).
pub fn compute_targets(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let f_today = autostand_core::dates::next_business_day(today);
    let prev = autostand_core::dates::prev_business_day_before(f_today);
    let f_prev = autostand_core::dates::next_business_day(prev);
    (f_today, f_prev)
}
