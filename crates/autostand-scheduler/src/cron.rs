//! Cron schedule parsing + next-run computation.
//! Default: `0 7-19 * * 1-5` (hourly 07-19 weekdays).
//!
//! Supports a 5-field POSIX cron subset: `minute hour day-of-month month day-of-week`.
//! Each field accepts: `*`, a single value, a comma list (`7,8,9`), a range (`7-19`),
//! or a step (`*/2`, `7-19/1`). Names are NOT supported.

use chrono::{DateTime, Datelike, Timelike, Utc};

/// A parsed cron expression (5-field POSIX subset).
#[derive(Debug, Clone)]
pub struct CronSpec {
    minutes: Vec<u32>,
    hours: Vec<u32>,
    doms: Vec<u32>,
    months: Vec<u32>,
    dows: Vec<u32>,
}

/// Parse a 5-field cron expression.
pub fn parse(expr: &str) -> Result<CronSpec, String> {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!("expected 5 fields, got {}", fields.len()));
    }
    Ok(CronSpec {
        minutes: parse_field(fields[0], 0, 59)?,
        hours: parse_field(fields[1], 0, 23)?,
        doms: parse_field(fields[2], 1, 31)?,
        months: parse_field(fields[3], 1, 12)?,
        dows: parse_field(fields[4], 0, 6)?,
    })
}

fn parse_field(field: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let mut out: Vec<u32> = Vec::new();
    for part in field.split(',') {
        for v in expand_part(part, min, max)? {
            if !out.contains(&v) {
                out.push(v);
            }
        }
    }
    if out.is_empty() {
        Err(format!("field '{field}' expanded to no values"))
    } else {
        out.sort_unstable();
        Ok(out)
    }
}

fn expand_part(part: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    if part == "*" {
        return Ok((min..=max).collect());
    }
    if let Some((range, step)) = part.split_once('/') {
        let step: u32 = step.parse().map_err(|_| format!("invalid step '{step}'"))?;
        if step == 0 {
            return Err("step must be > 0".into());
        }
        let base = if range == "*" {
            (min..=max).collect::<Vec<_>>()
        } else {
            expand_range(range, min, max)?
        };
        return Ok(base.into_iter().step_by(step as usize).collect());
    }
    if part.contains('-') {
        return expand_range(part, min, max);
    }
    let v: u32 = part
        .parse()
        .map_err(|_| format!("invalid value '{part}' in field"))?;
    if v < min || v > max {
        return Err(format!("value {v} out of range [{min},{max}]"));
    }
    Ok(vec![v])
}

fn expand_range(range: &str, min: u32, max: u32) -> Result<Vec<u32>, String> {
    let (lo_s, hi_s) = range
        .split_once('-')
        .ok_or_else(|| format!("not a range: {range}"))?;
    let lo: u32 = lo_s
        .parse()
        .map_err(|_| format!("invalid range low '{lo_s}'"))?;
    let hi: u32 = hi_s
        .parse()
        .map_err(|_| format!("invalid range high '{hi_s}'"))?;
    if lo < min || hi > max || lo > hi {
        return Err(format!("range {lo}-{hi} invalid for [{min},{max}]"));
    }
    Ok((lo..=hi).collect())
}

/// Compute the next run after `from` that satisfies `spec`.
pub fn next_run(expr: &str, from: DateTime<Utc>) -> Result<DateTime<Utc>, String> {
    let spec = parse(expr)?;
    Ok(next_run_spec(&spec, from))
}

fn next_run_spec(spec: &CronSpec, from: DateTime<Utc>) -> DateTime<Utc> {
    let mut t = from + chrono::Duration::minutes(1);
    loop {
        if spec.months.contains(&t.month())
            && spec.hours.contains(&t.hour())
            && spec.minutes.contains(&t.minute())
            && day_matches(spec, t)
        {
            return t;
        }
        t += chrono::Duration::minutes(1);
    }
}

/// POSIX cron: if both DOM and DOW are restricted, day matches if either matches;
/// if only one is restricted, that one must match; if both are `*`, day always matches.
fn day_matches(spec: &CronSpec, t: DateTime<Utc>) -> bool {
    let every_day_of_month = spec.doms.len() == 31;
    let every_day_of_week = spec.dows.len() == 7;
    let dom_hit = spec.doms.contains(&t.day());
    let weekday_hit = spec.dows.contains(&t.weekday().num_days_from_sunday());
    match (every_day_of_month, every_day_of_week) {
        (false, false) => dom_hit || weekday_hit,
        (false, true) => dom_hit,
        (true, false) => weekday_hit,
        (true, true) => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn parses_default() {
        let spec = parse("0 7-19 * * 1-5").expect("parse");
        assert_eq!(spec.minutes, vec![0]);
        assert_eq!(spec.hours, (7..=19).collect::<Vec<_>>());
        assert_eq!(spec.doms, (1..=31).collect::<Vec<_>>());
        assert_eq!(spec.months, (1..=12).collect::<Vec<_>>());
        assert_eq!(spec.dows, vec![1, 2, 3, 4, 5]);
    }

    #[test]
    fn next_run_default_finds_next_hour() {
        let from = Utc.with_ymd_and_hms(2026, 8, 3, 6, 30, 0).unwrap();
        let next = next_run("0 7-19 * * 1-5", from).expect("next");
        assert_eq!(next.hour(), 7);
        assert_eq!(next.minute(), 0);
        assert_eq!(next.weekday().num_days_from_sunday(), 1);
    }

    #[test]
    fn next_run_skips_weekend() {
        let fri_evening = Utc.with_ymd_and_hms(2026, 8, 7, 20, 0, 0).unwrap();
        let next = next_run("0 7-19 * * 1-5", fri_evening).expect("next");
        assert_eq!(next.weekday().num_days_from_sunday(), 1);
        assert_eq!(next.hour(), 7);
        assert_eq!(next.day(), 10);
    }

    #[test]
    fn rejects_bad_field_count() {
        assert!(parse("0 7-19 * *").is_err());
        assert!(parse("0 7-19 * * 1-5 extra").is_err());
    }
}
