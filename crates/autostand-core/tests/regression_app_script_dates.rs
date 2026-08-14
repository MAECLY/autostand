//! Regression: the App Script's filing-date truth table, written out in full.
//!
//! Every row below was read off `~/Sync/Github_Dailies/scripts/compile.sh` and
//! `scripts/lib.sh` — the reference implementation this app ports and must never
//! modify — and confirmed by running those functions:
//!
//! | rule | source |
//! |------|--------|
//! | `F = next_business_day(TODAY)` | `compile.sh:534` |
//! | `next_business_day`: Fri +3, Sat +2, Sun +1, otherwise +1 | `lib.sh:62-67` |
//! | window of `F` = `[prev_business_day_before(F) … min(F-1, TODAY)]` | `compile.sh:9-10`, `:177-181` |
//! | `prev_business_day_before` = last weekday strictly before | `compile.sh:53-61` |
//! | the day list walks **natural** days, weekends included | `compile.sh:192-193` |
//! | `git log --since range_start --until range_end`, `range_end` may be a weekend day | `compile.sh:202` |
//! | `--date` names a **work day**, not a file | `compile.sh:39` + `:534` |
//!
//! # Why this file exists
//!
//! Autostand shipped a window of `range_end = prev_business_day_before(F)` and
//! `range_start = prev_business_day_before(range_end)` — two *business* days,
//! ending one day early. It reproduced neither half of the rule above, and the
//! damage was invisible from the app: Saturday and Sunday reached no file at all,
//! and every midweek day was reported in two consecutive standups.
//!
//! [`legacy_window`] below is that formula, kept verbatim so the tests can state
//! what the current implementation must *not* do. Delete it only when the App
//! Script itself changes.

use chrono::{Datelike, Duration, NaiveDate, Weekday};

use autostand_core::dates::{
    compute_window, next_business_day, prev_business_day_before, ArchiveMode, Window,
};

/// Build a date, panicking on an invalid one.
fn d(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
}

/// A `today` far past every fixture, so the `min(_, today)` clamp never fires.
///
/// The clamp has its own tests; these are about the *shape* of the rule.
fn never_clamps() -> NaiveDate {
    d(2030, 1, 1)
}

/// The window autostand used to compute — **not** the App Script's.
///
/// Kept so the assertions can name the exact defect instead of only asserting
/// the correct answer: a future refactor that reintroduces it fails here with a
/// message that says which of the two failure modes came back.
fn legacy_window(f: NaiveDate) -> (NaiveDate, NaiveDate) {
    let range_end = prev_business_day_before(f);
    (prev_business_day_before(range_end), range_end)
}

// ── the table ─────────────────────────────────────────────────────────────

/// `TODAY → F` for a full reference week, both directions of the weekend.
#[test]
fn work_on_a_day_is_filed_under_the_next_business_day() {
    let table = [
        (d(2026, 8, 10), d(2026, 8, 11)), // Mon → Tue
        (d(2026, 8, 11), d(2026, 8, 12)), // Tue → Wed
        (d(2026, 8, 12), d(2026, 8, 13)), // Wed → Thu
        (d(2026, 8, 13), d(2026, 8, 14)), // Thu → Fri
        (d(2026, 8, 14), d(2026, 8, 17)), // Fri → Mon  (+3)
        (d(2026, 8, 15), d(2026, 8, 17)), // Sat → Mon  (+2)
        (d(2026, 8, 16), d(2026, 8, 17)), // Sun → Mon  (+1)
    ];
    for (today, expected) in table {
        assert_eq!(next_business_day(today), expected, "TODAY={today}");
        assert_eq!(
            ArchiveMode::NextBusinessDay.filing_date(today),
            expected,
            "TODAY={today}"
        );
    }
}

/// `F → [range_start … range_end]` for the same week.
///
/// Monday's row is the one the port got wrong: its range **ends on a Sunday**,
/// which is how `git log --until` reaches the weekend's commits at all.
#[test]
fn each_filing_date_claims_the_days_since_the_previous_file() {
    let table = [
        (d(2026, 8, 10), d(2026, 8, 7), d(2026, 8, 9)), // Mon ← Fri..Sun
        (d(2026, 8, 11), d(2026, 8, 10), d(2026, 8, 10)), // Tue ← Mon
        (d(2026, 8, 12), d(2026, 8, 11), d(2026, 8, 11)), // Wed ← Tue
        (d(2026, 8, 13), d(2026, 8, 12), d(2026, 8, 12)), // Thu ← Wed
        (d(2026, 8, 14), d(2026, 8, 13), d(2026, 8, 13)), // Fri ← Thu
        (d(2026, 8, 17), d(2026, 8, 14), d(2026, 8, 16)), // Mon ← Fri..Sun
    ];
    for (f, start, end) in table {
        let window = compute_window(f, ArchiveMode::NextBusinessDay, never_clamps());
        assert_eq!(window.range_start, start, "F={f} range_start");
        assert_eq!(window.range_end, end, "F={f} range_end");
    }
}

#[test]
fn prev_business_day_before_skips_the_whole_weekend() {
    assert_eq!(prev_business_day_before(d(2026, 8, 10)), d(2026, 8, 7)); // Mon → Fri
    assert_eq!(prev_business_day_before(d(2026, 8, 9)), d(2026, 8, 7)); // Sun → Fri
    assert_eq!(prev_business_day_before(d(2026, 8, 8)), d(2026, 8, 7)); // Sat → Fri
    assert_eq!(prev_business_day_before(d(2026, 8, 11)), d(2026, 8, 10)); // Tue → Mon
}

// ── the two failure modes ─────────────────────────────────────────────────

#[test]
fn weekend_work_lands_in_mondays_file_and_not_on_the_floor() {
    // The headline defect. `compile.sh:192-193` steps one *calendar* day at a
    // time, so a Saturday note and a Sunday commit are both read into Monday's
    // standup. Under the old two-business-day window neither day belonged to
    // any file, and the work simply vanished.
    let monday = d(2026, 8, 10);
    let window = compute_window(monday, ArchiveMode::NextBusinessDay, never_clamps());

    assert_eq!(
        window.dates,
        vec![d(2026, 8, 7), d(2026, 8, 8), d(2026, 8, 9)],
        "Friday, Saturday and Sunday all belong to Monday's standup"
    );
    assert_eq!(
        window.range_end.weekday(),
        Weekday::Sun,
        "range_end must be allowed to be a weekend day: git log --until depends on it"
    );

    let (legacy_start, legacy_end) = legacy_window(monday);
    assert!(
        legacy_end < d(2026, 8, 8),
        "the old window stopped at {legacy_end} (start {legacy_start}) and dropped the weekend"
    );
}

#[test]
fn no_natural_day_is_left_out_of_every_file() {
    // Stronger than the Monday case: sweep more than a year and require each
    // calendar day to be claimed by exactly one standup.
    let (from, to) = (d(2026, 1, 1), d(2027, 3, 1));
    for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
        let mut claimed: Vec<NaiveDate> = Vec::new();
        for f in filing_dates(mode, from, to) {
            claimed.extend(compute_window(f, mode, never_clamps()).dates);
        }
        claimed.sort_unstable();

        let mut unique = claimed.clone();
        unique.dedup();
        assert_eq!(
            unique.len(),
            claimed.len(),
            "{mode:?}: at least one day is reported in two standups"
        );

        let span = (claimed[claimed.len() - 1] - claimed[0]).num_days() + 1;
        assert_eq!(
            i64::try_from(claimed.len()).expect("fits"),
            span,
            "{mode:?}: at least one day belongs to no standup at all"
        );

        // And the sweep really did cover weekends, or the check above proves
        // nothing about the case that broke.
        assert!(
            claimed.iter().any(|day| day.weekday() == Weekday::Sat),
            "{mode:?}: the sweep never saw a Saturday"
        );
    }
}

#[test]
fn a_day_of_work_is_never_reported_in_two_standups() {
    // The second defect, stated as the concrete pair a reader can check by hand:
    // Thursday belongs to Friday's file, and to Friday's file only.
    let (thursday, friday, saturday) = (d(2026, 8, 13), d(2026, 8, 14), d(2026, 8, 15));

    let friday_file = compute_window(friday, ArchiveMode::NextBusinessDay, never_clamps());
    assert_eq!(friday_file.dates, vec![thursday]);

    let thursday_file = compute_window(thursday, ArchiveMode::NextBusinessDay, never_clamps());
    assert!(
        !thursday_file.dates.contains(&thursday),
        "Thursday's own file covers Wednesday: {thursday_file:?}"
    );

    let (legacy_start, _) = legacy_window(friday);
    assert!(
        legacy_start < thursday,
        "the old Friday window started at {legacy_start} and re-reported a day \
         Thursday's standup had already published"
    );

    // Saturday is Monday's business, not Friday's.
    assert!(!friday_file.dates.contains(&saturday));
}

// ── the clamp, and the override ───────────────────────────────────────────

#[test]
fn a_window_never_claims_a_day_that_has_not_happened() {
    // `compile.sh:179` clamps range_end with `earlier "$dayb" "$TODAY"`. Filing
    // Monday's standup during Friday's scheduled run must stop at Friday.
    let window = compute_window(d(2026, 8, 10), ArchiveMode::NextBusinessDay, d(2026, 8, 7));
    assert_eq!(window.range_start, d(2026, 8, 7));
    assert_eq!(window.range_end, d(2026, 8, 7));
    assert_eq!(window.dates, vec![d(2026, 8, 7)]);
}

#[test]
fn a_filing_date_past_today_yields_nothing_rather_than_imaginary_work() {
    // `compile.sh:181` returns early instead of writing a standup for a day the
    // calendar has not reached.
    let window = compute_window(d(2026, 8, 11), ArchiveMode::SameDay, d(2026, 8, 10));
    assert!(window.is_empty(), "{window:?}");
    assert!(window.dates.is_empty());
}

#[test]
fn the_date_override_names_a_work_day_not_a_file() {
    // `compile.sh:39` stores `--date` in TODAY, and `:534` still runs it through
    // `next_business_day`. Reading it as a filing date is what made the app
    // write `2026-08-13.md` on Thursday instead of `2026-08-14.md`.
    let overridden = d(2026, 8, 13); // Thursday
    assert_eq!(
        ArchiveMode::NextBusinessDay.filing_date(overridden),
        d(2026, 8, 14),
        "--date 2026-08-13 compiles Friday's standup"
    );
}

#[test]
fn a_weekend_filing_date_is_computable_rather_than_a_panic() {
    // Nothing files a standup on a Saturday, but `--date` arithmetic can still
    // produce one, and the window must invert rather than blow up.
    let window = compute_window(d(2026, 8, 8), ArchiveMode::NextBusinessDay, never_clamps());
    assert_eq!(window.range_start, d(2026, 8, 7));
    assert_eq!(window.range_end, d(2026, 8, 7));
}

// ── same-day mode keeps the same guarantees ───────────────────────────────

#[test]
fn same_day_mode_still_rolls_the_weekend_into_monday() {
    // The opt-in policy changes the offset, never the invariant: no standup is
    // named after a Saturday, so weekend work still accumulates into Monday's.
    for day in [d(2026, 8, 15), d(2026, 8, 16)] {
        assert_eq!(
            ArchiveMode::SameDay.filing_date(day),
            d(2026, 8, 17),
            "{day}"
        );
    }
    let monday = compute_window(d(2026, 8, 17), ArchiveMode::SameDay, never_clamps());
    assert_eq!(monday.range_start, d(2026, 8, 15), "starts on the Saturday");
    assert_eq!(monday.range_end, d(2026, 8, 17), "and claims Monday itself");
}

#[test]
fn consecutive_files_are_contiguous_in_both_modes() {
    for mode in [ArchiveMode::NextBusinessDay, ArchiveMode::SameDay] {
        let mut previous: Option<Window> = None;
        for f in filing_dates(mode, d(2026, 1, 1), d(2027, 3, 1)) {
            let window = compute_window(f, mode, never_clamps());
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

/// Every distinct filing date produced by `[from, to]` under `mode`, ascending.
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
