//! Business-day arithmetic and the gather window.
//!
//! Invariants (from App Script `scripts/lib.sh` + `scripts/compile.sh`):
//! - `next_business_day(Fri) = Mon` (+3)
//! - `next_business_day(Sat) = Mon` (+2)
//! - `next_business_day(Sun) = Mon` (+1)
//! - otherwise +1
//! - `prev_business_day_before(F) = latest weekday strictly before F`
//!
//! # The window contract
//!
//! Whichever [`ArchiveMode`] is in force, `range_start` is **the day after the
//! last natural day the previous standup file covered**. That single rule is
//! what makes the sequence of standups a partition of the calendar: no natural
//! day is claimed twice, and none is dropped. Weekend work therefore always
//! lands in *some* file — the mode only decides which one.
//!
//! That is also why [`Window::dates`] walks **natural** days rather than
//! business days: `compile.sh:192-193` builds its `dates` list by stepping one
//! calendar day at a time, so a Saturday note file and a Sunday commit are read
//! into Monday's standup. Filtering weekends out here silently dropped every
//! weekend note autostand ever gathered.

use chrono::{Datelike, Duration, NaiveDate, Weekday};
use serde::{Deserialize, Serialize};

/// Which calendar day a compile files its standup under.
///
/// The two variants differ only in the offset between the day work happened and
/// the name of the file it is written to; both preserve the window contract
/// documented at the module level.
///
/// Wire values are `snake_case` (`"next_business_day" | "same_day"`), matching
/// the `AppConfig.dates.archive_mode` field of the IPC contract.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveMode {
    /// App Script behaviour, and the default: work done on `D` is reported in
    /// the standup named after `next_business_day(D)`.
    #[default]
    NextBusinessDay,
    /// Work done on `D` is reported in `D`'s own standup file.
    SameDay,
}

impl ArchiveMode {
    /// The standup file that work done on `day` belongs to.
    ///
    /// `SameDay` still rolls a weekend forward: there is no standup named after
    /// a Saturday, so weekend work is filed on the following Monday. That is
    /// not a special case bolted on — it is what keeps `SameDay` windows
    /// contiguous across a weekend, because Monday's window then starts on the
    /// Saturday.
    pub fn filing_date(self, day: NaiveDate) -> NaiveDate {
        match self {
            Self::NextBusinessDay => next_business_day(day),
            Self::SameDay if is_weekend(day) => next_business_day(day),
            Self::SameDay => day,
        }
    }

    /// The wire string this mode serializes to.
    ///
    /// Exists so the Terminal panel and the audit sidecar can name the policy a
    /// run used without going through `serde_json` for one word. A test pins it
    /// against the serialized form, so the two cannot drift.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::NextBusinessDay => "next_business_day",
            Self::SameDay => "same_day",
        }
    }
}

/// Is `date` a Saturday or a Sunday?
pub fn is_weekend(date: NaiveDate) -> bool {
    matches!(date.weekday(), Weekday::Sat | Weekday::Sun)
}

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

/// The gather window for a filing date `F`.
///
/// `dates` is the *expanded* list of natural days inside
/// `[range_start, range_end]` — it is what note gathering iterates over, while
/// `range_start`/`range_end` are what `git log --since/--until` is bounded by.
/// Serializable because it is part of the enrichment cache key and of the audit
/// sidecar's `window` field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Window {
    /// First day of the window (inclusive).
    pub range_start: NaiveDate,
    /// Last day of the window (inclusive). May be a Saturday or a Sunday: the
    /// App Script bounds `git log --until` on it, which is exactly how weekend
    /// commits reach Monday's file.
    pub range_end: NaiveDate,
    /// Every natural day in `[range_start, range_end]`, ascending — weekends
    /// included.
    pub dates: Vec<NaiveDate>,
}

impl Window {
    /// Does this window cover no day at all?
    ///
    /// Happens when the filing date is far enough in the future that clamping
    /// `range_end` to today inverts the range. `compile.sh:181` returns early on
    /// exactly this condition ("F is beyond the work we have").
    pub fn is_empty(&self) -> bool {
        self.range_end < self.range_start
    }
}

/// Compute the gather window for filing date `f` — step (a) of the pipeline.
///
/// `today` is the real calendar day the run fires on, and it is not decoration:
/// `range_end` is clamped to it (`compile.sh:179`, `earlier "$dayb" "$TODAY"`)
/// so a compile never claims work from a day that has not happened yet. Ignoring
/// it made a same-day filing date report a full day of imaginary work.
///
/// Holidays are not modelled (matches the App Script).
pub fn compute_window(f: NaiveDate, mode: ArchiveMode, today: NaiveDate) -> Window {
    // Both arms restate the module-level contract: `range_start` is the day
    // after the last day the previous file (`prev_business_day_before(f)`)
    // covered, and `range_end` is the last day this file may claim.
    let (range_start, last_claimable) = match mode {
        // The previous file ended on `prev_business_day_before(f) - 1`, so this
        // one starts on `prev_business_day_before(f)` itself.
        ArchiveMode::NextBusinessDay => (prev_business_day_before(f), f - Duration::days(1)),
        // The previous file ended on `prev_business_day_before(f)`, so this one
        // starts the day after it — and claims `f` itself.
        ArchiveMode::SameDay => (prev_business_day_before(f) + Duration::days(1), f),
    };
    let range_end = last_claimable.min(today);
    Window {
        range_start,
        range_end,
        dates: natural_days_between(range_start, range_end),
    }
}

/// List every natural day in the inclusive range `[start, end]`, weekends
/// included.
///
/// Returns an empty vector when `start > end`, so callers never have to guard
/// the inverted-range case.
pub fn natural_days_between(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    let mut days = Vec::new();
    let mut cursor = start;
    while cursor <= end {
        days.push(cursor);
        cursor += Duration::days(1);
    }
    days
}

/// List every business day (Mon–Fri) in the inclusive range `[start, end]`.
///
/// Returns an empty vector when `start > end`, so callers never have to guard the
/// inverted-range case. Deliberately *not* what [`Window::dates`] is built from —
/// see the module docs.
pub fn business_days_between(start: NaiveDate, end: NaiveDate) -> Vec<NaiveDate> {
    natural_days_between(start, end)
        .into_iter()
        .filter(|day| !is_weekend(*day))
        .collect()
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

    /// A `today` far enough in the future that the `min(_, today)` clamp never
    /// bites — used by every test that is about the *shape* of the window.
    fn never_clamps() -> NaiveDate {
        d(2030, 1, 1)
    }

    // ── filing date ───────────────────────────────────────────────────────

    #[test]
    fn next_business_day_mode_files_tomorrow_and_skips_the_weekend() {
        // Mon…Thu → the next day; Fri/Sat/Sun → Monday.
        let table = [
            (d(2026, 8, 3), d(2026, 8, 4)),  // Mon → Tue
            (d(2026, 8, 4), d(2026, 8, 5)),  // Tue → Wed
            (d(2026, 8, 5), d(2026, 8, 6)),  // Wed → Thu
            (d(2026, 8, 6), d(2026, 8, 7)),  // Thu → Fri
            (d(2026, 8, 7), d(2026, 8, 10)), // Fri → Mon
            (d(2026, 8, 8), d(2026, 8, 10)), // Sat → Mon
            (d(2026, 8, 9), d(2026, 8, 10)), // Sun → Mon
        ];
        for (day, expected) in table {
            assert_eq!(
                ArchiveMode::NextBusinessDay.filing_date(day),
                expected,
                "{day}"
            );
        }
    }

    #[test]
    fn same_day_mode_files_today_but_rolls_the_weekend_forward() {
        let table = [
            (d(2026, 8, 3), d(2026, 8, 3)),  // Mon → Mon
            (d(2026, 8, 4), d(2026, 8, 4)),  // Tue → Tue
            (d(2026, 8, 5), d(2026, 8, 5)),  // Wed → Wed
            (d(2026, 8, 6), d(2026, 8, 6)),  // Thu → Thu
            (d(2026, 8, 7), d(2026, 8, 7)),  // Fri → Fri
            (d(2026, 8, 8), d(2026, 8, 10)), // Sat → Mon
            (d(2026, 8, 9), d(2026, 8, 10)), // Sun → Mon
        ];
        for (day, expected) in table {
            assert_eq!(ArchiveMode::SameDay.filing_date(day), expected, "{day}");
        }
    }

    #[test]
    fn archive_mode_defaults_to_the_app_script_behaviour() {
        assert_eq!(ArchiveMode::default(), ArchiveMode::NextBusinessDay);
    }

    #[test]
    fn wire_label_matches_the_serialized_form() {
        // The log/sidecar label and the JSON value are the same vocabulary; a
        // second spelling would make a Terminal line unsearchable against the
        // sidecar it describes.
        for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
            let json = serde_json::to_string(&mode).expect("serialize");
            assert_eq!(json, format!("\"{}\"", mode.wire_label()));
        }
    }

    #[test]
    fn archive_mode_wire_values_are_snake_case() {
        for (mode, wire) in [
            (ArchiveMode::NextBusinessDay, "\"next_business_day\""),
            (ArchiveMode::SameDay, "\"same_day\""),
        ] {
            let json = serde_json::to_string(&mode).expect("serialize");
            assert_eq!(json, wire);
            let back: ArchiveMode = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, mode);
        }
    }

    // ── window shape, both modes, every weekday ───────────────────────────

    #[test]
    fn next_business_day_windows_for_a_whole_week() {
        // (filing date, range_start, range_end). Mon's window is the one that
        // swallows the weekend: Fri, Sat and Sun all belong to it.
        let table = [
            (d(2026, 8, 10), d(2026, 8, 7), d(2026, 8, 9)), // Mon ← Fri..Sun
            (d(2026, 8, 11), d(2026, 8, 10), d(2026, 8, 10)), // Tue ← Mon
            (d(2026, 8, 12), d(2026, 8, 11), d(2026, 8, 11)), // Wed ← Tue
            (d(2026, 8, 13), d(2026, 8, 12), d(2026, 8, 12)), // Thu ← Wed
            (d(2026, 8, 14), d(2026, 8, 13), d(2026, 8, 13)), // Fri ← Thu
        ];
        for (f, start, end) in table {
            let w = compute_window(f, ArchiveMode::NextBusinessDay, never_clamps());
            assert_eq!(w.range_start, start, "{f} start");
            assert_eq!(w.range_end, end, "{f} end");
        }
    }

    #[test]
    fn same_day_windows_for_a_whole_week() {
        let table = [
            (d(2026, 8, 10), d(2026, 8, 8), d(2026, 8, 10)), // Mon ← Sat..Mon
            (d(2026, 8, 11), d(2026, 8, 11), d(2026, 8, 11)), // Tue ← Tue
            (d(2026, 8, 12), d(2026, 8, 12), d(2026, 8, 12)), // Wed ← Wed
            (d(2026, 8, 13), d(2026, 8, 13), d(2026, 8, 13)), // Thu ← Thu
            (d(2026, 8, 14), d(2026, 8, 14), d(2026, 8, 14)), // Fri ← Fri
        ];
        for (f, start, end) in table {
            let w = compute_window(f, ArchiveMode::SameDay, never_clamps());
            assert_eq!(w.range_start, start, "{f} start");
            assert_eq!(w.range_end, end, "{f} end");
        }
    }

    #[test]
    fn monday_absorbs_the_whole_weekend_in_both_modes() {
        // The bug this replaces dropped Sat+Sun on the floor entirely.
        let (sat, sun) = (d(2026, 8, 8), d(2026, 8, 9));
        let next = compute_window(d(2026, 8, 10), ArchiveMode::NextBusinessDay, never_clamps());
        assert!(
            next.dates.contains(&sat) && next.dates.contains(&sun),
            "{next:?}"
        );
        let same = compute_window(d(2026, 8, 10), ArchiveMode::SameDay, never_clamps());
        assert!(
            same.dates.contains(&sat) && same.dates.contains(&sun),
            "{same:?}"
        );
    }

    #[test]
    fn window_dates_walk_natural_days_not_business_days() {
        let w = compute_window(d(2026, 8, 10), ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(
            w.dates,
            vec![d(2026, 8, 7), d(2026, 8, 8), d(2026, 8, 9)],
            "Fri, Sat and Sun are all read into Monday's standup"
        );
    }

    // ── the clamp on today ────────────────────────────────────────────────

    #[test]
    fn range_end_is_clamped_to_today() {
        // Filing Monday's standup *on* Friday (the scheduled 19:00 run): only
        // Friday itself exists yet, so the window must stop there.
        let w = compute_window(d(2026, 8, 10), ArchiveMode::NextBusinessDay, d(2026, 8, 7));
        assert_eq!(w.range_start, d(2026, 8, 7));
        assert_eq!(w.range_end, d(2026, 8, 7));
        assert_eq!(w.dates, vec![d(2026, 8, 7)]);
    }

    #[test]
    fn same_day_window_never_claims_tomorrow() {
        // Same-day filing on Tuesday: the window is Tuesday, not Tuesday-plus.
        let w = compute_window(d(2026, 8, 11), ArchiveMode::SameDay, d(2026, 8, 11));
        assert_eq!(w.range_start, d(2026, 8, 11));
        assert_eq!(w.range_end, d(2026, 8, 11));
    }

    #[test]
    fn a_filing_date_beyond_today_yields_an_empty_window() {
        // Tue's file compiled on Mon in same-day mode: nothing to claim yet.
        let w = compute_window(d(2026, 8, 11), ArchiveMode::SameDay, d(2026, 8, 10));
        assert!(w.is_empty(), "{w:?}");
        assert!(w.dates.is_empty());
    }

    #[test]
    fn a_covered_window_is_not_empty() {
        let w = compute_window(d(2026, 8, 11), ArchiveMode::SameDay, d(2026, 8, 11));
        assert!(!w.is_empty());
    }

    // ── the contract: contiguous, no gaps, no overlaps ────────────────────

    /// Every filing date in `[from, to]` for `mode`, ascending and deduplicated.
    fn filing_dates(mode: ArchiveMode, from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
        let mut out: Vec<NaiveDate> = Vec::new();
        let mut day = from;
        while day <= to {
            let f = mode.filing_date(day);
            if out.last() != Some(&f) {
                out.push(f);
            }
            day += Duration::days(1);
        }
        out
    }

    #[test]
    fn consecutive_windows_tile_the_calendar_without_gaps_or_overlaps() {
        for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
            let files = filing_dates(mode, d(2026, 1, 1), d(2027, 3, 1));
            let mut previous: Option<Window> = None;
            for f in files {
                let window = compute_window(f, mode, never_clamps());
                assert!(!window.is_empty(), "{mode:?} {f}: empty window");
                if let Some(prev) = previous {
                    assert_eq!(
                        window.range_start,
                        prev.range_end + Duration::days(1),
                        "{mode:?}: {f} must start the day after {} ended",
                        prev.range_end
                    );
                }
                previous = Some(window);
            }
        }
    }

    #[test]
    fn every_natural_day_lands_in_exactly_one_window() {
        for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
            let files = filing_dates(mode, d(2026, 2, 1), d(2026, 12, 1));
            let mut seen: Vec<NaiveDate> = Vec::new();
            for f in &files {
                seen.extend(compute_window(*f, mode, never_clamps()).dates);
            }
            let mut sorted = seen.clone();
            sorted.sort_unstable();
            sorted.dedup();
            assert_eq!(
                sorted.len(),
                seen.len(),
                "{mode:?}: a day was claimed twice"
            );
            // Contiguity: no calendar hole between the first and the last day.
            let span = (sorted[sorted.len() - 1] - sorted[0]).num_days() + 1;
            assert_eq!(
                i64::try_from(sorted.len()).expect("fits"),
                span,
                "{mode:?}: a day was dropped"
            );
        }
    }

    // ── boundaries ────────────────────────────────────────────────────────

    #[test]
    fn friday_window_covers_only_thursday_in_next_business_day_mode() {
        let w = compute_window(d(2026, 8, 14), ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(w.range_start, d(2026, 8, 13));
        assert_eq!(w.range_end, d(2026, 8, 13));
    }

    #[test]
    fn a_weekend_filing_date_is_still_computable() {
        // Nothing files a standup on a Saturday, but `--date 2026-08-08` must not
        // panic or invert: Sat's window is Fri..Fri in next-business-day mode.
        let w = compute_window(d(2026, 8, 8), ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(w.range_start, d(2026, 8, 7));
        assert_eq!(w.range_end, d(2026, 8, 7));
    }

    #[test]
    fn window_crosses_the_month_boundary() {
        // Monday 2026-08-03 ← Fri 2026-07-31 .. Sun 2026-08-02.
        let w = compute_window(d(2026, 8, 3), ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(w.range_start, d(2026, 7, 31));
        assert_eq!(w.range_end, d(2026, 8, 2));
        assert_eq!(w.dates, vec![d(2026, 7, 31), d(2026, 8, 1), d(2026, 8, 2)]);
    }

    #[test]
    fn window_crosses_the_year_boundary() {
        // Mon 2027-01-04 ← Fri 2027-01-01 .. Sun 2027-01-03 (next-business-day),
        // and ← Sat 2027-01-02 .. Mon 2027-01-04 (same-day).
        let next = compute_window(d(2027, 1, 4), ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(next.range_start, d(2027, 1, 1));
        assert_eq!(next.range_end, d(2027, 1, 3));

        let same = compute_window(d(2027, 1, 4), ArchiveMode::SameDay, never_clamps());
        assert_eq!(same.range_start, d(2027, 1, 2));
        assert_eq!(same.range_end, d(2027, 1, 4));

        // Fri 2027-01-01 files the last days of 2026 in next-business-day mode.
        let newyear = compute_window(d(2027, 1, 1), ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(newyear.range_start, d(2026, 12, 31));
        assert_eq!(newyear.range_end, d(2026, 12, 31));
    }

    #[test]
    fn natural_days_between_includes_the_weekend() {
        assert_eq!(
            natural_days_between(d(2026, 8, 7), d(2026, 8, 10)),
            vec![d(2026, 8, 7), d(2026, 8, 8), d(2026, 8, 9), d(2026, 8, 10)]
        );
    }

    #[test]
    fn natural_days_between_inverted_range_is_empty() {
        assert!(natural_days_between(d(2026, 8, 10), d(2026, 8, 6)).is_empty());
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
        let w = compute_window(d(2026, 8, 10), ArchiveMode::NextBusinessDay, never_clamps());
        let json = serde_json::to_string(&w).expect("serialize");
        let back: Window = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(w, back);
    }
}
