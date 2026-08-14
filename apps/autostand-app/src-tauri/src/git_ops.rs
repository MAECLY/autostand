//! Git operations on the dailies repo: pull, commit, push, merge driver.
//!
//! See step (5) `commit_push` and § Error recovery of `docs/specs/pipeline.md`,
//! `docs/architecture/05-security.md` § Git safety, and
//! `docs/specs/standup-file-format.md` § Invariants.
//!
//! Four rules shape every function here:
//!
//! 1. **The local file is authoritative.** A failing `git pull` or `git push`
//!    logs a warning and returns `Ok`: the standup is already on disk, and the
//!    next run re-syncs and re-pushes it. Only a misconfigured caller — `git`
//!    missing from `PATH`, or a directory that is not a repository — is an error.
//! 2. **Only `.md` files are ever staged.** Audit sidecars (`.json`) and dirty
//!    hashes (`.txt`) live under `state/` and must never reach the remote.
//! 3. **Refusing to commit is always safe; committing garbage is not.** Unmerged
//!    paths, a detached `HEAD`, or conflict markers inside a file all downgrade
//!    the run to "committed nothing" instead of pushing a broken merge.
//! 4. **No shell.** Every argument is passed to `git` as a separate `OsStr`, so a
//!    file name can never be reinterpreted as shell syntax.
//!
//! Two hard invariants inherited from the App Script (see `AGENTS.md`): commits
//! carry **no** `Co-Authored-By` trailer, and `--no-verify` is never passed.

use std::ffi::{OsStr, OsString};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::Duration;

use autostand_runlog::proc::{run_process, ProcError, ProcSpec};

use crate::run_log::RunKind;

/// Upper bound on a single `git` invocation.
///
/// WHY: the scheduler runs unattended, so a `push` that hangs on an unreachable
/// remote must not wedge the pipeline forever. A timeout is reported exactly like
/// a non-zero exit — a warning — because the file on disk is still authoritative.
const GIT_TIMEOUT: Duration = Duration::from_secs(180);

/// Name of the attributes file that carries the union merge driver rule.
const GITATTRIBUTES: &str = ".gitattributes";

/// The rule [`ensure_gitattributes`] installs.
///
/// WHY this glob and not `*.md`: only the dated standup files may be merged by
/// concatenation. `README.md` and friends are hand-edited prose where a real
/// conflict must stop and be resolved, not be silently doubled. This is byte-for-
/// byte the rule the App Script's dailies repo already carries, so running
/// autostand against an existing repo is a no-op.
const UNION_RULE: &str = "20[0-9][0-9]-[0-9][0-9]-[0-9][0-9].md merge=union";

/// Patterns that already satisfy the union requirement for dated standup files,
/// so a repo carrying any of them is left untouched.
const UNION_PATTERNS: [&str; 3] = ["20[0-9][0-9]-[0-9][0-9]-[0-9][0-9].md", "*.md", "*"];

/// Comment written above [`UNION_RULE`] so a human reading the dailies repo knows
/// why the rule is there before deciding to delete it.
const UNION_COMMENT: &str = "\
# autostand: a standup file holds one AUTO block per machine plus one shared
# MANUAL region. Two machines render the same date on the same day and an
# unattended run cannot resolve a merge conflict, so `union` — a built-in driver,
# no external program — keeps both sides' lines and conflict markers can never be
# committed. fileops collapses any duplicated AUTO block on the next run.
";

/// Line prefixes that mark an unresolved merge.
///
/// The trailing space is deliberate and matches the App Script: it keeps a
/// Markdown setext underline (`=======`) or a deeply nested blockquote from being
/// mistaken for a conflict. `=======` is intentionally absent for the same reason.
const CONFLICT_MARKERS: [&str; 3] = ["<<<<<<< ", ">>>>>>> ", "||||||| "];

/// Failures that mean the caller is misconfigured rather than merely offline.
///
/// Everything recoverable — a rejected push, a dirty index, a file with conflict
/// markers — is reported through [`CommitOutcome`] and a `tracing` warning
/// instead, per § Error recovery of `docs/specs/pipeline.md`.
#[derive(Debug, thiserror::Error)]
pub enum GitError {
    /// No `git` executable on `PATH`.
    #[error("git executable not found on PATH (running `git {command}`)")]
    GitMissing {
        /// Subcommand that was being run when the lookup failed.
        command: String,
    },
    /// The directory is not a git working tree, so there is nothing to sync to.
    #[error("not a git repository: {0}")]
    NotARepo(PathBuf),
    /// `git` was found but the subprocess could not be run or awaited.
    #[error("failed to run `git {command}` in {dir}: {source}")]
    Spawn {
        /// Subcommand that failed to start.
        command: String,
        /// Working directory the subcommand was to run in.
        dir: PathBuf,
        /// Underlying OS error.
        #[source]
        source: std::io::Error,
    },
}

/// What [`commit_push`] actually did.
///
/// `committed: false` is a **normal** outcome (nothing touched, nothing changed,
/// or the repo was in a state where committing would be unsafe) — never an error.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    /// Whether a commit object was created.
    pub committed: bool,
    /// Whether that commit reached the remote. Always `false` when `committed` is
    /// `false`; a `true`/`false` pair means the commit is local and will be pushed
    /// by a later run.
    pub pushed: bool,
    /// The `standup: <comma-separated dates>` message derived from the eligible
    /// file stems. Empty when no `.md` file was eligible; otherwise populated even
    /// if the commit did not happen, so callers can log what *would* have landed.
    pub message: String,
}

impl CommitOutcome {
    /// Outcome for a run that created no commit, carrying the message it would
    /// have used (empty when no file was eligible in the first place).
    fn not_committed(message: String) -> Self {
        Self {
            committed: false,
            pushed: false,
            message,
        }
    }
}

/// Whether `dir` is the root of a git working tree.
///
/// WHY a filesystem probe instead of `git rev-parse`: this runs on every pipeline
/// pass and on every `commit_push`, where a subprocess would be pure overhead.
/// `.git` is tested with `exists()` rather than `is_dir()` because a linked
/// worktree or a submodule stores `.git` as a *file* pointing at the real git dir.
pub fn is_git_repo(dir: &Path) -> bool {
    dir.join(".git").exists()
}

/// Install the union merge driver rule in `<dir>/.gitattributes` if it is missing.
///
/// Returns `true` when the file was created or appended to, `false` when a rule
/// covering the dated standup files was already present.
///
/// WHY self-healing rather than one-time setup: the rule is what stops two
/// machines that render the same date from committing `<<<<<<<` markers. A repo
/// cloned before autostand existed, or one where the file was deleted, must
/// repair itself on the next run. The function only ever **appends**, so unrelated
/// rules (`README.md -merge`, LFS filters, `linguist-*`) survive untouched, and it
/// is idempotent so repeated runs produce no churn to commit.
pub fn ensure_gitattributes(dir: &Path) -> std::io::Result<bool> {
    let path = dir.join(GITATTRIBUTES);
    let existing = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(err) => return Err(err),
    };

    if has_union_rule(&existing) {
        return Ok(false);
    }

    let mut next = existing;
    if !next.is_empty() {
        if !next.ends_with('\n') {
            next.push('\n');
        }
        next.push('\n');
    }
    next.push_str(UNION_COMMENT);
    next.push_str(UNION_RULE);
    next.push('\n');

    write_atomic(&path, &next)?;
    tracing::info!(path = %path.display(), "installed the union merge driver rule");
    Ok(true)
}

/// `git pull --rebase --autostash` on the dailies repo.
///
/// A failed pull is a warning, not an error: the local copy is still usable and
/// the next run retries. Errors are reserved for "this is not a repo" and "git is
/// not installed".
#[tracing::instrument(skip_all, fields(dir = %dir.display()))]
pub async fn sync_pull(dir: &Path) -> Result<(), GitError> {
    if !is_git_repo(dir) {
        return Err(GitError::NotARepo(dir.to_path_buf()));
    }

    let pull = run_git(dir, ["pull", "--rebase", "--autostash"]).await?;
    if pull.success {
        tracing::info!("dailies repo synced");
    } else {
        tracing::warn!(
            stderr = %pull.stderr,
            "git pull failed; continuing with the local copy (retry next run)"
        );
    }

    // WHY check the index and not the exit code: a conflicted autostash *pop*
    // exits 0 and leaves no rebase state behind, which is how conflict markers
    // once reached the remote. Trust `ls-files --unmerged`.
    let unmerged = run_git(dir, ["ls-files", "--unmerged"]).await?;
    if !unmerged.stdout.is_empty() {
        tracing::warn!("unmerged paths after pull; aborting the rebase");
        let abort = run_git(dir, ["rebase", "--abort"]).await?;
        if !abort.success {
            // Nothing more to do here: `commit_push` refuses to commit while the
            // index is unmerged, so the repo is left for the user to resolve.
            tracing::warn!(stderr = %abort.stderr, "could not abort the rebase");
        }
    }

    Ok(())
}

/// Stage, commit and push the standup files touched by this run.
///
/// Only `.md` paths are staged — audit sidecars and dirty hashes are `.json`/
/// `.txt` under `state/` and are therefore excluded by construction. The commit
/// message is exactly `standup: <comma-separated dates>`, built from the file
/// stems in the order they were touched.
///
/// The commit is created with `-m`, which bypasses any `commit.template`, and no
/// trailer is ever appended; hooks are left enabled (`--no-verify` is forbidden).
#[tracing::instrument(skip_all, fields(dir = %dir.display(), files = files.len()))]
pub async fn commit_push(dir: &Path, files: &[PathBuf]) -> Result<CommitOutcome, GitError> {
    // Repo-relative, `.md`-only, de-duplicated, in the order they were touched.
    let mut candidates: Vec<PathBuf> = Vec::with_capacity(files.len());
    for path in files.iter().filter(|p| is_markdown(p)) {
        let relative = path.strip_prefix(dir).unwrap_or(path).to_path_buf();
        if !candidates.contains(&relative) {
            candidates.push(relative);
        }
    }
    if candidates.is_empty() {
        tracing::info!("no standup files touched; nothing to commit");
        return Ok(CommitOutcome::not_committed(String::new()));
    }

    if !is_git_repo(dir) {
        return Err(GitError::NotARepo(dir.to_path_buf()));
    }

    let unmerged = run_git(dir, ["ls-files", "--unmerged"]).await?;
    if !unmerged.stdout.is_empty() {
        tracing::warn!("unmerged paths present; NOT committing");
        return Ok(CommitOutcome::not_committed(String::new()));
    }

    // WHY: a commit made on a detached HEAD can never be pushed, so it would
    // silently strand the day's standup on this machine.
    let head = run_git(dir, ["symbolic-ref", "--quiet", "HEAD"]).await?;
    if !head.success {
        tracing::warn!("detached HEAD; NOT committing");
        return Ok(CommitOutcome::not_committed(String::new()));
    }

    let safe = drop_conflicted(dir, candidates).await;
    if safe.is_empty() {
        tracing::warn!("no file is safe to commit");
        return Ok(CommitOutcome::not_committed(String::new()));
    }

    let message = commit_message(&safe);

    let add = run_git(dir, git_args(&["add", "--"], &safe)).await?;
    if !add.success {
        tracing::warn!(stderr = %add.stderr, "git add failed; NOT committing");
        return Ok(CommitOutcome::not_committed(message));
    }

    // `--quiet` exits 0 when the staged tree matches HEAD: nothing changed since
    // the last run, which is the common case for a re-triggered compile.
    let staged = run_git(dir, git_args(&["diff", "--cached", "--quiet", "--"], &safe)).await?;
    if staged.success {
        tracing::info!("standup files already committed; nothing to do");
        return Ok(CommitOutcome::not_committed(message));
    }

    let commit = run_git(
        dir,
        git_args(&["commit", "--message", &message, "--"], &safe),
    )
    .await?;
    if !commit.success {
        tracing::warn!(stderr = %commit.stderr, "git commit failed; retry next run");
        return Ok(CommitOutcome::not_committed(message));
    }
    tracing::info!(message = %message, "committed standup files");

    let push = run_git(dir, ["push", "origin", "HEAD"]).await?;
    if !push.success {
        tracing::warn!(
            stderr = %push.stderr,
            "git push failed; the commit is local and will push on a later run"
        );
        return Ok(CommitOutcome {
            committed: true,
            pushed: false,
            message,
        });
    }
    tracing::info!("pushed");

    Ok(CommitOutcome {
        committed: true,
        pushed: true,
        message,
    })
}

/// Whether `path` is a Markdown file, i.e. a standup file rather than state.
///
/// Deliberately case-sensitive: standup files are always `<date>.md`, and an
/// exact match keeps the "only `.md` is staged" rule mechanically checkable.
fn is_markdown(path: &Path) -> bool {
    path.extension() == Some(OsStr::new("md"))
}

/// Drop any file that carries conflict markers (or that cannot be read).
///
/// One bad file must not block the others, so this filters instead of failing.
async fn drop_conflicted(dir: &Path, candidates: Vec<PathBuf>) -> Vec<PathBuf> {
    let mut safe = Vec::with_capacity(candidates.len());
    for relative in candidates {
        let absolute = dir.join(&relative);
        match tokio::fs::read_to_string(&absolute).await {
            Ok(text) if has_conflict_markers(&text) => {
                tracing::warn!(file = %relative.display(), "conflict markers present; skipping file");
            }
            Ok(_) => safe.push(relative),
            Err(err) => {
                tracing::warn!(file = %relative.display(), error = %err, "unreadable; skipping file");
            }
        }
    }
    safe
}

/// Whether `text` contains an unresolved merge marker.
fn has_conflict_markers(text: &str) -> bool {
    text.lines()
        .any(|line| CONFLICT_MARKERS.iter().any(|m| line.starts_with(m)))
}

/// Build `standup: <comma-separated dates>` from the file stems.
fn commit_message(files: &[PathBuf]) -> String {
    let dates: Vec<&str> = files
        .iter()
        .filter_map(|p| p.file_stem().and_then(OsStr::to_str))
        .collect();
    format!("standup: {}", dates.join(", "))
}

/// Concatenate fixed `git` arguments with a pathspec list, as separate `OsString`s.
fn git_args(fixed: &[&str], paths: &[PathBuf]) -> Vec<OsString> {
    let mut args: Vec<OsString> = fixed.iter().map(OsString::from).collect();
    args.extend(paths.iter().map(|p| p.as_os_str().to_os_string()));
    args
}

/// Whether `text` already contains a `merge=union` rule covering standup files.
fn has_union_rule(text: &str) -> bool {
    text.lines().any(|line| {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            return false;
        }
        let mut fields = line.split_whitespace();
        let Some(pattern) = fields.next() else {
            return false;
        };
        UNION_PATTERNS.contains(&pattern) && fields.any(|attr| attr == "merge=union")
    })
}

/// Write `contents` to `path` via write → fsync → rename.
///
/// WHY: `.gitattributes` is the file that prevents conflict markers; a torn write
/// during a crash would silently disable that protection.
fn write_atomic(path: &Path, contents: &str) -> std::io::Result<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp_name = path
        .file_name()
        .unwrap_or_else(|| OsStr::new("attributes"))
        .to_os_string();
    tmp_name.push(".autostand.tmp");
    let tmp = dir.join(tmp_name);

    if let Err(err) = write_and_sync(&tmp, contents) {
        drop(std::fs::remove_file(&tmp));
        return Err(err);
    }
    std::fs::rename(&tmp, path)
}

/// Create `path`, write `contents`, and fsync before returning.
fn write_and_sync(path: &Path, contents: &str) -> std::io::Result<()> {
    let mut file = std::fs::File::create(path)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()
}

/// Captured result of one `git` invocation.
#[derive(Debug)]
struct GitOutput {
    /// Whether git exited with status 0.
    success: bool,
    /// Trimmed stdout.
    stdout: String,
    /// Trimmed stderr, or the timeout notice when [`GIT_TIMEOUT`] elapsed.
    stderr: String,
}

/// Run `git <args>` in `dir` and capture its output.
///
/// A timeout is surfaced as an unsuccessful [`GitOutput`] rather than an error so
/// every caller's existing "non-zero exit ⇒ warn and carry on" branch handles it.
///
/// The spawn goes through the workspace's single spawner, so the Terminal shows
/// `git pull` / `git commit` / `git push` with an exit code and a duration. What
/// it never shows is the argv: every call site here passes the dailies directory
/// and the standup file names, and one of them passes the commit message.
/// `git <subcommand>` is safe to display because every caller's first argument is
/// a literal in this file.
async fn run_git<I, S>(dir: &Path, args: I) -> Result<GitOutput, GitError>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let args: Vec<OsString> = args
        .into_iter()
        .map(|arg| arg.as_ref().to_os_string())
        .collect();
    let command = args
        .first()
        .map(|arg| arg.to_string_lossy().into_owned())
        .unwrap_or_default();

    let spec = ProcSpec::new("git", RunKind::RepoSync.step())
        .args(&args)
        .cwd(dir)
        // An unattended run must never block on a credential prompt.
        .env("GIT_TERMINAL_PROMPT", "0")
        .timeout(GIT_TIMEOUT)
        .label(format!("git {command}"));

    match run_process(spec).await {
        Ok(out) => Ok(GitOutput {
            success: out.success,
            stdout: out.stdout_trimmed().to_owned(),
            stderr: out.stderr_trimmed().to_owned(),
        }),
        Err(ProcError::Spawn {
            kind: std::io::ErrorKind::NotFound,
            ..
        }) => Err(GitError::GitMissing { command }),
        Err(ProcError::Spawn { kind, .. } | ProcError::Io { kind, .. }) => Err(GitError::Spawn {
            command,
            dir: dir.to_path_buf(),
            source: std::io::Error::from(kind),
        }),
        Err(ProcError::Timeout { secs, .. }) => Ok(GitOutput {
            success: false,
            stdout: String::new(),
            stderr: format!("`git {command}` timed out after {secs}s"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        commit_push, ensure_gitattributes, is_git_repo, sync_pull, CommitOutcome, GitError,
    };
    use std::path::{Path, PathBuf};
    use std::sync::Arc;
    use tempfile::TempDir;

    /// Run `git <args>` in `dir`, asserting success, and return trimmed stdout.
    ///
    /// A synchronous fixture, not an action: it seeds a throwaway repository
    /// before the code under test runs, has no run to log into, and is the one
    /// shape of direct spawn `crates/autostand-runlog/tests/single_spawner.rs`
    /// deliberately does not scan.
    fn git(dir: &Path, args: &[&str]) -> String {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_owned()
    }

    /// A throwaway repo with a local identity and every ambient influence
    /// (signing, hooks, commit template) disabled, so the assertions below cannot
    /// depend on the developer's global git configuration.
    fn repo() -> TempDir {
        let dir = TempDir::new().expect("temp dir");
        let path = dir.path();
        git(path, &["init", "--quiet"]);
        // Version-agnostic equivalent of `git init -b main`.
        git(path, &["symbolic-ref", "HEAD", "refs/heads/main"]);
        git(path, &["config", "user.email", "tester@example.invalid"]);
        git(path, &["config", "user.name", "Autostand Tester"]);
        git(path, &["config", "commit.gpgsign", "false"]);
        git(path, &["config", "commit.template", ""]);
        let hooks = path.join("empty-hooks");
        std::fs::create_dir_all(&hooks).expect("hooks dir");
        git(
            path,
            &["config", "core.hooksPath", hooks.to_str().expect("utf-8")],
        );
        dir
    }

    /// Write `body` to `<dir>/<name>`, creating parent directories.
    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("parent dir");
        }
        std::fs::write(&path, body).expect("write file");
        path
    }

    #[test]
    fn is_git_repo_distinguishes_a_repo_from_a_plain_directory() {
        let plain = TempDir::new().expect("temp dir");
        assert!(!is_git_repo(plain.path()));

        let repo = repo();
        assert!(is_git_repo(repo.path()));
    }

    #[test]
    fn ensure_gitattributes_writes_once_then_is_idempotent() {
        let dir = TempDir::new().expect("temp dir");

        assert!(ensure_gitattributes(dir.path()).expect("first call"));
        let after_first = std::fs::read_to_string(dir.path().join(".gitattributes")).expect("read");
        assert!(
            after_first.contains("merge=union"),
            "rule missing: {after_first}"
        );

        assert!(
            !ensure_gitattributes(dir.path()).expect("second call"),
            "second call must report no write"
        );
        let after_second =
            std::fs::read_to_string(dir.path().join(".gitattributes")).expect("read");
        assert_eq!(after_first, after_second, "file must not churn");
    }

    #[test]
    fn ensure_gitattributes_keeps_unrelated_lines() {
        let dir = TempDir::new().expect("temp dir");
        write(
            dir.path(),
            ".gitattributes",
            "# hand written\nREADME.md  -merge\n*.png binary\n",
        );

        assert!(ensure_gitattributes(dir.path()).expect("append"));

        let text = std::fs::read_to_string(dir.path().join(".gitattributes")).expect("read");
        assert!(text.contains("README.md  -merge"), "clobbered: {text}");
        assert!(text.contains("*.png binary"), "clobbered: {text}");
        assert!(text.contains("merge=union"), "rule missing: {text}");
    }

    #[test]
    fn ensure_gitattributes_accepts_an_existing_union_rule() {
        for existing in [
            "20[0-9][0-9]-[0-9][0-9]-[0-9][0-9].md merge=union\n",
            "*.md merge=union\n",
            "# comment\n\n*.md   merge=union   text\n",
        ] {
            let dir = TempDir::new().expect("temp dir");
            write(dir.path(), ".gitattributes", existing);

            assert!(
                !ensure_gitattributes(dir.path()).expect("no write"),
                "should have recognised {existing:?}"
            );
            let text = std::fs::read_to_string(dir.path().join(".gitattributes")).expect("read");
            assert_eq!(text, existing, "file must be untouched");
        }
    }

    #[test]
    fn ensure_gitattributes_ignores_a_commented_out_rule() {
        let dir = TempDir::new().expect("temp dir");
        write(dir.path(), ".gitattributes", "#*.md merge=union\n");

        assert!(
            ensure_gitattributes(dir.path()).expect("append"),
            "a commented rule is not an active rule"
        );
    }

    #[tokio::test]
    async fn commit_push_stages_only_markdown_files() {
        let dir = repo();
        let standup = write(dir.path(), "2026-08-03.md", "# standup\n");
        let audit = write(dir.path(), "state/audit/2026-08-03-mac.json", "{}\n");
        let hash = write(dir.path(), "state/hashes/2026-08-03-mac.txt", "deadbeef\n");

        let outcome = commit_push(dir.path(), &[standup, audit, hash])
            .await
            .expect("commit_push");

        assert!(outcome.committed, "the .md file should have been committed");
        assert!(
            !outcome.pushed,
            "there is no remote, so the push must fail non-fatally"
        );

        let listed = git(dir.path(), &["show", "--name-only", "--format=", "HEAD"]);
        assert_eq!(listed, "2026-08-03.md", "only the .md file may be staged");
    }

    #[tokio::test]
    async fn commit_message_is_standup_plus_comma_separated_dates() {
        let dir = repo();
        let today = write(dir.path(), "2026-08-04.md", "# today\n");
        let prev = write(dir.path(), "2026-08-03.md", "# prev\n");

        let outcome = commit_push(dir.path(), &[today, prev])
            .await
            .expect("commit_push");

        assert_eq!(outcome.message, "standup: 2026-08-04, 2026-08-03");
        assert_eq!(
            git(dir.path(), &["log", "-1", "--format=%s"]),
            "standup: 2026-08-04, 2026-08-03"
        );
    }

    #[tokio::test]
    async fn commit_has_no_coauthor_trailer() {
        let dir = repo();
        let standup = write(dir.path(), "2026-08-03.md", "# standup\n");

        commit_push(dir.path(), &[standup]).await.expect("commit");

        let body = git(dir.path(), &["log", "-1", "--format=%B"]);
        assert!(
            !body.to_ascii_lowercase().contains("co-authored-by"),
            "the App Script forbids coauthor trailers, message was: {body:?}"
        );
        assert_eq!(body, "standup: 2026-08-03", "no trailers of any kind");
        assert_eq!(
            git(dir.path(), &["log", "-1", "--format=%ae"]),
            "tester@example.invalid",
            "the commit must keep the repo's own identity"
        );
    }

    #[tokio::test]
    async fn commit_push_with_no_files_is_a_noop() {
        let dir = repo();

        let outcome = commit_push(dir.path(), &[]).await.expect("commit_push");

        assert_eq!(outcome, CommitOutcome::not_committed(String::new()));
        // `git log` fails on an unborn HEAD, which proves nothing was committed.
        let out = std::process::Command::new("git")
            .args(["log", "--oneline"])
            .current_dir(dir.path())
            .output()
            .expect("git runs");
        assert!(!out.status.success(), "no commit may exist");
    }

    #[tokio::test]
    async fn commit_push_with_only_state_files_is_a_noop() {
        let dir = repo();
        let audit = write(dir.path(), "state/audit/2026-08-03-mac.json", "{}\n");

        let outcome = commit_push(dir.path(), &[audit])
            .await
            .expect("commit_push");

        assert!(!outcome.committed);
        assert!(outcome.message.is_empty());
    }

    #[tokio::test]
    async fn commit_push_reports_nothing_to_commit_on_a_second_run() {
        let dir = repo();
        let standup = write(dir.path(), "2026-08-03.md", "# standup\n");

        assert!(
            commit_push(dir.path(), std::slice::from_ref(&standup))
                .await
                .expect("first commit")
                .committed
        );

        let second = commit_push(dir.path(), &[standup])
            .await
            .expect("second commit");
        assert!(!second.committed, "an unchanged file is not an error");
        assert_eq!(second.message, "standup: 2026-08-03");
        assert_eq!(git(dir.path(), &["rev-list", "--count", "HEAD"]), "1");
    }

    #[tokio::test]
    async fn commit_push_skips_files_with_conflict_markers() {
        let dir = repo();
        let clean = write(dir.path(), "2026-08-03.md", "# clean\n");
        let broken = write(
            dir.path(),
            "2026-08-04.md",
            "<<<<<<< HEAD\nours\n=======\ntheirs\n>>>>>>> other\n",
        );

        let outcome = commit_push(dir.path(), &[clean, broken])
            .await
            .expect("commit_push");

        assert!(outcome.committed);
        assert_eq!(outcome.message, "standup: 2026-08-03");
        assert_eq!(
            git(dir.path(), &["show", "--name-only", "--format=", "HEAD"]),
            "2026-08-03.md",
            "a file with markers must never be committed"
        );
    }

    #[tokio::test]
    async fn commit_push_refuses_on_a_detached_head() {
        let dir = repo();
        let first = write(dir.path(), "2026-08-03.md", "# standup\n");
        commit_push(dir.path(), &[first]).await.expect("first");
        git(dir.path(), &["checkout", "--quiet", "--detach", "HEAD"]);

        let next = write(dir.path(), "2026-08-04.md", "# next\n");
        let outcome = commit_push(dir.path(), &[next]).await.expect("commit_push");

        assert!(
            !outcome.committed,
            "a detached-HEAD commit could never be pushed"
        );
        assert_eq!(git(dir.path(), &["rev-list", "--count", "HEAD"]), "1");
    }

    #[tokio::test]
    async fn commit_push_on_a_plain_directory_is_an_error() {
        let dir = TempDir::new().expect("temp dir");
        let standup = write(dir.path(), "2026-08-03.md", "# standup\n");

        let err = commit_push(dir.path(), &[standup])
            .await
            .expect_err("not a repo");

        assert!(
            matches!(err, GitError::NotARepo(ref path) if path == dir.path()),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn sync_pull_on_a_plain_directory_is_an_error() {
        let dir = TempDir::new().expect("temp dir");

        let err = sync_pull(dir.path()).await.expect_err("not a repo");

        assert!(
            matches!(err, GitError::NotARepo(ref path) if path == dir.path()),
            "unexpected error: {err:?}"
        );
    }

    #[tokio::test]
    async fn sync_pull_without_a_remote_is_non_fatal() {
        let dir = repo();
        let standup = write(dir.path(), "2026-08-03.md", "# standup\n");
        commit_push(dir.path(), &[standup]).await.expect("commit");

        // No remote is configured, so the pull fails — and that must not abort.
        sync_pull(dir.path())
            .await
            .expect("pull failure is a warning");
    }

    // -- terminal visibility ------------------------------------------------

    /// Committing is the flagship action of a compile, so it must be visible —
    /// and it is also the call whose argv carries the dailies path, the file
    /// names and the commit message, so *only* the subcommand may be shown.
    #[tokio::test]
    async fn commit_push_reaches_the_terminal_without_leaking_its_argv() {
        let dir = repo();
        let standup = write(dir.path(), "2026-08-03.md", "# standup\n");
        let sink = Arc::new(autostand_runlog::RecordingSink::new());
        let as_ref: autostand_runlog::SinkRef = sink.clone();

        let outcome = autostand_runlog::scoped(as_ref, async {
            commit_push(dir.path(), &[standup]).await.expect("commit")
        })
        .await;
        assert!(outcome.committed);

        let lines = sink.lines();
        let rendered = lines
            .iter()
            .map(|line| {
                format!(
                    "{} {} {}",
                    line.step,
                    line.message,
                    line.detail.clone().unwrap_or_default()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        assert!(rendered.contains("git add"), "{rendered}");
        assert!(rendered.contains("git commit"), "{rendered}");
        assert!(rendered.contains("git push"), "{rendered}");
        assert!(
            lines.iter().all(|line| line.step == "repo_sync"),
            "every git line belongs to the repo_sync step: {rendered}"
        );

        let leaked = dir.path().to_string_lossy().to_string();
        assert!(!rendered.contains(&leaked), "repo path leaked: {rendered}");
        assert!(
            !rendered.contains("2026-08-03"),
            "the commit message and file name leaked: {rendered}"
        );
        // The push fails (there is no remote) and git says so loudly on stderr.
        assert!(
            !rendered.contains("origin"),
            "git stderr reached the panel: {rendered}"
        );
    }
}
