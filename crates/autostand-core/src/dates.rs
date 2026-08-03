//! Business-day arithmetic.
//!
//! Invariants (from App Script `lib.sh`):
//! - `next_business_day(Fri) = Mon` (+3)
//! - `next_business_day(Sat) = Mon` (+2)
//! - `next_business_day(Sun) = Mon` (+1)
//! - otherwise +1
//! - `prev_business_day_before(F) = latest weekday strictly before F`

use chrono::{Datelike, Duration, NaiveDate, Weekday};

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
}
