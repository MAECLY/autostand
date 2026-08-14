//! The single process spawner of the workspace.
//!
//! Nothing else may call `std::process::Command` or `tokio::process::Command`.
//! Routing every child through here buys three things at once:
//!
//! 1. **Visibility.** Every spawn logs to the current [`RunSink`](crate::RunSink),
//!    so the Terminal panel cannot miss an action.
//! 2. **Privacy in one place.** A git/gh argv carries repository paths and
//!    branch names; an LLM CLI's stdout *is* the standup body; and
//!    `docs/specs/audit.md` forbids stderr and API bodies in telemetry. The
//!    redaction rules live here instead of at thirty call sites.
//! 3. **One timeout/kill policy.** Every child is `kill_on_drop`, and a timeout
//!    always kills rather than leaking a process.
//!
//! # What ends up in the Terminal
//!
//! | Policy | Start line | End line | stdout | stderr |
//! |---|---|---|---|---|
//! | [`StreamPolicy::Silent`] | — | — | never | never |
//! | [`StreamPolicy::Summary`] | display name | exit code + duration + byte count | never | never |
//! | [`StreamPolicy::Lines`] | display name | exit code + duration + byte count | line by line | never |
//!
//! The **display name** is [`ProcSpec::label`] when the caller set one, else the
//! program's file name (`/opt/homebrew/bin/git` → `git`). The argv is *never*
//! rendered, not even partially: no heuristic can tell a safe subcommand from a
//! customer's branch name, so the caller states what is safe to show.
//!
//! [`StreamPolicy::Lines`] is opt-in per call site and must never be used for an
//! LLM render or for the local sidecar: that stdout is the user's standup.

use std::borrow::Cow;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};

use tokio::io::{AsyncBufReadExt as _, AsyncWriteExt as _, BufReader, Lines};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

use crate::{log, LogLine};

/// Longest stdout line echoed under [`StreamPolicy::Lines`]; the rest is elided.
const MAX_LINE_CHARS: usize = 200;

/// Most stdout lines echoed under [`StreamPolicy::Lines`] per process.
///
/// A chatty child would otherwise push the whole run out of the viewer's
/// 500-line ring buffer.
const MAX_STREAMED_LINES: usize = 200;

/// How much of a child's output reaches the Terminal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StreamPolicy {
    /// Log nothing at all. For probes so frequent that logging them is noise
    /// (CLI detection sweeps, `kill -0` liveness checks).
    Silent,
    /// Log that the process ran and how it ended — never its argv or output.
    /// The default, and the right answer for git, gh and every LLM call.
    #[default]
    Summary,
    /// Everything `Summary` logs, plus each stdout line.
    ///
    /// Only for children whose stdout is known to be non-sensitive progress
    /// text (package managers, model downloads). Never for a render.
    Lines,
}

/// Everything needed to start one child process.
///
/// Build with [`ProcSpec::new`] plus the builder methods; the fields are public
/// so a caller may also construct it literally.
#[derive(Debug, Clone)]
pub struct ProcSpec {
    /// Executable to run. An absolute path is preferred — `PATH` lookups are a
    /// detection concern, not a spawning one.
    pub program: PathBuf,
    /// Arguments, verbatim. Never logged.
    pub args: Vec<OsString>,
    /// Working directory, or the parent's when `None`. Never logged.
    pub cwd: Option<PathBuf>,
    /// Extra environment entries merged over the parent's. Never logged —
    /// tokens and API keys travel this way.
    pub env: Vec<(OsString, OsString)>,
    /// Bytes to feed the child's stdin, then EOF. Never logged: this is where
    /// the render prompt goes.
    pub stdin: Option<Vec<u8>>,
    /// Wall-clock budget. `None` means "wait forever", which is only correct
    /// for a child the caller drives itself.
    pub timeout: Option<Duration>,
    /// Step label the log lines are filed under (`gather`, `render_llm`, …).
    pub step: Cow<'static, str>,
    /// How much of the child's output reaches the Terminal.
    pub stream: StreamPolicy,
    /// Human-readable name shown instead of the program's file name.
    ///
    /// The caller owns this string because only the caller knows what is safe
    /// to show: `"git pull"` is fine, `"git pull /Users/me/work/acme"` is not.
    /// Must never contain a path, a branch name or a token.
    pub label: Option<Cow<'static, str>>,
}

impl ProcSpec {
    /// Spec for `program`, filed under `step`, with the default
    /// [`StreamPolicy::Summary`].
    pub fn new(program: impl Into<PathBuf>, step: impl Into<Cow<'static, str>>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
            stdin: None,
            timeout: None,
            step: step.into(),
            stream: StreamPolicy::Summary,
            label: None,
        }
    }

    /// Append one argument.
    #[must_use]
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// Append several arguments.
    #[must_use]
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|a| a.as_ref().to_os_string()));
        self
    }

    /// Run the child in `dir`.
    #[must_use]
    pub fn cwd(mut self, dir: impl Into<PathBuf>) -> Self {
        self.cwd = Some(dir.into());
        self
    }

    /// Set one environment entry.
    #[must_use]
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.env
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    /// Feed `data` to the child's stdin, then close it.
    #[must_use]
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }

    /// Set the wall-clock budget.
    #[must_use]
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Set the wall-clock budget in seconds.
    #[must_use]
    pub fn timeout_secs(self, secs: u64) -> Self {
        self.timeout(Duration::from_secs(secs))
    }

    /// Choose how much of the child's output reaches the Terminal.
    #[must_use]
    pub fn stream(mut self, stream: StreamPolicy) -> Self {
        self.stream = stream;
        self
    }

    /// Set the human-readable display name. Must contain no path or secret.
    #[must_use]
    pub fn label(mut self, label: impl Into<Cow<'static, str>>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// What the Terminal shows for this process.
    ///
    /// The label when set, else the program's **file name** — never the full
    /// path, which on macOS routinely contains the user's home directory.
    pub fn display_name(&self) -> String {
        if let Some(label) = &self.label {
            return label.as_ref().to_string();
        }
        program_file_name(&self.program)
    }
}

/// File name of `program`, or `"process"` when it has none.
fn program_file_name(program: &Path) -> String {
    program.file_name().map_or_else(
        || "process".to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Result of a finished child.
///
/// `stdout` / `stderr` are returned to the caller for its own parsing and error
/// mapping; this crate never logs either of them.
#[derive(Debug, Clone)]
pub struct ProcOutput {
    /// Exit code, or `None` when the child was killed by a signal.
    pub code: Option<i32>,
    /// Whether the child exited zero.
    pub success: bool,
    /// Captured stdout, lossily decoded as UTF-8.
    ///
    /// Line endings are normalized to `\n` and a trailing newline is always
    /// present when the child produced any output at all — every caller trims.
    pub stdout: String,
    /// Captured stderr, lossily decoded as UTF-8. Empty for a piped child.
    pub stderr: String,
    /// Wall-clock duration.
    pub duration: Duration,
}

impl ProcOutput {
    /// `stdout` with surrounding whitespace removed.
    pub fn stdout_trimmed(&self) -> &str {
        self.stdout.trim()
    }

    /// `stderr` with surrounding whitespace removed.
    pub fn stderr_trimmed(&self) -> &str {
        self.stderr.trim()
    }
}

/// Why a child could not be run to completion.
///
/// A non-zero exit is **not** an error: it is a [`ProcOutput`] with
/// `success == false`, because most callers treat "git said no" as data.
#[derive(Debug, thiserror::Error)]
pub enum ProcError {
    /// The executable could not be started (missing, not executable, …).
    #[error("{program}: could not start ({kind})")]
    Spawn {
        /// Display name of the program.
        program: String,
        /// Kind of the underlying IO error. The message is deliberately not
        /// carried through: it can embed the resolved path.
        kind: std::io::ErrorKind,
    },
    /// The child outlived its budget and was killed.
    #[error("{program}: timed out after {secs}s")]
    Timeout {
        /// Display name of the program.
        program: String,
        /// The budget that elapsed.
        secs: u64,
    },
    /// Reading from or writing to the child failed.
    #[error("{program}: io error ({kind})")]
    Io {
        /// Display name of the program.
        program: String,
        /// Kind of the underlying IO error.
        kind: std::io::ErrorKind,
    },
}

impl ProcError {
    /// Display name of the program that failed.
    pub fn program(&self) -> &str {
        match self {
            Self::Spawn { program, .. }
            | Self::Timeout { program, .. }
            | Self::Io { program, .. } => program,
        }
    }
}

/// Build the `tokio` command for `spec` without spawning it.
fn command_for(spec: &ProcSpec, stdin: Stdio, stderr: Stdio) -> Command {
    let mut command = Command::new(&spec.program);
    command
        .args(&spec.args)
        .kill_on_drop(true)
        .stdin(stdin)
        .stdout(Stdio::piped())
        .stderr(stderr);
    if let Some(dir) = &spec.cwd {
        command.current_dir(dir);
    }
    for (key, value) in &spec.env {
        command.env(key, value);
    }
    command
}

/// Emit the "process started" line, unless the policy is silent.
fn log_start(spec: &ProcSpec, display: &str) {
    if spec.stream == StreamPolicy::Silent {
        return;
    }
    let line = LogLine::info(spec.step.as_ref(), display.to_string());
    // Arg *count* is safe; arg *content* is not.
    log(if spec.args.is_empty() {
        line
    } else {
        line.with_detail(format!("{} arg(s)", spec.args.len()))
    });
}

/// Emit the "process finished" line, unless the policy is silent.
fn log_end(spec: &ProcSpec, display: &str, output: &ProcOutput) {
    if spec.stream == StreamPolicy::Silent {
        return;
    }
    let millis = duration_millis(output.duration);
    if output.success {
        log(LogLine::done(spec.step.as_ref(), format!("{display} ok"))
            .with_detail(format!("{millis} ms, {} B", output.stdout.len())));
        return;
    }
    let ending = match output.code {
        Some(code) => format!("{display} exited {code}"),
        None => format!("{display} was terminated"),
    };
    // Warn, not error: a non-zero exit is routinely an expected answer (`git
    // rev-parse` on a non-repo). The caller decides whether it is fatal.
    log(LogLine::warn(spec.step.as_ref(), ending).with_detail(format!("{millis} ms")));
}

/// Milliseconds of `duration`, saturating.
fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}

/// Shorten one streamed stdout line to [`MAX_LINE_CHARS`].
fn clip_line(line: &str) -> String {
    let trimmed = line.trim_end();
    if trimmed.chars().count() <= MAX_LINE_CHARS {
        return trimmed.to_string();
    }
    let mut clipped: String = trimmed
        .chars()
        .take(MAX_LINE_CHARS.saturating_sub(1))
        .collect();
    clipped.push('…');
    clipped
}

/// Run `spec` to completion and return what it produced.
///
/// Never returns `Err` for a non-zero exit — see [`ProcError`]. A timeout kills
/// the child (`kill_on_drop`) before returning.
pub async fn run_process(spec: ProcSpec) -> Result<ProcOutput, ProcError> {
    let display = spec.display_name();
    let stdin_mode = if spec.stdin.is_some() {
        Stdio::piped()
    } else {
        Stdio::null()
    };
    let mut command = command_for(&spec, stdin_mode, Stdio::piped());

    log_start(&spec, &display);

    let started = Instant::now();
    let mut child = command.spawn().map_err(|err| {
        let error = ProcError::Spawn {
            program: display.clone(),
            kind: err.kind(),
        };
        if spec.stream != StreamPolicy::Silent {
            log(LogLine::error(
                spec.step.as_ref(),
                format!("{display} could not start"),
            ));
        }
        error
    })?;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let payload = spec.stdin.clone();

    // stdin, stdout and stderr are driven concurrently on purpose: a prompt
    // larger than the pipe buffer deadlocks if the writer waits for the reader.
    let pump = async {
        let write = async {
            if let Some(mut handle) = stdin {
                if let Some(bytes) = payload {
                    handle.write_all(&bytes).await?;
                }
                handle.shutdown().await?;
            }
            Ok::<(), std::io::Error>(())
        };
        let read_out = read_stdout(stdout, &spec);
        let read_err = async {
            let mut buffer = Vec::new();
            if let Some(mut handle) = stderr {
                tokio::io::AsyncReadExt::read_to_end(&mut handle, &mut buffer).await?;
            }
            Ok::<Vec<u8>, std::io::Error>(buffer)
        };
        let (write, out, err) = tokio::join!(write, read_out, read_err);
        write?;
        let status = child.wait().await?;
        Ok::<_, std::io::Error>((status, out?, err?))
    };

    let finished = match spec.timeout {
        Some(budget) => {
            let Ok(result) = tokio::time::timeout(budget, pump).await else {
                let secs = budget.as_secs();
                if spec.stream != StreamPolicy::Silent {
                    log(LogLine::error(
                        spec.step.as_ref(),
                        format!("{display} timed out after {secs}s"),
                    ));
                }
                return Err(ProcError::Timeout {
                    program: display,
                    secs,
                });
            };
            result
        }
        None => pump.await,
    };

    let (status, stdout, stderr) = finished.map_err(|err| {
        if spec.stream != StreamPolicy::Silent {
            log(LogLine::error(
                spec.step.as_ref(),
                format!("{display} failed"),
            ));
        }
        ProcError::Io {
            program: display.clone(),
            kind: err.kind(),
        }
    })?;

    let output = ProcOutput {
        code: status.code(),
        success: status.success(),
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        duration: started.elapsed(),
    };
    log_end(&spec, &display, &output);
    Ok(output)
}

/// Drain the child's stdout, echoing lines when the policy asks for it.
async fn read_stdout(
    stdout: Option<ChildStdout>,
    spec: &ProcSpec,
) -> Result<String, std::io::Error> {
    let mut collected = String::new();
    let Some(handle) = stdout else {
        return Ok(collected);
    };
    let mut lines = BufReader::new(handle).lines();
    let mut echoed = 0usize;
    while let Some(line) = lines.next_line().await? {
        if spec.stream == StreamPolicy::Lines {
            match echoed.cmp(&MAX_STREAMED_LINES) {
                std::cmp::Ordering::Less => {
                    log(LogLine::info(spec.step.as_ref(), clip_line(&line)));
                }
                std::cmp::Ordering::Equal => log(LogLine::warn(
                    spec.step.as_ref(),
                    "output truncated in the panel",
                )),
                std::cmp::Ordering::Greater => {}
            }
            echoed += 1;
        }
        collected.push_str(&line);
        collected.push('\n');
    }
    Ok(collected)
}

/// A child the caller drives itself, one line at a time.
///
/// For protocol children like the local GGUF sidecar: write a request line,
/// read a response line. The child's stdout is **not** echoed under
/// [`StreamPolicy::Summary`] — for the sidecar that stdout is the standup body.
#[derive(Debug)]
pub struct PipedChild {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: Option<Lines<BufReader<ChildStdout>>>,
    display: String,
    step: Cow<'static, str>,
    stream: StreamPolicy,
    started: Instant,
    deadline: Option<Instant>,
    budget_secs: u64,
}

/// Start `spec` and hand the caller the pipes.
///
/// stderr is discarded rather than piped: nobody would read it, and a full
/// stderr pipe deadlocks a long-lived protocol child. `spec.stdin` is ignored —
/// a piped child is fed with [`PipedChild::write_line`].
pub fn run_process_piped(spec: &ProcSpec) -> Result<PipedChild, ProcError> {
    let display = spec.display_name();
    let mut command = command_for(spec, Stdio::piped(), Stdio::null());

    log_start(spec, &display);

    let started = Instant::now();
    let mut child = command.spawn().map_err(|err| {
        if spec.stream != StreamPolicy::Silent {
            log(LogLine::error(
                spec.step.as_ref(),
                format!("{display} could not start"),
            ));
        }
        ProcError::Spawn {
            program: display.clone(),
            kind: err.kind(),
        }
    })?;

    let stdin = child.stdin.take();
    let stdout = child.stdout.take().map(|out| BufReader::new(out).lines());
    Ok(PipedChild {
        child,
        stdin,
        stdout,
        display,
        step: spec.step.clone(),
        stream: spec.stream,
        started,
        deadline: spec.timeout.map(|budget| started + budget),
        budget_secs: spec.timeout.map_or(0, |budget| budget.as_secs()),
    })
}

impl PipedChild {
    /// Display name of the child, for the caller's own log lines.
    pub fn display_name(&self) -> &str {
        &self.display
    }

    /// Write `line` plus a newline to the child's stdin.
    ///
    /// The payload is never logged: for the sidecar it is the render prompt.
    pub async fn write_line(&mut self, line: &str) -> Result<(), ProcError> {
        let handle = self.stdin.as_mut().ok_or_else(|| ProcError::Io {
            program: self.display.clone(),
            kind: std::io::ErrorKind::BrokenPipe,
        })?;
        handle
            .write_all(line.as_bytes())
            .await
            .and(handle.write_all(b"\n").await)
            .and(handle.flush().await)
            .map_err(|err| ProcError::Io {
                program: self.display.clone(),
                kind: err.kind(),
            })
    }

    /// Close the child's stdin so it sees EOF.
    pub async fn close_stdin(&mut self) -> Result<(), ProcError> {
        if let Some(mut handle) = self.stdin.take() {
            handle.shutdown().await.map_err(|err| ProcError::Io {
                program: self.display.clone(),
                kind: err.kind(),
            })?;
        }
        Ok(())
    }

    /// Read the next stdout line, honouring the remaining budget.
    ///
    /// `Ok(None)` means the child closed its stdout.
    pub async fn next_line(&mut self) -> Result<Option<String>, ProcError> {
        let Some(lines) = self.stdout.as_mut() else {
            return Ok(None);
        };
        let read = lines.next_line();
        let line = match self.deadline {
            Some(deadline) => {
                let Ok(result) = tokio::time::timeout_at(deadline.into(), read).await else {
                    if self.stream != StreamPolicy::Silent {
                        log(LogLine::error(
                            self.step.as_ref(),
                            format!("{} timed out after {}s", self.display, self.budget_secs),
                        ));
                    }
                    return Err(ProcError::Timeout {
                        program: self.display.clone(),
                        secs: self.budget_secs,
                    });
                };
                result
            }
            None => read.await,
        }
        .map_err(|err| ProcError::Io {
            program: self.display.clone(),
            kind: err.kind(),
        })?;
        if let Some(text) = &line {
            if self.stream == StreamPolicy::Lines {
                log(LogLine::info(self.step.as_ref(), clip_line(text)));
            }
        }
        Ok(line)
    }

    /// Kill the child now. Idempotent.
    pub async fn kill(&mut self) {
        let _ = self.child.kill().await;
    }

    /// Wait for the child and emit the summary line.
    ///
    /// `stdout` on the returned [`ProcOutput`] is empty: a piped child's output
    /// was already consumed by [`PipedChild::next_line`].
    pub async fn finish(mut self) -> Result<ProcOutput, ProcError> {
        self.close_stdin().await?;
        let wait = self.child.wait();
        let status = match self.deadline {
            Some(deadline) => {
                let Ok(result) = tokio::time::timeout_at(deadline.into(), wait).await else {
                    let secs = self.budget_secs;
                    if self.stream != StreamPolicy::Silent {
                        log(LogLine::error(
                            self.step.as_ref(),
                            format!("{} timed out after {secs}s", self.display),
                        ));
                    }
                    return Err(ProcError::Timeout {
                        program: self.display.clone(),
                        secs,
                    });
                };
                result
            }
            None => wait.await,
        }
        .map_err(|err| ProcError::Io {
            program: self.display.clone(),
            kind: err.kind(),
        })?;

        let output = ProcOutput {
            code: status.code(),
            success: status.success(),
            stdout: String::new(),
            stderr: String::new(),
            duration: self.started.elapsed(),
        };
        if self.stream != StreamPolicy::Silent {
            let millis = duration_millis(output.duration);
            let line = if output.success {
                LogLine::done(self.step.as_ref(), format!("{} ok", self.display))
            } else {
                LogLine::warn(
                    self.step.as_ref(),
                    match output.code {
                        Some(code) => format!("{} exited {code}", self.display),
                        None => format!("{} was terminated", self.display),
                    },
                )
            };
            log(line.with_detail(format!("{millis} ms")));
        }
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        clip_line, program_file_name, run_process, run_process_piped, ProcError, ProcSpec,
        StreamPolicy, MAX_LINE_CHARS,
    };
    use crate::{scoped, LogLevel, RecordingSink, SinkRef};
    use std::path::Path;
    use std::sync::Arc;
    use std::time::Duration;

    fn recorder() -> (Arc<RecordingSink>, SinkRef) {
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();
        (sink, as_ref)
    }

    #[test]
    fn the_display_name_is_the_file_name_never_the_path() {
        assert_eq!(program_file_name(Path::new("/opt/homebrew/bin/git")), "git");
        assert_eq!(program_file_name(Path::new("git")), "git");
        assert_eq!(program_file_name(Path::new("/")), "process");
        let spec = ProcSpec::new("/Users/someone/private/tools/gh", "repo_sync");
        assert_eq!(spec.display_name(), "gh");
        assert_eq!(spec.label("gh repo view").display_name(), "gh repo view");
    }

    #[test]
    fn a_streamed_line_is_clipped_not_dumped() {
        assert_eq!(clip_line("short  "), "short");
        let long = "x".repeat(MAX_LINE_CHARS * 2);
        let clipped = clip_line(&long);
        assert_eq!(clipped.chars().count(), MAX_LINE_CHARS);
        assert!(clipped.ends_with('…'));
    }

    #[tokio::test]
    async fn summary_reports_the_run_without_leaking_argv_or_stdout() {
        let (sink, as_ref) = recorder();
        let output = scoped(as_ref, async {
            run_process(
                ProcSpec::new("/bin/echo", "gather")
                    .args(["/Users/someone/work/acme-private", "feat/ACME-42-secret"])
                    .label("git log")
                    .timeout_secs(10),
            )
            .await
            .expect("echo runs")
        })
        .await;

        assert!(output.success);
        assert!(output.stdout.contains("acme-private"), "stdout is returned");

        let rendered = sink
            .lines()
            .iter()
            .map(|line| {
                format!(
                    "{} {}",
                    line.message,
                    line.detail.clone().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(rendered.contains("git log"), "{rendered}");
        assert!(
            !rendered.contains("acme-private"),
            "argv leaked: {rendered}"
        );
        assert!(
            !rendered.contains("ACME-42"),
            "branch name leaked: {rendered}"
        );
        assert!(!rendered.contains("/bin/echo"), "path leaked: {rendered}");
        assert!(rendered.contains("2 arg(s)"), "{rendered}");
    }

    #[tokio::test]
    async fn silent_logs_absolutely_nothing() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            run_process(
                ProcSpec::new("/bin/echo", "gather")
                    .arg("hello")
                    .stream(StreamPolicy::Silent),
            )
            .await
            .expect("echo runs")
        })
        .await;
        assert!(sink.lines().is_empty());
    }

    #[tokio::test]
    async fn lines_echoes_stdout_when_the_caller_opted_in() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            run_process(
                ProcSpec::new("/bin/echo", "model_download")
                    .arg("downloading 10%")
                    .stream(StreamPolicy::Lines),
            )
            .await
            .expect("echo runs")
        })
        .await;
        assert!(sink.messages().iter().any(|m| m == "downloading 10%"));
    }

    #[tokio::test]
    async fn a_non_zero_exit_is_data_not_an_error() {
        let (sink, as_ref) = recorder();
        let output = scoped(as_ref, async {
            run_process(ProcSpec::new("/bin/sh", "gather").args(["-c", "exit 3"]))
                .await
                .expect("sh runs")
        })
        .await;
        assert!(!output.success);
        assert_eq!(output.code, Some(3));
        let last = sink.lines().pop().expect("an end line");
        assert_eq!(last.level, LogLevel::Warn);
        assert!(last.message.contains("exited 3"), "{}", last.message);
    }

    #[tokio::test]
    async fn stderr_never_reaches_the_terminal_but_does_reach_the_caller() {
        let (sink, as_ref) = recorder();
        let output = scoped(as_ref, async {
            run_process(
                ProcSpec::new("/bin/sh", "gather")
                    .args(["-c", "echo fatal-credential-hint 1>&2; exit 1"]),
            )
            .await
            .expect("sh runs")
        })
        .await;
        assert!(output.stderr.contains("fatal-credential-hint"));
        let rendered = sink.messages().join("\n");
        assert!(
            !rendered.contains("fatal-credential-hint"),
            "docs/specs/audit.md forbids stderr in telemetry: {rendered}"
        );
    }

    #[tokio::test]
    async fn stdin_is_delivered_and_never_logged() {
        let (sink, as_ref) = recorder();
        let output = scoped(as_ref, async {
            run_process(ProcSpec::new("/bin/cat", "render_llm").stdin("secret prompt body"))
                .await
                .expect("cat runs")
        })
        .await;
        assert_eq!(output.stdout_trimmed(), "secret prompt body");
        assert!(!sink.messages().join("\n").contains("secret prompt"));
    }

    #[tokio::test]
    async fn a_timeout_is_reported_and_the_child_is_killed() {
        let (sink, as_ref) = recorder();
        let started = std::time::Instant::now();
        let error = scoped(as_ref, async {
            run_process(
                ProcSpec::new("/bin/sleep", "gather")
                    .arg("30")
                    .label("sleep")
                    .timeout(Duration::from_millis(200)),
            )
            .await
            .expect_err("the budget elapses")
        })
        .await;
        assert!(matches!(error, ProcError::Timeout { .. }));
        assert!(started.elapsed() < Duration::from_secs(5));
        let last = sink.lines().pop().expect("a timeout line");
        assert_eq!(last.level, LogLevel::Error);
        assert!(last.message.contains("timed out"), "{}", last.message);
    }

    #[tokio::test]
    async fn a_missing_binary_is_a_spawn_error_with_no_path_in_it() {
        let (sink, as_ref) = recorder();
        let error = scoped(as_ref, async {
            run_process(ProcSpec::new(
                "/definitely/not/a/real/binary-xyz",
                "cli_detect",
            ))
            .await
            .expect_err("missing binary")
        })
        .await;
        assert!(matches!(error, ProcError::Spawn { .. }));
        assert_eq!(error.program(), "binary-xyz");
        assert!(!error.to_string().contains("/definitely/not"));
        assert!(sink
            .messages()
            .iter()
            .any(|m| m.contains("could not start")));
    }

    #[tokio::test]
    async fn a_piped_child_speaks_a_line_protocol_without_echoing_it() {
        let (sink, as_ref) = recorder();
        let echoed = scoped(as_ref, async {
            let mut child = run_process_piped(
                &ProcSpec::new("/bin/cat", "render_llm")
                    .label("local model")
                    .timeout_secs(10),
            )
            .expect("cat starts");
            child
                .write_line("{\"prompt\":\"the standup body\"}")
                .await
                .expect("write");
            let line = child.next_line().await.expect("read").expect("a line");
            child.close_stdin().await.expect("close");
            let output = child.finish().await.expect("exit");
            assert!(output.success);
            line
        })
        .await;
        assert_eq!(echoed, "{\"prompt\":\"the standup body\"}");
        let rendered = sink.messages().join("\n");
        assert!(rendered.contains("local model"), "{rendered}");
        assert!(
            !rendered.contains("standup body"),
            "an LLM body must never reach the panel: {rendered}"
        );
    }

    /// Regression guard for the trap that will break the migration: a spawner
    /// call inside a bare `tokio::spawn` logs nowhere.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_spawn_inside_a_bare_task_loses_the_terminal() {
        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            tokio::spawn(async {
                let _ = run_process(ProcSpec::new("/bin/echo", "gather").arg("hi")).await;
            })
            .await
            .unwrap();
        })
        .await;
        assert!(sink.lines().is_empty());

        let (sink, as_ref) = recorder();
        scoped(as_ref, async {
            tokio::spawn(crate::inherit(async {
                let _ = run_process(ProcSpec::new("/bin/echo", "gather").arg("hi")).await;
            }))
            .await
            .unwrap();
        })
        .await;
        assert!(!sink.lines().is_empty(), "inherit() restores the Terminal");
    }
}
