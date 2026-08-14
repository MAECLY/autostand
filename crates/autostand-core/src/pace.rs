//! Burn-rate projection for a quota window.
//!
//! A percentage on its own does not answer the question a user actually has
//! before compiling: *will this run out before the window resets?* 40% left in
//! a five-hour window is comfortable four hours in and alarming ten minutes in.
//!
//! Pure and dependency-free by design — every input is passed in, so the whole
//! module is table-testable and the caller owns the clock.

use std::time::Duration;

/// How consumption is tracking against the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Pace {
    /// Projected to finish the window with headroom to spare.
    Ahead,
    /// Projected to land close to the limit.
    OnTrack,
    /// Projected to exhaust the window before it resets.
    Behind,
}

/// Fraction of the limit a projection must stay under to count as [`Pace::Ahead`].
const AHEAD_MARGIN: f64 = 0.9;

/// Never project from less than this much elapsed time.
const MINIMUM_ELAPSED: Duration = Duration::from_secs(60);

/// Fraction of the period that must elapse before a projection is meaningful.
const MINIMUM_ELAPSED_FRACTION: u32 = 100;

/// Shortest elapsed time that yields a usable projection for `period`.
///
/// One request in the first seconds of a five-hour window extrapolates to an
/// absurd total; refusing to answer is better than answering wrongly.
fn minimum_elapsed(period: Duration) -> Duration {
    (period / MINIMUM_ELAPSED_FRACTION).max(MINIMUM_ELAPSED)
}

/// Project consumption to the end of the window.
///
/// Returns `None` when the inputs cannot support a projection: a non-positive
/// limit or period, a non-finite reading, or too little of the window elapsed.
#[must_use]
pub fn evaluate(used: f64, limit: f64, elapsed: Duration, period: Duration) -> Option<Pace> {
    if !used.is_finite() || !limit.is_finite() || limit <= 0.0 {
        return None;
    }
    if period.is_zero() || elapsed < minimum_elapsed(period) {
        return None;
    }
    let projected = used / elapsed.as_secs_f64() * period.as_secs_f64();
    if projected <= limit * AHEAD_MARGIN {
        Some(Pace::Ahead)
    } else if projected <= limit {
        Some(Pace::OnTrack)
    } else {
        Some(Pace::Behind)
    }
}

/// Seconds until the quota is exhausted at the current rate.
///
/// `None` when nothing has been consumed yet, when the inputs are unusable, or
/// when the limit is already reached — an exhausted window is a state, not a
/// countdown.
#[must_use]
pub fn seconds_to_run_out(used: f64, limit: f64, elapsed: Duration) -> Option<f64> {
    if !used.is_finite() || !limit.is_finite() || used <= 0.0 || limit <= 0.0 {
        return None;
    }
    let remaining = limit - used;
    if remaining <= 0.0 {
        return None;
    }
    let elapsed = elapsed.as_secs_f64();
    if elapsed <= 0.0 {
        return None;
    }
    Some(remaining / (used / elapsed))
}

#[cfg(test)]
mod tests {
    use super::{evaluate, seconds_to_run_out, Pace};
    use std::time::Duration;

    const FIVE_HOURS: Duration = Duration::from_secs(18_000);
    const WEEK: Duration = Duration::from_secs(604_800);

    #[test]
    fn steady_low_consumption_is_ahead() {
        // 10% used at the halfway mark projects to 20% of a 100% limit.
        let pace = evaluate(10.0, 100.0, FIVE_HOURS / 2, FIVE_HOURS);
        assert_eq!(pace, Some(Pace::Ahead));
    }

    #[test]
    fn landing_just_under_the_limit_is_on_track() {
        // 48% at halfway projects to 96%, inside the limit but past the margin.
        let pace = evaluate(48.0, 100.0, FIVE_HOURS / 2, FIVE_HOURS);
        assert_eq!(pace, Some(Pace::OnTrack));
    }

    #[test]
    fn overshooting_the_limit_is_behind() {
        // 60% at halfway projects to 120%.
        let pace = evaluate(60.0, 100.0, FIVE_HOURS / 2, FIVE_HOURS);
        assert_eq!(pace, Some(Pace::Behind));
    }

    #[test]
    fn the_margin_boundary_lands_on_ahead() {
        // Exactly 90% projected: the margin is inclusive.
        assert_eq!(
            evaluate(45.0, 100.0, FIVE_HOURS / 2, FIVE_HOURS),
            Some(Pace::Ahead)
        );
    }

    #[test]
    fn the_limit_boundary_lands_on_on_track() {
        assert_eq!(
            evaluate(50.0, 100.0, FIVE_HOURS / 2, FIVE_HOURS),
            Some(Pace::OnTrack)
        );
    }

    /// One request in the first minute of a five-hour window would extrapolate
    /// to an absurd total; refusing is better than answering wrongly.
    #[test]
    fn too_little_elapsed_yields_no_projection() {
        assert_eq!(
            evaluate(5.0, 100.0, Duration::from_secs(30), FIVE_HOURS),
            None
        );
        // A week's 1% is 100 minutes, so half an hour is still too early.
        assert_eq!(evaluate(5.0, 100.0, Duration::from_secs(1_800), WEEK), None);
    }

    /// A short window must not be gated by the 1% rule alone: 1% of ten minutes
    /// is six seconds, which is far too little to project from.
    #[test]
    fn the_floor_applies_to_short_windows() {
        let ten_minutes = Duration::from_secs(600);
        assert_eq!(
            evaluate(5.0, 100.0, Duration::from_secs(30), ten_minutes),
            None
        );
        assert!(evaluate(5.0, 100.0, Duration::from_secs(120), ten_minutes).is_some());
    }

    #[test]
    fn unusable_inputs_yield_no_projection() {
        assert_eq!(evaluate(10.0, 0.0, FIVE_HOURS / 2, FIVE_HOURS), None);
        assert_eq!(evaluate(10.0, -5.0, FIVE_HOURS / 2, FIVE_HOURS), None);
        assert_eq!(evaluate(f64::NAN, 100.0, FIVE_HOURS / 2, FIVE_HOURS), None);
        assert_eq!(
            evaluate(10.0, f64::INFINITY, FIVE_HOURS / 2, FIVE_HOURS),
            None
        );
        assert_eq!(evaluate(10.0, 100.0, FIVE_HOURS / 2, Duration::ZERO), None);
    }

    #[test]
    fn run_out_extrapolates_the_current_rate() {
        // 25% in one hour: 75% left at 25%/h is three more hours.
        let seconds = seconds_to_run_out(25.0, 100.0, Duration::from_secs(3_600))
            .expect("a rate this clear projects");
        assert!((seconds - 10_800.0).abs() < 1.0, "got {seconds}");
    }

    #[test]
    fn run_out_is_absent_without_a_rate_or_headroom() {
        assert_eq!(
            seconds_to_run_out(0.0, 100.0, Duration::from_secs(3_600)),
            None
        );
        assert_eq!(
            seconds_to_run_out(100.0, 100.0, Duration::from_secs(3_600)),
            None
        );
        assert_eq!(
            seconds_to_run_out(120.0, 100.0, Duration::from_secs(3_600)),
            None
        );
        assert_eq!(seconds_to_run_out(25.0, 100.0, Duration::ZERO), None);
    }

    #[test]
    fn pace_serializes_snake_case_for_the_ipc_contract() {
        assert_eq!(
            serde_json::to_string(&Pace::OnTrack).unwrap(),
            "\"on_track\""
        );
        assert_eq!(serde_json::to_string(&Pace::Ahead).unwrap(), "\"ahead\"");
        assert_eq!(serde_json::to_string(&Pace::Behind).unwrap(), "\"behind\"");
    }
}
