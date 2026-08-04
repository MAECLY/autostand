//! Business-day arithmetic.
//!
//! Invariants (from App Script `lib.sh`):
//! - `next_business_day(Fri) = Mon` (+3)
//! - `next_business_day(Sat) = Mon` (+2)
//! - `next_business_day(Sun) = Mon` (+1)
//! - otherwise +1
//! - `prev_business_day_before(F) = latest weekday strictly before F`
//!
//! Also hosts the gather window (step (a) of `docs/specs/pipeline.md`): the
//! two-business-day range ending the day before the filing date `F`.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Compute the next business day after `date`.
/// Work done on day `D` is reported in `next_business_day(D)`.
pub fn next_business_day(date: NaiveDate) -> NaiveDate {
    match date.weekday() {
        Weekday::Fri => date + Duration::days(3),
        Weekday::Sat => date + Duration::days(2),
        // Sun and Mon–Thu all advance by 1 day.
        _ => date + Duration::days(1),
    }
}

/// Compute the latest weekday strictly before `date`.
/// Used to find `range_start` for a filing date `F`.
pub fn prev_business_day_before(date: NaiveDate) -> NaiveDate {
    let mut candidate = date - Duration::days(1);
    while candidate.weekday() == Weekday::Sat || candidate.weekday() == Weekday::Sun {
        candidate -= Duration::days(1);
    }
    candidate
}

/// The gather window for a filing date `F`: the two-business-day range that ends
/// on the last business day before `F`.
///
/// `dates` is the *expanded* list of business days inside
/// `[range_start, range_end]` — it is what note gathering iterates over, while
/// `range_start`/`range_end` are what `git log --since/--until` is bounded by.
/// Serializable because it is part of the enrichment cache key and of the audit
/// sidecar's `window` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    /// First day of the window (inclusive). Always a weekday.
    pub range_start: NaiveDate,
    /// Last day of the window (inclusive). Always a weekday.
    pub range_end: NaiveDate,
    /// Every business day in `[range_start, range_end]`, ascending.
    pub dates: Vec<NaiveDate>,
}

/// Compute the gather window for filing date `f` — step (a) of the pipeline.
///
/// `range_end` is the last business day strictly before `f`; `range_start` is the
/// business day before that. So a Monday `F` yields the prior Thursday–Friday.
/// Holidays are not modelled (matches the App Script).
pub fn compute_window(f: NaiveDate) -> Window {
    let range_end = prev_business_day_before(f);
    let range_start = prev_business_day_before(range_end);
    Window {
        range_start,
        range_end,
        dates: business_days_between(range_start, range_end),
    }
}

/// List every business day (Mon–Fri) in the inclusive range `[start, end]`.
///
/// Returns an empty vector when `start > end`, so callers never have to guard the
/// inverted-range case.
pub fn business_days_between(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut days = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        if !matches!(cursor.weekday(), Weekday::Sat | Weekday::Sun) {
            days.push(cursor);
        }
        cursor += Duration::days(1);
    }
    days
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn friday_skips_weekend() {
        let fri = NaiveDate::from_ymd_opt(2026, 8, 7).unwrap(); // Fri
        assert_eq!(next_business_day(fri).weekday(), Weekday::Mon);
        assert_eq!(
            next_business_day(fri),
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap()
        );
    }

    #[test]
    fn saturday_to_monday() {
        let sat = NaiveDate::from_ymd_opt(2026, 8, 8).unwrap();
        assert_eq!(
            next_business_day(sat),
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap()
        );
    }

    #[test]
    fn sunday_to_monday() {
        let sun = NaiveDate::from_ymd_opt(2026, 8, 9).unwrap();
        assert_eq!(
            next_business_day(sun),
            NaiveDate::from_ymd_opt(2026, 8, 10).unwrap()
        );
    }

    #[test]
    fn monday_to_tuesday() {
        let mon = NaiveDate::from_ymd_opt(2026, 8, 10).unwrap();
        assert_eq!(
            next_business_day(mon),
            NaiveDate::from_ymd_opt(2026, 8, 11).unwrap()
        );
    }

    #[test]
    fn prev_before_monday_is_friday() {
        let mon = NaiveDate::from_ymd_opt(2026, 8, 10).unwrap();
        assert_eq!(
            prev_business_day_before(mon),
            NaiveDate::from_ymd_opt(2026, 8, 7).unwrap()
        );
    }

    #[test]
    fn prev_before_sunday_is_friday() {
        let sun = NaiveDate::from_ymd_opt(2026, 8, 9).unwrap();
        assert_eq!(
            prev_business_day_before(sun),
            NaiveDate::from_ymd_opt(2026, 8, 7).unwrap()
        );
    }

    fn d(y: i32, m: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, day).expect("valid date")
    }

    #[test]
    fn window_for_monday_is_prior_thursday_friday() {
        // 2026-08-10 is a Monday.
        let w = compute_window(d(2026, 8, 10));
        assert_eq!(w.range_start, d(2026, 8, 6)); // Thu
        assert_eq!(w.range_end, d(2026, 8, 7)); // Fri
        assert_eq!(w.dates, vec![d(2026, 8, 6), d(2026, 8, 7)]);
    }

    #[test]
    fn window_for_tuesday_spans_the_weekend() {
        // 2026-08-11 is a Tuesday: range is Fri..Mon, but Sat/Sun are not dates.
        let w = compute_window(d(2026, 8, 11));
        assert_eq!(w.range_start, d(2026, 8, 7)); // Fri
        assert_eq!(w.range_end, d(2026, 8, 10)); // Mon
        assert_eq!(w.dates, vec![d(2026, 8, 7), d(2026, 8, 10)]);
    }

    #[test]
    fn window_for_weekend_filing_date_falls_back_to_the_week() {
        // Saturday 2026-08-08 → Thu..Fri, same as the following Monday.
        let w = compute_window(d(2026, 8, 8));
        assert_eq!(w.range_start, d(2026, 8, 6));
        assert_eq!(w.range_end, d(2026, 8, 7));
    }

    #[test]
    fn window_crosses_the_month_boundary() {
        // Monday 2026-08-03 → Thu 2026-07-30 .. Fri 2026-07-31.
        let w = compute_window(d(2026, 8, 3));
        assert_eq!(w.range_start, d(2026, 7, 30));
        assert_eq!(w.range_end, d(2026, 7, 31));
        assert_eq!(w.dates, vec![d(2026, 7, 30), d(2026, 7, 31)]);
    }

    #[test]
    fn window_dates_are_always_weekdays() {
        let mut day = d(2026, 8, 1);
        for _ in 0..40 {
            for date in compute_window(day).dates {
                assert!(!matches!(date.weekday(), Weekday::Sat | Weekday::Sun));
            }
            day += Duration::days(1);
        }
    }

    #[test]
    fn business_days_between_skips_weekend() {
        let days = business_days_between(d(2026, 8, 6), d(2026, 8, 11));
        assert_eq!(
            days,
            vec![d(2026, 8, 6), d(2026, 8, 7), d(2026, 8, 10), d(2026, 8, 11)]
        );
    }

    #[test]
    fn business_days_between_single_day_is_inclusive() {
        assert_eq!(
            business_days_between(d(2026, 8, 6), d(2026, 8, 6)),
            vec![d(2026, 8, 6)]
        );
    }

    #[test]
    fn business_days_between_weekend_only_is_empty() {
        assert!(business_days_between(d(2026, 8, 8), d(2026, 8, 9)).is_empty());
    }

    #[test]
    fn business_days_between_inverted_range_is_empty() {
        assert!(business_days_between(d(2026, 8, 10), d(2026, 8, 6)).is_empty());
    }

    #[test]
    fn window_serde_roundtrip() {
        let w = compute_window(d(2026, 8, 10));
        let json = serde_json::to_string(&w).expect("serialize");
        let back: Window = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(w, back);
    }
}
