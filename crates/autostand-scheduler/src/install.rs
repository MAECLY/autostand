//! Installing the **real** OS scheduler unit: `launchd`, `systemd --user`, or
//! Windows Task Scheduler.
//!
//! The in-process runtime ([`crate::cron`] driven by a tokio task inside the
//! app) only fires while the app's window is open. A machine with autostand
//! closed produces no standup, which is exactly the gap this module closes: it
//! writes a unit that runs the app's headless entry point
//! (`autostand-app --compile`) on the user's schedule, whether or not anything
//! is running.
//!
//! # Shape of the module
//!
//! Everything that produces *text* — [`launchd_plist`], [`systemd_service`],
//! [`systemd_timer`], [`schtasks_command`] — is pure, compiled on every
//! platform, and unit-tested everywhere. Only the four functions that touch the
//! machine ([`detect`], [`install`], [`uninstall`], [`unit_contents`]) are
//! behind `cfg`. That split is deliberate: a plist is a string, and a string is
//! testable on a Linux CI runner.
//!
//! # The translated cron subset
//!
//! [`plan`] translates one of autostand's 5-field cron expressions into the
//! `minute × hour × weekday` triple every one of the three schedulers can
//! express, by *enumerating* the expression's runs over a single week through
//! [`crate::cron::next_run`]. Nothing here re-implements cron parsing — the
//! canonical parser stays the only one.
//!
//! An expression outside that subset (a day-of-month or month restriction, or
//! more than [`MAX_WEEKLY_TIMES`] run times in a week) is **rejected**. Silently
//! installing a unit that fires at the wrong time is far worse than refusing:
//! the user would get standups on days they never asked for and never find out
//! why.
//!
//! See `docs/tauri/03-platform-targets.md` § Scheduler per platform and
//! `docs/architecture/04-state-machine.md` § Scheduler source.

use std::path::Path;
use std::process::Command;

use chrono::{Datelike, Duration, TimeZone, Timelike, Utc};
use thiserror::Error;

use crate::cron;

// ── identity ──────────────────────────────────────────────────────────────

/// `launchd` job label, and the plist's filename stem.
///
/// Matches the bundle identifier in `apps/autostand-app/src-tauri/tauri.conf.json`;
/// `launchctl` requires the label and the filename to agree.
pub const LAUNCHD_LABEL: &str = "com.miguel50flowers.autostand";

/// `systemd` unit stem: `autostand.service` + `autostand.timer`.
pub const SYSTEMD_STEM: &str = "autostand";

/// Windows Task Scheduler task name (`schtasks /TN`).
pub const TASK_NAME: &str = "autostand";

/// The argument every installed unit passes to the app binary.
///
/// `main.rs` routes this to the headless pipeline: one compile, one exit code,
/// no window.
pub const COMPILE_ARG: &str = "--compile";

/// Most distinct run times a week's schedule may have.
///
/// A `launchd` plist enumerates one `dict` per time, so an expression such as
/// `*/1 * * * *` would produce a 10 080-entry plist. The cap keeps the unit
/// readable and turns an absurd schedule into a clear error.
pub const MAX_WEEKLY_TIMES: usize = 1500;

/// A Sunday, used as the anchor of [`weekly_times`]' one-week enumeration.
///
/// Any Sunday works — [`plan`] has already established that the expression puts
/// no restriction on day-of-month or month, so every week is identical.
const WEEK_ANCHOR_SUNDAY: (i32, u32, u32) = (2024, 1, 7);

/// Environment variable that forces [`install`] / [`uninstall`] to refuse.
///
/// Set it in CI (or anywhere a run must not touch the user's login items).
pub const NO_INSTALL_ENV: &str = "AUTOSTAND_NO_INSTALL";

// ── kinds ─────────────────────────────────────────────────────────────────

/// Which scheduler is driving autostand on this machine.
///
/// The variants line up one-for-one with the `SchedulerStatus.source` values in
/// `docs/tauri/02-ipc-contracts.md`, so the app can map without inventing a
/// sixth state. [`detect`] never returns [`SchedulerKind::InProcess`]: that is
/// the app's own tokio tick, which the OS cannot see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerKind {
    /// macOS `launchd` agent (`~/Library/LaunchAgents`).
    Launchd,
    /// Linux `systemd --user` timer.
    Systemd,
    /// Windows Task Scheduler task.
    TaskScheduler,
    /// The app's own in-process cron tick (only while the app is open).
    InProcess,
    /// Nothing installed, nothing running.
    None,
}

impl SchedulerKind {
    /// The IPC wire value for this kind.
    ///
    /// Must stay identical to `commands::types::SchedulerSource`'s serde
    /// representation; the app's mapping is checked against this.
    pub fn wire_label(self) -> &'static str {
        match self {
            Self::Launchd => "launchd",
            Self::Systemd => "systemd",
            Self::TaskScheduler => "task-scheduler",
            Self::InProcess => "in-process",
            Self::None => "none",
        }
    }
}

/// Why an install, uninstall or translation could not happen.
#[derive(Debug, Error)]
pub enum InstallError {
    /// The expression is not a valid autostand cron expression.
    #[error("invalid cron '{expr}': {reason}")]
    Cron {
        /// The offending expression, echoed back.
        expr: String,
        /// The canonical parser's complaint.
        reason: String,
    },
    /// The expression parses but no unit can express it.
    #[error("cannot express cron '{expr}' as a {target} schedule: {reason}")]
    Unsupported {
        /// The offending expression, echoed back.
        expr: String,
        /// Which unit format could not carry it.
        target: &'static str,
        /// What specifically does not fit.
        reason: String,
    },
    /// Writing or removing a unit file failed.
    #[error("io: {0}")]
    Io(String),
    /// A `launchctl` / `systemctl` / `schtasks` invocation failed.
    #[error("`{program} {args}` failed: {reason}")]
    Command {
        /// Program that was run.
        program: String,
        /// Arguments it was run with.
        args: String,
        /// Spawn error, or the program's stderr.
        reason: String,
    },
    /// No home directory, so there is nowhere to put a user-scoped unit.
    #[error("no home directory; cannot place a user scheduler unit")]
    NoHome,
    /// This OS has no supported user-scoped scheduler.
    #[error("no system scheduler is supported on this platform")]
    UnsupportedPlatform,
    /// Refused because the caller is a test binary or opted out.
    #[error("refusing to change the user's scheduled jobs from a test run")]
    Sandboxed,
}

impl From<std::io::Error> for InstallError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

// ── cron → schedule ───────────────────────────────────────────────────────

/// A cron expression reduced to what every OS scheduler can express.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Schedule {
    /// Minutes-past-the-hour, ascending, deduplicated.
    pub minutes: Vec<u32>,
    /// Hours, ascending, deduplicated.
    pub hours: Vec<u32>,
    /// Days of the week in cron numbering (`0` = Sunday), ascending.
    /// `None` means "every day", which every format spells as the absence of a
    /// day restriction rather than as a seven-element list.
    pub weekdays: Option<Vec<u32>>,
}

impl Schedule {
    /// How many discrete run times a week this schedule has.
    pub fn weekly_count(&self) -> usize {
        let days = self.weekdays.as_ref().map_or(7, Vec::len);
        self.minutes.len() * self.hours.len() * days
    }
}

/// Translate a 5-field cron expression into a [`Schedule`].
///
/// The runs are *enumerated* through [`crate::cron::next_run`] rather than
/// re-parsed, so the translation inherits the canonical semantics — including
/// the POSIX day-of-month/day-of-week rules — instead of guessing at them.
///
/// # Errors
///
/// [`InstallError::Cron`] when the expression does not parse;
/// [`InstallError::Unsupported`] when it restricts day-of-month or month (no
/// unit format can carry cron's "DOM *or* DOW" semantics faithfully), or when it
/// has more than [`MAX_WEEKLY_TIMES`] run times in a week.
pub fn plan(expr: &str) -> Result<Schedule, InstallError> {
    let trimmed = expr.trim();
    cron::parse(trimmed).map_err(|reason| InstallError::Cron {
        expr: trimmed.to_string(),
        reason,
    })?;

    let fields: Vec<&str> = trimmed.split_whitespace().collect();
    // Checked textually, not by expansion: `1-31` and `*/1` *mean* "every day",
    // but accepting them would mean accepting `*/2` on the same code path, and
    // a fortnightly-ish day-of-month rule has no faithful unit equivalent.
    for (index, name) in [(2, "day-of-month"), (3, "month")] {
        if fields.get(index).copied() != Some("*") {
            return Err(InstallError::Unsupported {
                expr: trimmed.to_string(),
                target: "system unit",
                reason: format!("the {name} field must be '*'"),
            });
        }
    }

    let times = weekly_times(trimmed)?;
    if times.is_empty() {
        return Err(InstallError::Unsupported {
            expr: trimmed.to_string(),
            target: "system unit",
            reason: "the expression never fires".to_string(),
        });
    }

    let weekdays = sorted_unique(times.iter().map(|&(dow, _, _)| dow));
    let hours = sorted_unique(times.iter().map(|&(_, hour, _)| hour));
    let minutes = sorted_unique(times.iter().map(|&(_, _, minute)| minute));

    let schedule = Schedule {
        minutes,
        hours,
        weekdays: if weekdays.len() == 7 {
            None
        } else {
            Some(weekdays)
        },
    };
    // With day-of-month and month unrestricted, a cron expression is exactly the
    // cross product of its minute, hour and weekday sets. If the enumeration
    // disagrees, the translation would be lossy and must not be installed.
    if schedule.weekly_count() != times.len() {
        return Err(InstallError::Unsupported {
            expr: trimmed.to_string(),
            target: "system unit",
            reason: "the run times are not a minute × hour × weekday grid".to_string(),
        });
    }
    Ok(schedule)
}

/// Every `(weekday, hour, minute)` the expression fires at during one week.
fn weekly_times(expr: &str) -> Result<Vec<(u32, u32, u32)>, InstallError> {
    let (year, month, day) = WEEK_ANCHOR_SUNDAY;
    let start = Utc
        .with_ymd_and_hms(year, month, day, 0, 0, 0)
        .single()
        .expect("the hard-coded week anchor is a real UTC instant");
    let end = start + Duration::days(7);
    // `next_run` is strictly-after, so back off one minute to keep 00:00 Sunday
    // inside the window.
    let mut cursor = start - Duration::minutes(1);

    let mut out: Vec<(u32, u32, u32)> = Vec::new();
    loop {
        let next = cron::next_run(expr, cursor).map_err(|reason| InstallError::Cron {
            expr: expr.to_string(),
            reason,
        })?;
        if next >= end {
            return Ok(out);
        }
        out.push((
            next.weekday().num_days_from_sunday(),
            next.hour(),
            next.minute(),
        ));
        if out.len() > MAX_WEEKLY_TIMES {
            return Err(InstallError::Unsupported {
                expr: expr.to_string(),
                target: "system unit",
                reason: format!("more than {MAX_WEEKLY_TIMES} run times per week"),
            });
        }
        cursor = next;
    }
}

/// Collect an iterator into an ascending, deduplicated vector.
fn sorted_unique(values: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut out: Vec<u32> = values.collect();
    out.sort_unstable();
    out.dedup();
    out
}

// ── macOS: launchd ────────────────────────────────────────────────────────

/// The `LaunchAgent` plist that runs `exe --compile` on `cron`'s schedule.
///
/// `StartCalendarInterval` is an array of dicts, one per run time; `launchd`
/// fires when *every* key in a dict matches, and any dict matching is enough.
/// A `Weekday` key is emitted only when the schedule restricts days — an
/// every-day schedule would otherwise need seven times as many entries.
///
/// `RunAtLoad` is `false` on purpose: loading the agent (which happens on every
/// login) must not itself compile a standup.
///
/// # Errors
///
/// Whatever [`plan`] returns.
pub fn launchd_plist(cron_expr: &str, exe: &Path) -> Result<String, InstallError> {
    let schedule = plan(cron_expr)?;
    let mut intervals = String::new();
    match schedule.weekdays.as_deref() {
        Some(days) => {
            for &day in days {
                push_intervals(&mut intervals, Some(day), &schedule);
            }
        }
        None => push_intervals(&mut intervals, None, &schedule),
    }

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>{label}</string>
	<key>ProgramArguments</key>
	<array>
		<string>{exe}</string>
		<string>{arg}</string>
	</array>
	<key>StartCalendarInterval</key>
	<array>
{intervals}	</array>
	<key>RunAtLoad</key>
	<false/>
	<key>ProcessType</key>
	<string>Background</string>
</dict>
</plist>
"#,
        label = LAUNCHD_LABEL,
        exe = xml_escape(&exe.to_string_lossy()),
        arg = COMPILE_ARG,
    ))
}

/// Append one `StartCalendarInterval` dict per `(hour, minute)` of `schedule`.
fn push_intervals(out: &mut String, weekday: Option<u32>, schedule: &Schedule) {
    use std::fmt::Write as _;

    for &hour in &schedule.hours {
        for &minute in &schedule.minutes {
            out.push_str("\t\t<dict>");
            if let Some(day) = weekday {
                // Writing into a String is infallible; the Result exists only
                // because `write!` is generic over `io::Write` too.
                let _ = write!(out, "<key>Weekday</key><integer>{day}</integer>");
            }
            let _ = write!(out, "<key>Hour</key><integer>{hour}</integer>");
            let _ = write!(out, "<key>Minute</key><integer>{minute}</integer>");
            out.push_str("</dict>\n");
        }
    }
}

/// Escape the five XML metacharacters that can appear in a filesystem path.
fn xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ── Linux: systemd --user ─────────────────────────────────────────────────

/// The `autostand.service` unit: a one-shot that runs `exe --compile`.
///
/// The schedule lives in the *timer*, never here, so changing the cron rewrites
/// one file and leaves the other byte-identical.
pub fn systemd_service(exe: &Path) -> String {
    format!(
        "[Unit]
Description=autostand daily standup compile

[Service]
Type=oneshot
ExecStart=\"{exe}\" {arg}
",
        exe = systemd_escape(&exe.to_string_lossy()),
        arg = COMPILE_ARG,
    )
}

/// The `autostand.timer` unit carrying `cron_expr` as an `OnCalendar=` rule.
///
/// `Persistent=true` is what makes a missed boundary (laptop asleep, machine
/// off) fire once on the next boot instead of being lost — the same guarantee
/// the in-process runtime gets from its durable last-run record.
///
/// # Errors
///
/// Whatever [`plan`] returns.
pub fn systemd_timer(cron_expr: &str) -> Result<String, InstallError> {
    let on_calendar = on_calendar(&plan(cron_expr)?);
    Ok(format!(
        "[Unit]
Description=autostand standup schedule

[Timer]
OnCalendar={on_calendar}
Persistent=true
AccuracySec=1min

[Install]
WantedBy=timers.target
"
    ))
}

/// Render a [`Schedule`] as a `systemd.time(7)` `OnCalendar` expression.
///
/// Shape: `[<weekdays> ]*-*-* <hours>:<minutes>:00`. The date component stays
/// `*-*-*` because [`plan`] has already refused any day-of-month or month
/// restriction.
fn on_calendar(schedule: &Schedule) -> String {
    let days = schedule.weekdays.as_ref().map_or_else(String::new, |days| {
        // systemd counts weekdays Mon…Sun, cron counts them Sun…Sat; ranges are
        // only contiguous in systemd's order.
        let mut indices: Vec<u32> = days.iter().map(|&dow| (dow + 6) % 7).collect();
        indices.sort_unstable();
        format!("{} ", compress(&indices, systemd_day_name))
    });
    format!(
        "{days}*-*-* {hours}:{minutes}:00",
        hours = compress(&schedule.hours, two_digits),
        minutes = compress(&schedule.minutes, two_digits),
    )
}

/// systemd's name for a Monday-based weekday index.
fn systemd_day_name(index: u32) -> String {
    match index {
        0 => "Mon",
        1 => "Tue",
        2 => "Wed",
        3 => "Thu",
        4 => "Fri",
        5 => "Sat",
        _ => "Sun",
    }
    .to_string()
}

/// Zero-padded two-digit rendering of an hour or minute.
fn two_digits(value: u32) -> String {
    format!("{value:02}")
}

/// Render ascending `values` as a systemd list, collapsing runs of three or
/// more consecutive values into an `a..b` range.
fn compress(values: &[u32], label: impl Fn(u32) -> String) -> String {
    let mut parts: Vec<String> = Vec::new();
    let mut start = 0usize;
    while start < values.len() {
        let mut end = start;
        while end + 1 < values.len() && values[end + 1] == values[end] + 1 {
            end += 1;
        }
        if end - start >= 2 {
            parts.push(format!("{}..{}", label(values[start]), label(values[end])));
        } else {
            for value in &values[start..=end] {
                parts.push(label(*value));
            }
        }
        start = end + 1;
    }
    parts.join(",")
}

/// Escape a path for a systemd `ExecStart=` value.
///
/// `%` opens a unit specifier and must be doubled; the value is wrapped in
/// double quotes by the caller, so quotes and backslashes need escaping too.
fn systemd_escape(raw: &str) -> String {
    raw.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('%', "%%")
}

// ── Windows: Task Scheduler ───────────────────────────────────────────────

/// The `/TR` value: the quoted executable followed by `--compile`.
pub fn task_run_value(exe: &Path) -> String {
    format!("\"{}\" {COMPILE_ARG}", exe.to_string_lossy())
}

/// `schtasks` arguments that create (or replace) the autostand task.
///
/// Task Scheduler has one start time per task plus an optional repetition, so
/// the cron subset it can carry is narrower than the other two:
///
/// - exactly one minute value (`/ST` takes a single `HH:MM`), and
/// - hours that form an arithmetic progression, rendered as `/RI` + `/DU`.
///
/// # Errors
///
/// [`InstallError::Unsupported`] when the schedule needs several start minutes
/// or unevenly spaced hours, plus whatever [`plan`] returns.
pub fn schtasks_args(cron_expr: &str, exe: &Path) -> Result<Vec<String>, InstallError> {
    let schedule = plan(cron_expr)?;
    let unsupported = |reason: String| InstallError::Unsupported {
        expr: cron_expr.trim().to_string(),
        target: "Task Scheduler",
        reason,
    };

    let [minute] = schedule.minutes[..] else {
        return Err(unsupported(
            "/ST takes a single start minute, and this schedule needs several".to_string(),
        ));
    };
    let first = *schedule
        .hours
        .first()
        .expect("plan rejects an empty schedule");
    let last = *schedule
        .hours
        .last()
        .expect("plan rejects an empty schedule");
    let repetition = hour_step(&schedule.hours).ok_or_else(|| {
        unsupported("the hours are not evenly spaced, so /RI cannot express them".to_string())
    })?;

    let mut args: Vec<String> = vec![
        "/Create".into(),
        "/F".into(),
        "/TN".into(),
        TASK_NAME.into(),
        "/TR".into(),
        task_run_value(exe),
    ];
    args.push("/SC".into());
    if let Some(days) = schedule.weekdays.as_deref() {
        args.push("WEEKLY".into());
        args.push("/D".into());
        args.push(windows_day_list(days));
    } else {
        args.push("DAILY".into());
    }
    args.push("/ST".into());
    args.push(format!("{first:02}:{minute:02}"));
    if repetition > 0 {
        args.push("/RI".into());
        args.push((repetition * 60).to_string());
        args.push("/DU".into());
        args.push(format!("{:02}:00", last - first));
    }
    Ok(args)
}

/// The `schtasks` command line as a human would type it.
///
/// This is what [`unit_contents`] returns on Windows: Task Scheduler has no
/// unit *file* the way `launchd` and `systemd` do, so the command **is** the
/// artifact. [`install`] runs the same arguments through
/// [`std::process::Command`], which does its own quoting.
///
/// # Errors
///
/// Whatever [`schtasks_args`] returns.
pub fn schtasks_command(cron_expr: &str, exe: &Path) -> Result<String, InstallError> {
    let rendered: Vec<String> = schtasks_args(cron_expr, exe)?
        .iter()
        .map(|arg| shell_quote(arg))
        .collect();
    Ok(format!("schtasks {}", rendered.join(" ")))
}

/// Constant gap in hours between consecutive run hours.
///
/// `None` means the hours are unevenly spaced, which `/RI` cannot express. A
/// single hour reports `0`: there is no gap, and no repetition to install.
fn hour_step(hours: &[u32]) -> Option<u32> {
    let Some(step) = hours.windows(2).next().map(|pair| pair[1] - pair[0]) else {
        return Some(0);
    };
    if hours.windows(2).all(|pair| pair[1] - pair[0] == step) {
        Some(step)
    } else {
        None
    }
}

/// Windows' `/D` day list, in Monday-first order.
fn windows_day_list(cron_days: &[u32]) -> String {
    let mut indices: Vec<u32> = cron_days.iter().map(|&dow| (dow + 6) % 7).collect();
    indices.sort_unstable();
    indices
        .into_iter()
        .map(windows_day_token)
        .collect::<Vec<_>>()
        .join(",")
}

/// Windows' token for a Monday-based weekday index.
fn windows_day_token(index: u32) -> &'static str {
    match index {
        0 => "MON",
        1 => "TUE",
        2 => "WED",
        3 => "THU",
        4 => "FRI",
        5 => "SAT",
        _ => "SUN",
    }
}

/// Quote an argument for display in a copy-pasteable command line.
fn shell_quote(arg: &str) -> String {
    if arg.contains(' ') || arg.contains('"') {
        format!("\"{}\"", arg.replace('"', "\\\""))
    } else {
        arg.to_string()
    }
}

// ── install guards ────────────────────────────────────────────────────────

/// May this process change the user's scheduled jobs?
///
/// Installing a unit edits the user's login items, so it must never happen as a
/// side effect of a test run. Three independent guards, because each one alone
/// has a hole: `cfg!(test)` only covers this crate's own unit tests, the env
/// var only covers callers that remember to set it, and the exe-path check only
/// covers cargo's harness layout.
fn may_touch_scheduled_jobs() -> bool {
    if cfg!(test) || std::env::var_os(NO_INSTALL_ENV).is_some() {
        return false;
    }
    !std::env::current_exe().is_ok_and(|exe| is_cargo_test_binary(&exe))
}

/// Does `exe` look like a binary cargo built for `cargo test`?
///
/// Test and benchmark harnesses land in `target/<profile>/deps/`; `cargo run`
/// and an installed app never do.
fn is_cargo_test_binary(exe: &Path) -> bool {
    exe.parent()
        .and_then(Path::file_name)
        .is_some_and(|name| name == "deps")
}

/// [`may_touch_scheduled_jobs`] as a `Result`, for use with `?`.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn guard() -> Result<(), InstallError> {
    if may_touch_scheduled_jobs() {
        Ok(())
    } else {
        Err(InstallError::Sandboxed)
    }
}

// ── shared side-effecting helpers ─────────────────────────────────────────

/// Write a unit file, creating its directory, at mode `0644`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn write_unit(path: &Path, contents: &str) -> Result<(), InstallError> {
    use std::os::unix::fs::PermissionsExt;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o644))?;
    Ok(())
}

/// Remove a file, treating "already gone" as success.
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn remove_unit(path: &Path) -> Result<(), InstallError> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(err) => Err(err.into()),
    }
}

/// Run a command, turning a non-zero exit into an [`InstallError::Command`].
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run(program: &str, args: &[&str]) -> Result<(), InstallError> {
    let fail = |reason: String| InstallError::Command {
        program: program.to_string(),
        args: args.join(" "),
        reason,
    };
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|err| fail(err.to_string()))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    Err(fail(if stderr.is_empty() {
        format!("exit {}", output.status)
    } else {
        stderr
    }))
}

/// Run a command whose failure is expected and harmless (an unload that finds
/// nothing loaded), logging rather than propagating.
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
fn run_best_effort(program: &str, args: &[&str]) {
    if let Err(err) = run(program, args) {
        tracing::debug!(error = %err, "scheduler: best-effort command failed");
    }
}

// ── macOS implementation ──────────────────────────────────────────────────

/// Path of the `LaunchAgent` plist.
#[cfg(target_os = "macos")]
fn agent_path() -> Result<std::path::PathBuf, InstallError> {
    Ok(dirs::home_dir()
        .ok_or(InstallError::NoHome)?
        .join("Library")
        .join("LaunchAgents")
        .join(format!("{LAUNCHD_LABEL}.plist")))
}

/// The user's numeric uid, read from the ownership of their home directory.
///
/// `launchctl bootstrap` needs the `gui/<uid>` domain target. Reading the uid
/// off `$HOME` avoids both a `libc` dependency (the workspace denies
/// `unsafe_code`) and an `id -u` subprocess.
#[cfg(target_os = "macos")]
fn current_uid() -> Result<u32, InstallError> {
    use std::os::unix::fs::MetadataExt;

    let home = dirs::home_dir().ok_or(InstallError::NoHome)?;
    Ok(std::fs::metadata(home)?.uid())
}

/// What is installed on this machine right now.
///
/// Read-only and cheap: it never installs anything, so
/// `get_scheduler_status` may call it freely.
#[cfg(target_os = "macos")]
pub fn detect() -> SchedulerKind {
    if agent_path().is_ok_and(|path| path.is_file()) {
        SchedulerKind::Launchd
    } else {
        SchedulerKind::None
    }
}

/// Write the `LaunchAgent` plist and (re)bootstrap it.
///
/// `bootout` before `bootstrap` because `launchd` refuses to bootstrap a label
/// that is already loaded; a label that was *not* loaded makes `bootout` fail,
/// which is why it is best-effort. The modern subcommands are used
/// deliberately — `load`/`unload` are deprecated and silently no-op in some
/// session types.
///
/// # Errors
///
/// [`InstallError::Sandboxed`] from a test binary, plus any translation,
/// filesystem or `launchctl` failure.
#[cfg(target_os = "macos")]
pub fn install(cron_expr: &str, exe: &Path) -> Result<SchedulerKind, InstallError> {
    guard()?;
    let contents = launchd_plist(cron_expr, exe)?;
    let path = agent_path()?;
    let domain = format!("gui/{}", current_uid()?);

    run_best_effort(
        "launchctl",
        &["bootout", &format!("{domain}/{LAUNCHD_LABEL}")],
    );
    write_unit(&path, &contents)?;
    run(
        "launchctl",
        &["bootstrap", &domain, &path.to_string_lossy()],
    )?;
    Ok(SchedulerKind::Launchd)
}

/// Unload the agent and delete its plist.
///
/// # Errors
///
/// [`InstallError::Sandboxed`] from a test binary, or a filesystem failure.
#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<(), InstallError> {
    guard()?;
    let path = agent_path()?;
    if let Ok(uid) = current_uid() {
        run_best_effort(
            "launchctl",
            &["bootout", &format!("gui/{uid}/{LAUNCHD_LABEL}")],
        );
    }
    remove_unit(&path)
}

/// The plist [`install`] writes.
///
/// # Errors
///
/// Whatever [`launchd_plist`] returns.
#[cfg(target_os = "macos")]
pub fn unit_contents(cron_expr: &str, exe: &Path) -> Result<String, InstallError> {
    launchd_plist(cron_expr, exe)
}

// ── Linux implementation ──────────────────────────────────────────────────

/// Directory holding the user's systemd units.
#[cfg(target_os = "linux")]
fn units_dir() -> Result<std::path::PathBuf, InstallError> {
    Ok(dirs::config_dir()
        .ok_or(InstallError::NoHome)?
        .join("systemd")
        .join("user"))
}

/// What is installed on this machine right now.
///
/// Both the timer *and* the `timers.target.wants` symlink must be present: the
/// unit file alone only means "written", while the symlink is what
/// `systemctl --user enable` creates and is what actually arms the timer.
#[cfg(target_os = "linux")]
pub fn detect() -> SchedulerKind {
    let Ok(dir) = units_dir() else {
        return SchedulerKind::None;
    };
    let timer = dir.join(format!("{SYSTEMD_STEM}.timer"));
    let wants = dir
        .join("timers.target.wants")
        .join(format!("{SYSTEMD_STEM}.timer"));
    if timer.is_file() && wants.exists() {
        SchedulerKind::Systemd
    } else {
        SchedulerKind::None
    }
}

/// Write both units, reload the user manager and arm the timer.
///
/// # Errors
///
/// [`InstallError::Sandboxed`] from a test binary, plus any translation,
/// filesystem or `systemctl` failure.
#[cfg(target_os = "linux")]
pub fn install(cron_expr: &str, exe: &Path) -> Result<SchedulerKind, InstallError> {
    guard()?;
    let service = systemd_service(exe);
    let timer = systemd_timer(cron_expr)?;
    let dir = units_dir()?;

    write_unit(&dir.join(format!("{SYSTEMD_STEM}.service")), &service)?;
    write_unit(&dir.join(format!("{SYSTEMD_STEM}.timer")), &timer)?;
    run("systemctl", &["--user", "daemon-reload"])?;
    run(
        "systemctl",
        &[
            "--user",
            "enable",
            "--now",
            &format!("{SYSTEMD_STEM}.timer"),
        ],
    )?;
    Ok(SchedulerKind::Systemd)
}

/// Disarm the timer, delete both units and reload.
///
/// # Errors
///
/// [`InstallError::Sandboxed`] from a test binary, or a filesystem failure.
#[cfg(target_os = "linux")]
pub fn uninstall() -> Result<(), InstallError> {
    guard()?;
    let dir = units_dir()?;
    run_best_effort(
        "systemctl",
        &[
            "--user",
            "disable",
            "--now",
            &format!("{SYSTEMD_STEM}.timer"),
        ],
    );
    remove_unit(&dir.join(format!("{SYSTEMD_STEM}.timer")))?;
    remove_unit(&dir.join(format!("{SYSTEMD_STEM}.service")))?;
    run_best_effort("systemctl", &["--user", "daemon-reload"]);
    Ok(())
}

/// Both units [`install`] writes, each preceded by its filename.
///
/// Unlike macOS and Windows, Linux needs two files; the returned text is a
/// human-readable transcript rather than a single installable file.
///
/// # Errors
///
/// Whatever [`systemd_timer`] returns.
#[cfg(target_os = "linux")]
pub fn unit_contents(cron_expr: &str, exe: &Path) -> Result<String, InstallError> {
    Ok(format!(
        "# {SYSTEMD_STEM}.service\n{service}\n# {SYSTEMD_STEM}.timer\n{timer}",
        service = systemd_service(exe),
        timer = systemd_timer(cron_expr)?,
    ))
}

// ── Windows implementation ────────────────────────────────────────────────

/// What is installed on this machine right now.
///
/// Task Scheduler keeps its store outside the user's home, so this asks
/// `schtasks` instead of looking for a file. The query is read-only.
#[cfg(windows)]
pub fn detect() -> SchedulerKind {
    let queried = Command::new("schtasks")
        .args(["/Query", "/TN", TASK_NAME])
        .output();
    match queried {
        Ok(output) if output.status.success() => SchedulerKind::TaskScheduler,
        _ => SchedulerKind::None,
    }
}

/// Create or replace the scheduled task (`/F` overwrites).
///
/// # Errors
///
/// [`InstallError::Sandboxed`] from a test binary, plus any translation or
/// `schtasks` failure.
#[cfg(windows)]
pub fn install(cron_expr: &str, exe: &Path) -> Result<SchedulerKind, InstallError> {
    guard()?;
    let args = schtasks_args(cron_expr, exe)?;
    let borrowed: Vec<&str> = args.iter().map(String::as_str).collect();
    run("schtasks", &borrowed)?;
    Ok(SchedulerKind::TaskScheduler)
}

/// Delete the scheduled task.
///
/// # Errors
///
/// [`InstallError::Sandboxed`] from a test binary, or a `schtasks` failure.
#[cfg(windows)]
pub fn uninstall() -> Result<(), InstallError> {
    guard()?;
    run("schtasks", &["/Delete", "/F", "/TN", TASK_NAME])
}

/// The `schtasks` command line [`install`] runs.
///
/// # Errors
///
/// Whatever [`schtasks_command`] returns.
#[cfg(windows)]
pub fn unit_contents(cron_expr: &str, exe: &Path) -> Result<String, InstallError> {
    schtasks_command(cron_expr, exe)
}

// ── unsupported platforms ─────────────────────────────────────────────────

/// No user-scoped scheduler is known here.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn detect() -> SchedulerKind {
    SchedulerKind::None
}

/// Always [`InstallError::UnsupportedPlatform`].
///
/// # Errors
///
/// Always.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn install(_cron_expr: &str, _exe: &Path) -> Result<SchedulerKind, InstallError> {
    Err(InstallError::UnsupportedPlatform)
}

/// Always [`InstallError::UnsupportedPlatform`].
///
/// # Errors
///
/// Always.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn uninstall() -> Result<(), InstallError> {
    Err(InstallError::UnsupportedPlatform)
}

/// Always [`InstallError::UnsupportedPlatform`].
///
/// # Errors
///
/// Always.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn unit_contents(_cron_expr: &str, _exe: &Path) -> Result<String, InstallError> {
    Err(InstallError::UnsupportedPlatform)
}

#[cfg(test)]
mod tests {
    use super::{
        detect, install, is_cargo_test_binary, launchd_plist, may_touch_scheduled_jobs, plan,
        schtasks_args, schtasks_command, systemd_service, systemd_timer, task_run_value, uninstall,
        InstallError, Schedule, SchedulerKind, MAX_WEEKLY_TIMES,
    };
    use std::path::{Path, PathBuf};

    /// The shipped default schedule: hourly, 07–19, Monday to Friday.
    const HOURLY_WEEKDAYS: &str = "0 7-19 * * 1-5";

    /// A plain POSIX install path.
    fn exe() -> PathBuf {
        PathBuf::from("/Applications/autostand.app/Contents/MacOS/autostand")
    }

    // ── cron translation ──────────────────────────────────────────────────

    #[test]
    fn translates_the_default_schedule() {
        let schedule = plan(HOURLY_WEEKDAYS).expect("the default schedule is installable");
        assert_eq!(
            schedule,
            Schedule {
                minutes: vec![0],
                hours: (7..=19).collect(),
                weekdays: Some(vec![1, 2, 3, 4, 5]),
            }
        );
        assert_eq!(schedule.weekly_count(), 65);
    }

    #[test]
    fn an_every_day_schedule_drops_the_weekday_restriction() {
        // `None` (not a seven-element list) is what every unit format wants.
        let schedule = plan("30 9 * * *").expect("daily 09:30 is installable");
        assert_eq!(schedule.weekdays, None);
        assert_eq!(schedule.hours, vec![9]);
        assert_eq!(schedule.minutes, vec![30]);
    }

    #[test]
    fn a_full_weekday_range_is_also_every_day() {
        assert_eq!(plan("0 9 * * 0-6").expect("0-6").weekdays, None);
    }

    #[test]
    fn expands_comma_lists_and_steps() {
        let schedule = plan("0,30 8,17 * * 1,3,5").expect("installable");
        assert_eq!(schedule.minutes, vec![0, 30]);
        assert_eq!(schedule.hours, vec![8, 17]);
        assert_eq!(schedule.weekdays, Some(vec![1, 3, 5]));
        let stepped = plan("*/20 9 * * 1-5").expect("installable");
        assert_eq!(stepped.minutes, vec![0, 20, 40]);
    }

    #[test]
    fn weekday_zero_is_sunday_in_cron_numbering() {
        assert_eq!(plan("0 9 * * 0").expect("Sunday").weekdays, Some(vec![0]));
        assert_eq!(plan("0 9 * * 6").expect("Saturday").weekdays, Some(vec![6]));
    }

    #[test]
    fn rejects_an_unparseable_expression() {
        for bad in ["", "0 7-19 * *", "60 * * * *", "every hour", "*/0 * * * *"] {
            match plan(bad) {
                Ok(schedule) => panic!("{bad:?} must be rejected, got {schedule:?}"),
                Err(err) => assert!(matches!(err, InstallError::Cron { .. }), "{bad:?} → {err}"),
            }
        }
    }

    #[test]
    fn rejects_a_day_of_month_or_month_restriction() {
        // These are the expressions whose POSIX "DOM *or* DOW" semantics no unit
        // format carries faithfully; installing an approximation would fire on
        // days the user never asked for.
        for bad in ["0 9 1 * *", "0 9 1,15 * *", "0 9 * 3 *", "0 9 1 * 1"] {
            let err = plan(bad).expect_err("{bad} must be rejected");
            assert!(
                matches!(err, InstallError::Unsupported { .. }),
                "{bad:?} → {err}"
            );
            assert!(err.to_string().contains(bad), "{err}");
        }
    }

    #[test]
    fn rejects_a_schedule_with_too_many_weekly_run_times() {
        // Every minute of every day is 10 080 times a week.
        let err = plan("* * * * *").expect_err("minutely is not installable");
        match err {
            InstallError::Unsupported { reason, .. } => {
                assert!(reason.contains(&MAX_WEEKLY_TIMES.to_string()), "{reason}");
            }
            other => panic!("expected an Unsupported error, got {other}"),
        }
    }

    #[test]
    fn accepts_a_dense_but_bounded_schedule() {
        // 6 × 24 × 7 = 1008 run times: dense, still under the cap.
        assert_eq!(
            plan("*/10 * * * *").expect("installable").weekly_count(),
            1008
        );
    }

    // ── launchd ───────────────────────────────────────────────────────────

    #[test]
    fn the_plist_names_the_binary_the_label_and_the_compile_flag() {
        let plist = launchd_plist(HOURLY_WEEKDAYS, &exe()).expect("installable");
        assert!(plist.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(
            plist.contains("<key>Label</key>\n\t<string>com.miguel50flowers.autostand</string>")
        );
        assert!(
            plist.contains("<string>/Applications/autostand.app/Contents/MacOS/autostand</string>")
        );
        assert!(plist.contains("<string>--compile</string>"));
        // Loading the agent happens at every login; it must not compile.
        assert!(plist.contains("<key>RunAtLoad</key>\n\t<false/>"));
        assert!(plist.trim_end().ends_with("</plist>"));
    }

    #[test]
    fn the_plist_has_one_calendar_entry_per_run_time() {
        let plist = launchd_plist(HOURLY_WEEKDAYS, &exe()).expect("installable");
        assert_eq!(plist.matches("<dict><key>Weekday</key>").count(), 65);
        assert!(plist
            .contains("<dict><key>Weekday</key><integer>1</integer><key>Hour</key><integer>7</integer><key>Minute</key><integer>0</integer></dict>"));
        assert!(plist
            .contains("<dict><key>Weekday</key><integer>5</integer><key>Hour</key><integer>19</integer><key>Minute</key><integer>0</integer></dict>"));
        // Saturday and Sunday are not on a Mon-Fri schedule.
        assert!(!plist.contains("<key>Weekday</key><integer>0</integer>"));
        assert!(!plist.contains("<key>Weekday</key><integer>6</integer>"));
    }

    #[test]
    fn an_every_day_plist_omits_the_weekday_key_entirely() {
        // Emitting seven entries per time would be seven times the plist for the
        // same schedule.
        let plist = launchd_plist("0 9 * * *", &exe()).expect("installable");
        assert!(!plist.contains("<key>Weekday</key>"), "{plist}");
        assert_eq!(plist.matches("<dict><key>Hour</key>").count(), 1);
    }

    #[test]
    fn the_plist_escapes_xml_metacharacters_in_the_path() {
        // A path is arbitrary bytes; an unescaped `&` makes the plist unparseable
        // and `launchctl bootstrap` rejects the whole agent.
        let plist =
            launchd_plist("0 9 * * 1", Path::new("/Apps/a&b<c>/auto\"stand")).expect("installable");
        assert!(plist.contains("<string>/Apps/a&amp;b&lt;c&gt;/auto&quot;stand</string>"));
        assert!(!plist.contains("a&b"));
    }

    #[test]
    fn an_unexpressible_cron_never_reaches_the_plist() {
        assert!(matches!(
            launchd_plist("0 9 1 * *", &exe()),
            Err(InstallError::Unsupported { .. })
        ));
        assert!(matches!(
            launchd_plist("nope", &exe()),
            Err(InstallError::Cron { .. })
        ));
    }

    // ── systemd ───────────────────────────────────────────────────────────

    #[test]
    fn the_service_is_a_oneshot_that_runs_the_compile_flag() {
        let service = systemd_service(Path::new("/usr/local/bin/autostand-app"));
        assert!(service.contains("Type=oneshot"));
        assert!(service.contains("ExecStart=\"/usr/local/bin/autostand-app\" --compile"));
        // The schedule belongs to the timer; a cron change must not touch this.
        assert!(!service.contains("OnCalendar"));
    }

    #[test]
    fn the_service_escapes_systemd_specifiers_in_the_path() {
        // `%h` in an unescaped ExecStart would expand to the user's home.
        let service = systemd_service(Path::new("/opt/100%/auto\\stand"));
        assert!(service.contains("ExecStart=\"/opt/100%%/auto\\\\stand\" --compile"));
    }

    #[test]
    fn the_timer_compresses_the_default_schedule_into_ranges() {
        let timer = systemd_timer(HOURLY_WEEKDAYS).expect("installable");
        assert!(
            timer.contains("OnCalendar=Mon..Fri *-*-* 07..19:00:00"),
            "{timer}"
        );
        // Without Persistent a missed boundary (machine asleep) is lost forever.
        assert!(timer.contains("Persistent=true"));
        assert!(timer.contains("WantedBy=timers.target"));
    }

    #[test]
    fn the_timer_lists_non_contiguous_values() {
        let timer = systemd_timer("0,30 8,17 * * 1,3,5").expect("installable");
        assert!(
            timer.contains("OnCalendar=Mon,Wed,Fri *-*-* 08,17:00,30:00"),
            "{timer}"
        );
    }

    #[test]
    fn the_timer_orders_weekdays_the_way_systemd_does() {
        // cron counts Sun…Sat, systemd counts Mon…Sun; `0,6` is a contiguous
        // Sat..Sun weekend only in systemd's order.
        let timer = systemd_timer("0 9 * * 0,6").expect("installable");
        assert!(
            timer.contains("OnCalendar=Sat,Sun *-*-* 09:00:00"),
            "{timer}"
        );
    }

    #[test]
    fn an_every_day_timer_omits_the_weekday_prefix() {
        let timer = systemd_timer("15 6 * * *").expect("installable");
        assert!(timer.contains("OnCalendar=*-*-* 06:15:00"), "{timer}");
    }

    #[test]
    fn an_unexpressible_cron_never_reaches_the_timer() {
        assert!(matches!(
            systemd_timer("0 9 1 * *"),
            Err(InstallError::Unsupported { .. })
        ));
        assert!(matches!(
            systemd_timer("0 7-19 * *"),
            Err(InstallError::Cron { .. })
        ));
    }

    // ── Task Scheduler ────────────────────────────────────────────────────

    #[test]
    fn the_task_run_value_quotes_the_executable() {
        // `C:\Program Files\…` is the normal install location; unquoted, Task
        // Scheduler would run `C:\Program` with `Files\…` as an argument.
        assert_eq!(
            task_run_value(Path::new(r"C:\Program Files\autostand\autostand.exe")),
            r#""C:\Program Files\autostand\autostand.exe" --compile"#
        );
    }

    #[test]
    fn the_default_schedule_becomes_a_weekly_task_with_a_repetition() {
        let args =
            schtasks_args(HOURLY_WEEKDAYS, Path::new(r"C:\autostand.exe")).expect("installable");
        assert_eq!(
            args,
            vec![
                "/Create",
                "/F",
                "/TN",
                "autostand",
                "/TR",
                r#""C:\autostand.exe" --compile"#,
                "/SC",
                "WEEKLY",
                "/D",
                "MON,TUE,WED,THU,FRI",
                "/ST",
                "07:00",
                "/RI",
                "60",
                "/DU",
                "12:00",
            ]
        );
    }

    #[test]
    fn a_single_daily_time_needs_no_repetition() {
        let args =
            schtasks_args("30 9 * * *", Path::new(r"C:\autostand.exe")).expect("installable");
        assert!(args.contains(&"DAILY".to_string()));
        assert!(args.contains(&"09:30".to_string()));
        assert!(!args.contains(&"/RI".to_string()), "{args:?}");
        assert!(!args.contains(&"/D".to_string()), "{args:?}");
    }

    #[test]
    fn the_rendered_command_line_quotes_what_a_shell_would_split() {
        let line = schtasks_command(
            HOURLY_WEEKDAYS,
            Path::new(r"C:\Program Files\autostand.exe"),
        )
        .expect("installable");
        assert!(line.starts_with("schtasks /Create /F /TN autostand /TR "));
        assert!(
            line.contains(r#""\"C:\Program Files\autostand.exe\" --compile""#),
            "{line}"
        );
    }

    #[test]
    fn task_scheduler_refuses_several_start_minutes() {
        // `/ST` is a single HH:MM, so `0,30 9 * * *` would install a task that
        // fires at 09:00 only — half the schedule, silently.
        let err = schtasks_args("0,30 9 * * *", Path::new(r"C:\autostand.exe"))
            .expect_err("two start minutes");
        match err {
            InstallError::Unsupported { target, reason, .. } => {
                assert_eq!(target, "Task Scheduler");
                assert!(reason.contains("start minute"), "{reason}");
            }
            other => panic!("expected an Unsupported error, got {other}"),
        }
    }

    #[test]
    fn task_scheduler_refuses_unevenly_spaced_hours() {
        let err = schtasks_args("0 8,9,17 * * 1-5", Path::new(r"C:\autostand.exe"))
            .expect_err("uneven hours");
        assert!(matches!(err, InstallError::Unsupported { .. }), "{err}");
    }

    #[test]
    fn an_unexpressible_cron_never_reaches_schtasks() {
        assert!(matches!(
            schtasks_args("0 9 1 * *", Path::new(r"C:\autostand.exe")),
            Err(InstallError::Unsupported { .. })
        ));
        assert!(matches!(
            schtasks_args("nope", Path::new(r"C:\autostand.exe")),
            Err(InstallError::Cron { .. })
        ));
    }

    // ── guards ────────────────────────────────────────────────────────────

    #[test]
    fn a_unit_test_may_never_touch_the_users_scheduled_jobs() {
        assert!(!may_touch_scheduled_jobs());
    }

    #[test]
    fn install_and_uninstall_refuse_from_a_test_binary() {
        // The point of the whole module: running the suite must not add a
        // LaunchAgent to the developer's login items.
        assert!(matches!(
            install(HOURLY_WEEKDAYS, &exe()),
            Err(InstallError::Sandboxed | InstallError::UnsupportedPlatform)
        ));
        assert!(matches!(
            uninstall(),
            Err(InstallError::Sandboxed | InstallError::UnsupportedPlatform)
        ));
    }

    #[test]
    fn cargo_test_harness_binaries_are_recognised() {
        assert!(is_cargo_test_binary(Path::new(
            "/repo/target/debug/deps/autostand_scheduler-1a2b3c"
        )));
        assert!(is_cargo_test_binary(Path::new(
            "/repo/target/release/deps/pipeline_e2e-9f8e"
        )));
        // A real run, and a real install, must not be mistaken for one.
        assert!(!is_cargo_test_binary(Path::new(
            "/repo/target/debug/autostand-app"
        )));
        assert!(!is_cargo_test_binary(Path::new(
            "/Applications/autostand.app/Contents/MacOS/autostand"
        )));
    }

    // ── detection ─────────────────────────────────────────────────────────

    #[test]
    fn detect_is_read_only_and_never_reports_the_in_process_tick() {
        // Whatever this machine has, detection must not invent `InProcess` — the
        // app decides that from its own runtime, not from the OS.
        let kind = detect();
        assert_ne!(kind, SchedulerKind::InProcess);
        assert!(matches!(
            kind,
            SchedulerKind::Launchd
                | SchedulerKind::Systemd
                | SchedulerKind::TaskScheduler
                | SchedulerKind::None
        ));
    }

    #[test]
    fn kind_wire_labels_match_the_ipc_contract() {
        assert_eq!(SchedulerKind::Launchd.wire_label(), "launchd");
        assert_eq!(SchedulerKind::Systemd.wire_label(), "systemd");
        assert_eq!(SchedulerKind::TaskScheduler.wire_label(), "task-scheduler");
        assert_eq!(SchedulerKind::InProcess.wire_label(), "in-process");
        assert_eq!(SchedulerKind::None.wire_label(), "none");
    }
}
