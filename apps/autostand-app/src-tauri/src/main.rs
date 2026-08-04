//! autostand's two entry points.
//!
//! With no arguments this is the Tauri desktop app. With `--compile` it is the
//! headless standup compiler a `launchd` agent, a `systemd --user` timer or a
//! Windows Task Scheduler task runs: one pipeline run, a human-readable summary
//! on stdout, and an exit code the scheduler can log. Both paths go through the
//! same `pipeline_runner`; see `docs/architecture/04-state-machine.md`
//! § Scheduler source.

// Prevents additional console window on Windows in release.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![forbid(unsafe_code)]

use std::process::ExitCode;

use autostand_app::commands::types::{CompileResult, CompileStatus};
use autostand_app::pipeline_runner;
use autostand_app::state::AppState;
use autostand_app::AppError;
use autostand_scheduler::triggers::{self, TriggerSource};
use chrono::NaiveDate;

/// Date format of the `--date` argument; the same one every IPC DTO uses.
const DATE_FORMAT: &str = "%Y-%m-%d";

/// Exit code for a run in which every target compiled or was skipped.
const EXIT_OK: u8 = 0;
/// Exit code for a run in which at least one target failed to compile.
const EXIT_COMPILE_FAILED: u8 = 1;
/// Exit code for a run that never started (lock held, config unreadable,
/// spawned from inside a render subprocess).
const EXIT_RUN_FAILED: u8 = 2;
/// Exit code for a bad command line. `64` is `sysexits.h`'s `EX_USAGE`.
const EXIT_USAGE: u8 = 64;

/// What the command line asked for.
#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// No arguments: launch the desktop app.
    Gui,
    /// `--help` / `-h`.
    Help,
    /// `--version` / `-V`.
    Version,
    /// `--compile [--date YYYY-MM-DD]`.
    Compile {
        /// Explicit filing date; `None` compiles `F_TODAY` + the self-heal slot.
        date: Option<String>,
    },
}

fn main() -> ExitCode {
    match parse_args(std::env::args().skip(1)) {
        Ok(Command::Gui) => {
            autostand_app::run();
            ExitCode::from(EXIT_OK)
        }
        Ok(Command::Help) => {
            println!("{}", help_text());
            ExitCode::from(EXIT_OK)
        }
        Ok(Command::Version) => {
            println!("autostand {}", env!("CARGO_PKG_VERSION"));
            ExitCode::from(EXIT_OK)
        }
        Ok(Command::Compile { date }) => ExitCode::from(compile(date.as_deref())),
        Err(message) => {
            eprintln!("autostand: {message}");
            eprintln!("{}", help_text());
            ExitCode::from(EXIT_USAGE)
        }
    }
}

/// Parse the command line.
///
/// Hand-rolled rather than pulled from a crate: the surface is three flags, and
/// the binary is a GUI app whose dependency tree is already large. `--help`
/// wins wherever it appears so that `--compile --help` explains itself instead
/// of compiling a standup.
fn parse_args<I: Iterator<Item = String>>(args: I) -> Result<Command, String> {
    let args: Vec<String> = args.collect();
    if args.iter().any(|arg| arg == "--help" || arg == "-h") {
        return Ok(Command::Help);
    }
    if args.iter().any(|arg| arg == "--version" || arg == "-V") {
        return Ok(Command::Version);
    }
    if args.is_empty() {
        return Ok(Command::Gui);
    }

    let mut compile = false;
    let mut date: Option<String> = None;
    let mut rest = args.iter().peekable();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--compile" => compile = true,
            "--date" => {
                let value = rest
                    .next()
                    .ok_or_else(|| "--date needs a YYYY-MM-DD argument".to_string())?;
                date = Some(value.clone());
            }
            other => {
                if let Some(value) = other.strip_prefix("--date=") {
                    date = Some(value.to_string());
                } else {
                    return Err(format!("unknown argument '{other}'"));
                }
            }
        }
    }
    if !compile {
        return Err("--date only makes sense together with --compile".to_string());
    }
    Ok(Command::Compile { date })
}

/// `--help` output, including the exit codes a scheduler unit will see.
fn help_text() -> String {
    format!(
        "autostand {version} — daily standup automation

USAGE
  autostand-app                          launch the desktop app
  autostand-app --compile                compile F_TODAY (+ the self-heal slot)
  autostand-app --compile --date <DATE>  compile exactly <DATE> (YYYY-MM-DD)
  autostand-app --help | --version

The --compile form is what an installed scheduler unit (launchd, systemd --user,
Task Scheduler) runs. It needs no window and no display.

EXIT CODES
  {EXIT_OK}   every target compiled, or was deliberately skipped
  {EXIT_COMPILE_FAILED}   at least one target failed to compile
  {EXIT_RUN_FAILED}   the run never started (another compile holds the lock,
      unreadable config, or spawned from inside a render subprocess)
  {EXIT_USAGE}  bad command line",
        version = env!("CARGO_PKG_VERSION"),
    )
}

/// Run the pipeline once with no window, print what happened, return the code.
fn compile(date: Option<&str>) -> u8 {
    init_tracing();

    // An AI coding CLI spawned by a render emits the very session-end event that
    // triggers autostand; firing here is how the original App Script's infinite
    // render loop used to start.
    if !triggers::safe_to_fire() {
        eprintln!("autostand: refusing to compile from inside a render subprocess");
        return EXIT_RUN_FAILED;
    }

    let only_date = match date.map(parse_date).transpose() {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("autostand: {message}");
            return EXIT_USAGE;
        }
    };

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(err) => {
            eprintln!("autostand: could not start the async runtime: {err}");
            return EXIT_RUN_FAILED;
        }
    };
    let state = AppState::new();
    let outcome = runtime.block_on(pipeline_runner::trigger_headless(
        &state,
        TriggerSource::Scheduler,
        only_date,
    ));
    report(outcome.as_deref())
}

/// Parse a `--date` argument, refusing anything the pipeline would misfile.
fn parse_date(raw: &str) -> Result<NaiveDate, String> {
    NaiveDate::parse_from_str(raw, DATE_FORMAT)
        .map_err(|err| format!("invalid --date '{raw}': {err} (expected {DATE_FORMAT})"))
}

/// Print one line per compiled target and pick the exit code.
fn report(outcome: Result<&[CompileResult], &AppError>) -> u8 {
    match outcome {
        Ok(results) => {
            for result in results {
                println!("{}", summarize(result));
            }
            if results.iter().any(|r| r.status == CompileStatus::Error) {
                EXIT_COMPILE_FAILED
            } else {
                EXIT_OK
            }
        }
        Err(err) => {
            eprintln!("autostand: {err}");
            EXIT_RUN_FAILED
        }
    }
}

/// One target's outcome as a single human-readable line.
fn summarize(result: &CompileResult) -> String {
    let status = match result.status {
        CompileStatus::Ok => "ok",
        CompileStatus::Skip => "skip",
        CompileStatus::Error => "error",
    };
    let path = if result.file_path.is_empty() {
        "-"
    } else {
        result.file_path.as_str()
    };
    format!(
        "{date}  {status:<5} {host}  {path}  ({message})",
        date = result.date,
        host = result.host,
        message = result.message,
    )
}

/// Install a tracing subscriber for the headless path.
///
/// The GUI path does this inside `autostand_app::run`; a scheduled run needs it
/// too, because its only debugging surface is whatever the unit captured.
fn init_tracing() {
    use tracing_subscriber::EnvFilter;

    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    // A second init would panic; the headless path owns the process, but stay
    // defensive so a future caller cannot turn a logging detail into a crash.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::{
        help_text, parse_args, parse_date, report, summarize, Command, DATE_FORMAT,
        EXIT_COMPILE_FAILED, EXIT_OK, EXIT_RUN_FAILED,
    };
    use autostand_app::commands::types::{CompileResult, CompileStatus};
    use autostand_app::pipeline_runner::{base_result, error_result, skip_result};
    use autostand_app::AppError;
    use chrono::NaiveDate;
    use std::path::Path;

    /// Turn a `&str` slice into the owned iterator `parse_args` takes.
    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|s| (*s).to_string()))
    }

    fn date(year: i32, month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(year, month, day).expect("valid date")
    }

    #[test]
    fn no_arguments_launches_the_desktop_app() {
        assert_eq!(parse(&[]), Ok(Command::Gui));
    }

    #[test]
    fn compile_without_a_date_targets_the_default_pair() {
        assert_eq!(parse(&["--compile"]), Ok(Command::Compile { date: None }));
    }

    #[test]
    fn compile_accepts_a_date_in_both_spellings() {
        let expected = Ok(Command::Compile {
            date: Some("2026-08-03".to_string()),
        });
        assert_eq!(parse(&["--compile", "--date", "2026-08-03"]), expected);
        assert_eq!(parse(&["--compile", "--date=2026-08-03"]), expected);
        // Order must not matter: a unit's argv is written by hand.
        assert_eq!(parse(&["--date=2026-08-03", "--compile"]), expected);
    }

    #[test]
    fn help_wins_wherever_it_appears() {
        // `--compile --help` must explain itself, not compile a standup.
        for args in [
            vec!["--help"],
            vec!["-h"],
            vec!["--compile", "--help"],
            vec!["--help", "--compile"],
        ] {
            assert_eq!(parse(&args), Ok(Command::Help), "{args:?}");
        }
    }

    #[test]
    fn version_is_recognised() {
        assert_eq!(parse(&["--version"]), Ok(Command::Version));
        assert_eq!(parse(&["-V"]), Ok(Command::Version));
    }

    #[test]
    fn unknown_arguments_are_refused() {
        for args in [vec!["--run"], vec!["--compile", "--force"], vec!["compile"]] {
            let err = parse(&args).expect_err("must be refused");
            assert!(err.contains("unknown argument"), "{args:?} → {err}");
        }
    }

    #[test]
    fn a_dangling_date_flag_is_refused() {
        let err = parse(&["--compile", "--date"]).expect_err("no value");
        assert!(err.contains("YYYY-MM-DD"), "{err}");
    }

    #[test]
    fn a_date_without_compile_is_refused() {
        // Nothing to apply it to; running the GUI and ignoring it would be worse.
        let err = parse(&["--date", "2026-08-03"]).expect_err("no --compile");
        assert!(err.contains("--compile"), "{err}");
    }

    #[test]
    fn dates_are_parsed_strictly() {
        assert_eq!(parse_date("2026-08-03"), Ok(date(2026, 8, 3)));
        for bad in ["03-08-2026", "2026/08/03", "2026-13-01", "today", ""] {
            let err = parse_date(bad).expect_err("must be refused");
            assert!(err.contains(bad) && err.contains(DATE_FORMAT), "{err}");
        }
    }

    #[test]
    fn a_clean_run_exits_zero_and_a_failed_target_exits_one() {
        let path = Path::new("/dailies/2026-08-04.md");
        let ok = base_result(date(2026, 8, 4), "host", path);
        let skipped = skip_result(date(2026, 8, 3), "host", path, "frozen");
        assert_eq!(report(Ok(&[ok.clone(), skipped.clone()])), EXIT_OK);

        let failed = error_result(date(2026, 8, 3), "host", "disk full");
        assert_eq!(report(Ok(&[ok, skipped, failed])), EXIT_COMPILE_FAILED);
    }

    #[test]
    fn an_empty_result_set_is_not_a_failure() {
        assert_eq!(report(Ok(&[])), EXIT_OK);
    }

    #[test]
    fn a_run_that_never_started_exits_two() {
        // A busy lock is the expected outcome when a manual run overlaps a cron
        // boundary, and the scheduler needs to tell it apart from a bad compile.
        let busy = AppError::Lock("another compile is already running".into());
        assert_eq!(report(Err(&busy)), EXIT_RUN_FAILED);
    }

    #[test]
    fn each_target_is_summarized_on_one_line() {
        let mut result = base_result(
            date(2026, 8, 4),
            "MacStudio-de-Miguel",
            Path::new("/dailies/2026-08-04.md"),
        );
        result.message = "deterministic render".to_string();
        let line = summarize(&result);
        assert!(!line.contains('\n'), "{line}");
        for expected in [
            "2026-08-04",
            "ok",
            "MacStudio-de-Miguel",
            "/dailies/2026-08-04.md",
            "deterministic render",
        ] {
            assert!(line.contains(expected), "{expected} missing from {line}");
        }
    }

    #[test]
    fn an_errored_target_reports_a_placeholder_path() {
        // `error_result` leaves `file_path` empty because nothing was written;
        // printing a bare double space would read as a truncated line.
        let failed = error_result(date(2026, 8, 3), "host", "disk full");
        assert_eq!(failed.status, CompileStatus::Error);
        let line = summarize(&failed);
        assert!(line.contains(" -  "), "{line}");
        assert!(line.contains("disk full"), "{line}");
    }

    #[test]
    fn the_help_text_documents_every_exit_code_and_the_compile_flag() {
        let help = help_text();
        for expected in [
            "--compile",
            "--date",
            "launchd",
            "systemd",
            "Task Scheduler",
            "EXIT CODES",
        ] {
            assert!(help.contains(expected), "{expected} missing from help");
        }
    }

    /// Guard: the summary reads every `CompileStatus`, so a new variant that
    /// forgets a label fails to compile rather than printing something odd.
    #[test]
    fn every_compile_status_has_a_label() {
        let path = Path::new("/dailies/2026-08-04.md");
        let statuses = [CompileStatus::Ok, CompileStatus::Skip, CompileStatus::Error];
        for status in statuses {
            let result = CompileResult {
                status,
                ..base_result(date(2026, 8, 4), "host", path)
            };
            assert!(!summarize(&result).is_empty());
        }
    }
}
