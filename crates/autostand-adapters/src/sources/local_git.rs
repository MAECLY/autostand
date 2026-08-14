//! Local `Git` data source (authoritative commits).
//! See `docs/data-sources/01-local-git.md`.
//!
//! Every "I gathered nothing" path here is a `DataSourceError`, not an empty
//! [`SourceData`]. This source is the authoritative one: its FACTS block is what
//! `provenance::compute` derives `range_tickets`/`covered_tickets` from, and
//! those in turn are what the renderer's no-work-claim and out-of-range-ticket
//! checks key on. An empty FACTS block therefore does not merely lose data — it
//! disarms the validation, and it is indistinguishable from a day off. Only one
//! outcome is allowed to be silently empty: a window in which nobody at all
//! committed.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use async_trait::async_trait;

use super::helpers::{extract_ticket_keys, log_sweep, run_cmd, scan_repos};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// Timeout applied to every `git` invocation this source makes.
const GIT_TIMEOUT_SECS: u64 = 30;

/// Timeout for the short `git config` identity probe.
const GIT_CONFIG_TIMEOUT_SECS: u64 = 10;

/// Ref selector used when `git_refs` is blank.
const DEFAULT_REFS: &str = "--all";

/// How many distinct identities the "matched nobody" diagnostic quotes back.
const MAX_REPORTED_IDENTITIES: usize = 8;

/// Local git commit scanner.
pub struct LocalGitDataSource;

#[async_trait]
impl DataSource for LocalGitDataSource {
    fn id(&self) -> &'static str {
        "local-git"
    }
    fn display_name(&self) -> &'static str {
        "Local Git"
    }
    fn is_available(&self) -> bool {
        DataSourceConfig::from_env().github_dir.is_dir()
    }

    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        if !config.github_dir.is_dir() {
            return Err(DataSourceError::Misconfigured(format!(
                "`github_dir` is not a directory: {}",
                config.github_dir.display()
            )));
        }
        let repos = scan_repos(&config.github_dir);
        if repos.is_empty() {
            return Err(DataSourceError::Misconfigured(format!(
                "no git repositories directly under {} — local-git scans that directory at depth 1",
                config.github_dir.display()
            )));
        }

        let filter = AuthorFilter::resolve(&config.authors).await?;
        let spec = ScanSpec::new(&repos, window, &config.git_refs);
        let scan = spec.scan_commits(&filter).await;
        // One `git log` per repository ran silently (see `helpers::run_cmd`);
        // this is the line that puts the sweep in the Terminal. Repository names
        // are deliberately absent: only counts are safe to display.
        log_sweep(
            "local-git scanned repositories",
            format!(
                "{} repo(s), {} with commits, {} failed",
                repos.len(),
                scan.sections.len(),
                scan.errors.len()
            ),
        );

        if scan.sections.is_empty() {
            return spec.explain_empty_scan(&filter, &scan.errors).await;
        }
        // A repo that failed while others succeeded is a partial gather, not a
        // misconfiguration: report it and keep the commits we did read.
        for (repo, error) in &scan.errors {
            tracing::warn!(repo = %repo, error = %error, "local-git: git log failed");
        }

        let mut facts = scan.sections.join("\n\n");
        let ticket_days = spec.scan_ticket_days().await;
        if !ticket_days.is_empty() {
            let map: Vec<String> = ticket_days
                .iter()
                .map(|(key, days)| format!("{key}={}", dedup(days).join(",")))
                .collect();
            let _ = write!(facts, "\n\n<!-- TICKET_DAYS: {} -->", map.join(","));
        }

        Ok(SourceData {
            facts: Some(facts),
            notes: None,
            enrichment: None,
            files: Vec::new(),
        })
    }
}

// ── author filter ──────────────────────────────────────────────────────────

/// Where an [`AuthorFilter`] came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorOrigin {
    /// Taken from the configured `standup_authors`.
    Configured,
    /// Derived from this machine's git identity because `standup_authors` was empty.
    GitIdentity,
}

/// The values `git log --author` filters on, plus their provenance.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AuthorFilter {
    authors: Vec<String>,
    origin: AuthorOrigin,
}

impl AuthorFilter {
    /// Resolve the filter for `configured`, probing the machine's git identity
    /// only when the configured list is empty.
    async fn resolve(configured: &[String]) -> Result<Self, DataSourceError> {
        if !clean_authors(configured).is_empty() {
            return Self::from_parts(configured, &[]);
        }
        Self::from_parts(configured, &detect_git_identity().await)
    }

    /// [`AuthorFilter::resolve`] with the detected identity injected, so the
    /// precedence rules are testable without reading the machine's git config.
    fn from_parts(configured: &[String], detected: &[String]) -> Result<Self, DataSourceError> {
        let authors = clean_authors(configured);
        if !authors.is_empty() {
            return Ok(Self {
                authors,
                origin: AuthorOrigin::Configured,
            });
        }
        // WHY fall back instead of dropping the filter: an unfiltered `git log`
        // would report the whole team's commits as the user's own work, which is
        // worse than reporting none. WHY fall back at all: `standup_authors` is
        // optional and `AppConfig` derives `Default`, so an install that never
        // fills the Settings control carries an empty list — which used to skip the
        // commit scan outright and hand the renderer an empty FACTS block.
        let detected = clean_authors(detected);
        if detected.is_empty() {
            return Err(DataSourceError::Misconfigured(
                "no author filter: `standup_authors` is empty and this machine has no \
                 `git config user.email` or `user.name` to fall back on, so local-git cannot \
                 tell your commits from your teammates'"
                    .to_string(),
            ));
        }
        tracing::warn!(
            authors = ?detected,
            "local-git: `standup_authors` is empty; falling back to the machine git identity"
        );
        Ok(Self {
            authors: detected,
            origin: AuthorOrigin::GitIdentity,
        })
    }

    /// One `--author=<value>` flag per entry.
    ///
    /// WHY one flag per entry instead of a `|`-joined regex: git ORs repeated
    /// `--author` options, but a single value is a *basic* regular expression in
    /// which `|` is a literal character — so a joined pattern matches nothing and
    /// every multi-author config silently gathers zero commits. This mirrors the
    /// App Script's `for a in "${AUTHORS[@]}"; do author_args+=(--author="$a"); done`.
    fn args(&self) -> Vec<String> {
        self.authors
            .iter()
            .map(|author| format!("--author={author}"))
            .collect()
    }

    /// Human-readable provenance, for a diagnostic message.
    fn origin_label(&self) -> &'static str {
        match self.origin {
            AuthorOrigin::Configured => "`standup_authors`",
            AuthorOrigin::GitIdentity => "this machine's git identity (`standup_authors` is empty)",
        }
    }
}

/// Trim, drop blanks from and dedupe an author list.
///
/// Public because the Settings readiness report has to show the *same* list the
/// filter will be built from; a second implementation in the app would drift.
pub fn clean_authors(raw: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for author in raw {
        let trimmed = author.trim();
        if trimmed.is_empty() || out.iter().any(|seen| seen == trimmed) {
            continue;
        }
        out.push(trimmed.to_string());
    }
    out
}

/// This machine's git identity: `user.email`, or `user.name` when no email is set.
///
/// Email alone when it exists: a display name is a basic regex that a teammate's
/// name can also satisfy, and a plausible-but-wrong match is worse than none.
///
/// Public because Settings offers this exact value as the suggestion for an
/// empty `standup_authors`, and a suggestion that differs from the fallback
/// would teach the user the wrong thing.
pub async fn detect_git_identity() -> Vec<String> {
    for key in ["user.email", "user.name"] {
        if let Ok(value) = run_cmd("git", &["config", "--get", key], GIT_CONFIG_TIMEOUT_SECS).await
        {
            let value = value.trim();
            if !value.is_empty() {
                return vec![value.to_string()];
            }
        }
    }
    Vec::new()
}

/// Split `git_refs` into arguments, defaulting to `--all`.
///
/// WHY split: the field is free text and `--branches --remotes` is a legal value,
/// but passing it as a single argument makes `git log` reject it — which the old
/// code swallowed as "this repo has no commits in the window".
fn refs_args(git_refs: &str) -> Vec<String> {
    let args: Vec<String> = git_refs.split_whitespace().map(String::from).collect();
    if args.is_empty() {
        vec![DEFAULT_REFS.to_string()]
    } else {
        args
    }
}

// ── scan ───────────────────────────────────────────────────────────────────

/// The inputs every `git log` in one gather shares.
struct ScanSpec<'a> {
    repos: &'a [PathBuf],
    window: &'a DateWindow,
    refs: Vec<String>,
    since: String,
    until: String,
}

/// Outcome of the windowed, author-filtered pass over every repo.
#[derive(Debug, Default)]
struct CommitScan {
    /// One rendered FACTS section per repo that had matching commits.
    sections: Vec<String>,
    /// (repo name, git error) for repos whose `git log` failed.
    errors: Vec<(String, String)>,
}

/// What committed inside the window, ignoring the author filter.
#[derive(Debug, Default)]
struct WindowProbe {
    /// Distinct author/committer identities, capped at [`MAX_REPORTED_IDENTITIES`].
    identities: Vec<String>,
    /// Commits counted before the cap stopped the probe.
    commits: usize,
    /// Whether the probe stopped early, making `commits` a lower bound.
    truncated: bool,
}

impl<'a> ScanSpec<'a> {
    fn new(repos: &'a [PathBuf], window: &'a DateWindow, git_refs: &str) -> Self {
        // WHY explicit times: `--since=2026-07-31` is parsed by git's approxidate,
        // which fills every unspecified field from the *current clock*. A bare date
        // therefore means "range_start at whatever o'clock it is now" and drops every
        // commit made earlier that day, while `--until=<end + 1 day>` leaks the first
        // hours of the day after the window. Pinning midnight..23:59:59 makes the
        // window the closed day range `docs/specs/pipeline.md` step (a) describes,
        // independent of when the scheduler fired.
        Self {
            repos,
            window,
            refs: refs_args(git_refs),
            since: format!("--since={} 00:00:00", window.start.format("%Y-%m-%d")),
            until: format!("--until={} 23:59:59", window.end.format("%Y-%m-%d")),
        }
    }

    /// `git -C <repo> log <refs…>` — the prefix every query shares.
    fn log_args<'s>(&'s self, repo: &'s Path) -> Vec<&'s str> {
        let mut args = vec!["-C", repo.to_str().unwrap_or("."), "log"];
        args.extend(self.refs.iter().map(String::as_str));
        args
    }

    /// The windowed, author-filtered commit scan.
    async fn scan_commits(&self, filter: &AuthorFilter) -> CommitScan {
        let author_args = filter.args();
        let mut scan = CommitScan::default();
        for repo in self.repos {
            let mut args = self.log_args(repo);
            args.push("--no-merges");
            args.push(&self.since);
            args.push(&self.until);
            args.extend(author_args.iter().map(String::as_str));
            args.push("--format=%H|%cd|%s");
            args.push("--date=short");
            match run_cmd("git", &args, GIT_TIMEOUT_SECS).await {
                Ok(out) => {
                    let commits = parse_subjects(&out);
                    if !commits.is_empty() {
                        scan.sections
                            .push(render_section(&repo_name(repo), &commits));
                    }
                }
                Err(error) => scan.errors.push((repo_name(repo), error)),
            }
        }
        scan
    }

    /// Decide whether an empty scan is a quiet day or a broken configuration.
    ///
    /// This is the whole point of the source returning errors: an empty FACTS
    /// block is what a day off looks like *and* what an author filter that names
    /// nobody looks like, and nothing downstream can tell them apart.
    async fn explain_empty_scan(
        &self,
        filter: &AuthorFilter,
        errors: &[(String, String)],
    ) -> Result<SourceData, DataSourceError> {
        let probe = self.probe_window_identities().await;
        if !probe.identities.is_empty() {
            let at_least = if probe.truncated { "at least " } else { "" };
            return Err(DataSourceError::Misconfigured(format!(
                "the author filter [{filter_list}] from {origin} matched none of the \
                 {at_least}{commits} commits made between {start} and {end}; the identities that \
                 did commit are: {seen}",
                filter_list = filter.authors.join(", "),
                origin = filter.origin_label(),
                commits = probe.commits,
                start = self.window.start,
                end = self.window.end,
                seen = probe.identities.join(", "),
            )));
        }
        if !errors.is_empty() {
            let details: Vec<String> = errors
                .iter()
                .map(|(repo, error)| format!("{repo}: {error}"))
                .collect();
            return Err(DataSourceError::Misconfigured(format!(
                "no commits gathered and `git log` failed in {} of {} repos: {}",
                errors.len(),
                self.repos.len(),
                details.join("; ")
            )));
        }
        // Nobody at all committed in the window: the one honest empty result.
        Ok(SourceData::default())
    }

    /// Who committed inside the window, ignoring the author filter.
    ///
    /// Only ever runs when the filtered scan came back empty, so the second pass
    /// costs nothing on the path where the filter works.
    async fn probe_window_identities(&self) -> WindowProbe {
        let mut probe = WindowProbe::default();
        for repo in self.repos {
            let mut args = self.log_args(repo);
            args.push("--no-merges");
            args.push(&self.since);
            args.push(&self.until);
            args.push("--format=%ae|%ce");
            let Ok(out) = run_cmd("git", &args, GIT_TIMEOUT_SECS).await else {
                continue;
            };
            for line in out.lines().filter(|line| !line.trim().is_empty()) {
                probe.commits += 1;
                for identity in line.split('|').map(str::trim).filter(|id| !id.is_empty()) {
                    if probe.identities.iter().any(|seen| seen == identity) {
                        continue;
                    }
                    if probe.identities.len() >= MAX_REPORTED_IDENTITIES {
                        probe.truncated = true;
                        break;
                    }
                    probe.identities.push(identity.to_string());
                }
            }
            if probe.truncated {
                break;
            }
        }
        probe
    }

    /// Full ticket → commit-day map across every ref.
    ///
    /// Deliberately unwindowed and unfiltered by author: the anti-backdating guard
    /// needs to know that a ticket a note claims today was in fact committed last
    /// week. Runs only after the windowed scan found something, because its result
    /// is appended to a FACTS block that would otherwise not exist.
    async fn scan_ticket_days(&self) -> BTreeMap<String, Vec<String>> {
        let mut ticket_days: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for repo in self.repos {
            let mut args = self.log_args(repo);
            args.push("--format=%cd|%s");
            args.push("--date=short");
            let Ok(out) = run_cmd("git", &args, GIT_TIMEOUT_SECS).await else {
                continue;
            };
            for line in out.lines() {
                let mut parts = line.splitn(2, '|');
                let day = parts.next().unwrap_or("").trim().to_string();
                let subject = parts.next().unwrap_or("");
                for key in extract_ticket_keys(subject) {
                    ticket_days.entry(key).or_default().push(day.clone());
                }
            }
        }
        ticket_days
    }
}

/// Directory name of a repo path.
fn repo_name(repo: &Path) -> String {
    repo.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("?")
        .to_string()
}

// ── rendering ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct Commit {
    #[allow(dead_code)]
    hash: String,
    day: String,
    subject: String,
}

fn parse_subjects(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|line| {
            let mut parts = line.splitn(3, '|');
            let hash = parts.next()?.trim().to_string();
            let day = parts.next()?.trim().to_string();
            let subject = parts.next()?.trim().to_string();
            if hash.is_empty() && subject.is_empty() {
                return None;
            }
            Some(Commit { hash, day, subject })
        })
        .collect()
}

fn dedup<T: AsRef<str>>(items: &[T]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in items {
        let value = item.as_ref();
        if !out.iter().any(|seen| seen == value) {
            out.push(value.to_string());
        }
    }
    out
}

fn render_section(repo: &str, commits: &[Commit]) -> String {
    let keys: Vec<String> = commits
        .iter()
        .flat_map(|commit| extract_ticket_keys(&commit.subject))
        .collect();
    let tickets = dedup(&keys);
    let tickets_str = if tickets.is_empty() {
        "(none)".to_string()
    } else {
        tickets.join(", ")
    };
    let mut section = format!(
        "### repo: {repo} / tickets: {tickets_str} / commits ({}):\n",
        commits.len()
    );
    for commit in commits {
        let key = extract_ticket_keys(&commit.subject)
            .into_iter()
            .next()
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(section, "- ({key}) {}", commit.subject);
    }
    section
}

#[cfg(test)]
mod tests {
    use super::{
        clean_authors, dedup, parse_subjects, refs_args, render_section, AuthorFilter,
        AuthorOrigin, Commit, DataSourceError, LocalGitDataSource,
    };
    use crate::sources::{DataSource, DataSourceConfig, DateWindow};
    use chrono::NaiveDate;
    use std::path::{Path, PathBuf};
    use std::process::Command;
    use tempfile::TempDir;

    /// The identity every fixture commit is authored by.
    const FIXTURE_AUTHOR: &str = "Fixture Dev <fixture@example.invalid>";
    /// [`FIXTURE_AUTHOR`]'s email, as it would appear in `standup_authors`.
    const FIXTURE_EMAIL: &str = "fixture@example.invalid";
    /// The repo planted under the fixture scan root.
    const FIXTURE_REPO: &str = "acme-api";

    // -- fixtures ----------------------------------------------------------

    fn git(dir: &Path, args: &[&str], env: &[(&str, &str)]) {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(dir);
        for (key, value) in env {
            cmd.env(key, value);
        }
        let out = cmd.output().expect("git runs");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// `git init` a repo with every ambient influence (signing, hooks, template)
    /// disabled, so the assertions do not depend on the developer's `~/.gitconfig`.
    fn init_repo(dir: &Path) {
        std::fs::create_dir_all(dir).expect("repo dir");
        git(dir, &["init", "--quiet"], &[]);
        git(dir, &["symbolic-ref", "HEAD", "refs/heads/main"], &[]);
        git(dir, &["config", "user.email", FIXTURE_EMAIL], &[]);
        git(dir, &["config", "user.name", "Fixture Dev"], &[]);
        git(dir, &["config", "commit.gpgsign", "false"], &[]);
        git(dir, &["config", "commit.template", ""], &[]);
        let hooks = dir.join(".empty-hooks");
        std::fs::create_dir_all(&hooks).expect("hooks dir");
        git(
            dir,
            &["config", "core.hooksPath", &hooks.display().to_string()],
            &[],
        );
    }

    /// Commit `subject` authored by `author`, pinning both dates so `--since` /
    /// `--until` (which filter on the *committer* date) are deterministic.
    fn commit(repo: &Path, author: &str, when: &str, file: &str, subject: &str) {
        std::fs::write(repo.join(file), subject).expect("write source file");
        git(repo, &["add", "--", file], &[]);
        git(
            repo,
            &[
                "commit",
                "--quiet",
                "--author",
                author,
                "--message",
                subject,
            ],
            &[("GIT_AUTHOR_DATE", when), ("GIT_COMMITTER_DATE", when)],
        );
    }

    /// A scan root holding one repo: two in-window commits and one older commit
    /// whose ticket only exists outside the window.
    fn fixture() -> TempDir {
        let root = TempDir::new().expect("temp scan root");
        let repo = root.path().join(FIXTURE_REPO);
        init_repo(&repo);
        commit(
            &repo,
            FIXTURE_AUTHOR,
            "2026-07-20 12:00:00",
            "billing.rs",
            "feat(billing): FIF-900 import legacy invoices",
        );
        commit(
            &repo,
            FIXTURE_AUTHOR,
            "2026-08-03 12:00:00",
            "checkout.rs",
            "fix(checkout): FIF-201 correct total rounding",
        );
        commit(
            &repo,
            FIXTURE_AUTHOR,
            "2026-08-04 09:00:00",
            "inventory.rs",
            "feat(inventory): FIF-202 retry failed syncs",
        );
        root
    }

    fn day(month: u32, day: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, month, day).expect("valid date")
    }

    fn window(start: (u32, u32), end: (u32, u32)) -> DateWindow {
        DateWindow {
            start: day(start.0, start.1),
            end: day(end.0, end.1),
            dates: Vec::new(),
        }
    }

    /// The in-window range of [`fixture`]: both August commits, not the July one.
    fn august_window() -> DateWindow {
        window((8, 3), (8, 4))
    }

    fn config(root: &Path, authors: &[&str]) -> DataSourceConfig {
        DataSourceConfig {
            github_dir: root.to_path_buf(),
            authors: authors.iter().map(|a| (*a).to_string()).collect(),
            ..DataSourceConfig::default()
        }
    }

    // -- the regression this module exists for -----------------------------

    /// The bug: `standup_authors` has no Settings control, so the persisted config
    /// carried `[]`. The scan was wrapped in `if !author_args.is_empty()`, so an
    /// empty list skipped `git log` entirely and returned `Ok(SourceData::default())`
    /// — a result the pipeline cannot distinguish from "committed nothing today".
    /// Every audit sidecar on the author's machine had `facts: []` for this reason,
    /// which in turn left `range_tickets`/`covered_tickets` empty and disarmed the
    /// renderer's no-work-claim and out-of-range-ticket checks.
    #[tokio::test]
    async fn an_empty_author_list_is_never_a_silently_empty_gather() {
        let root = fixture();
        let result = LocalGitDataSource
            .gather(&august_window(), &config(root.path(), &[]))
            .await;

        match result {
            // The machine identity does not author the fixture commits, so the
            // fallback cannot match — but the failure must be *reported*.
            Err(DataSourceError::Misconfigured(message)) => assert!(
                message.contains("standup_authors"),
                "the diagnostic must name the field to fix: {message}"
            ),
            Err(other) => panic!("unexpected error kind: {other}"),
            Ok(data) => panic!("an empty author list must not gather silently: {data:?}"),
        }
    }

    #[tokio::test]
    async fn configured_authors_produce_a_facts_block_for_the_window() {
        let root = fixture();
        let data = LocalGitDataSource
            .gather(&august_window(), &config(root.path(), &[FIXTURE_EMAIL]))
            .await
            .expect("the configured author wrote both in-window commits");

        let facts = data.facts.expect("facts block");
        assert!(facts.contains("### repo: acme-api"), "{facts}");
        assert!(facts.contains("FIF-201"), "{facts}");
        assert!(facts.contains("FIF-202"), "{facts}");
        assert!(
            facts.contains("commits (2)"),
            "only the two in-window commits: {facts}"
        );
        assert!(
            !facts.contains("import legacy invoices"),
            "the July commit is outside the window: {facts}"
        );
        assert!(data.notes.is_none() && data.enrichment.is_none());
    }

    /// The `TICKET_DAYS` marker feeds the anti-backdating guard, so it must carry
    /// history from *outside* the window — that is the only way the guard can tell
    /// that a ticket a note claims today actually landed in July.
    #[tokio::test]
    async fn the_ticket_days_marker_carries_history_outside_the_window() {
        let root = fixture();
        let data = LocalGitDataSource
            .gather(&august_window(), &config(root.path(), &[FIXTURE_EMAIL]))
            .await
            .expect("gather");

        let facts = data.facts.expect("facts block");
        let marker = facts
            .lines()
            .find(|line| line.starts_with("<!-- TICKET_DAYS:"))
            .expect("ticket-days marker");
        assert!(marker.contains("FIF-900=2026-07-20"), "{marker}");
        assert!(marker.contains("FIF-201=2026-08-03"), "{marker}");
    }

    /// The second-worst failure mode: a filter that is set but wrong (a personal
    /// email configured while every commit carries the work one). It used to read
    /// exactly like a day off.
    #[tokio::test]
    async fn an_author_filter_that_matches_nobody_names_who_did_commit() {
        let root = fixture();
        let error = LocalGitDataSource
            .gather(
                &august_window(),
                &config(root.path(), &["nobody@else.invalid"]),
            )
            .await
            .expect_err("a filter matching nobody is a failure, not an empty day");

        let message = error.to_string();
        assert!(message.contains("nobody@else.invalid"), "{message}");
        assert!(
            message.contains(FIXTURE_EMAIL),
            "the diagnostic must quote the identity that did commit: {message}"
        );
        assert!(
            message.contains("2026-08-03") && message.contains("2026-08-04"),
            "{message}"
        );
    }

    /// The one empty result that is honest: the filter is fine, nobody committed.
    #[tokio::test]
    async fn a_window_without_any_commits_is_a_quiet_day_not_a_failure() {
        let root = fixture();
        let data = LocalGitDataSource
            .gather(
                &window((7, 1), (7, 2)),
                &config(root.path(), &[FIXTURE_EMAIL]),
            )
            .await
            .expect("a genuinely quiet window is not an error");
        assert!(data.facts.is_none());
    }

    #[tokio::test]
    async fn a_scan_root_without_repositories_is_reported() {
        let root = TempDir::new().expect("temp dir");
        std::fs::create_dir_all(root.path().join("not-a-repo")).expect("plain dir");
        let error = LocalGitDataSource
            .gather(&august_window(), &config(root.path(), &[FIXTURE_EMAIL]))
            .await
            .expect_err("a scan root with no repos is a misconfiguration");
        assert!(error.to_string().contains("no git repositories"), "{error}");
    }

    #[tokio::test]
    async fn a_missing_scan_root_is_reported() {
        let missing = PathBuf::from("/this/does/not/exist/at/all");
        let error = LocalGitDataSource
            .gather(&august_window(), &config(&missing, &[FIXTURE_EMAIL]))
            .await
            .expect_err("a missing scan root is a misconfiguration");
        assert!(error.to_string().contains("not a directory"), "{error}");
    }

    /// `git_refs` was passed to `git log` as one argument, so any value with a
    /// space (`--branches --tags`) made every invocation fail — and the failure was
    /// swallowed into an empty section list.
    #[tokio::test]
    async fn a_multi_token_git_refs_value_is_split_into_separate_arguments() {
        let root = fixture();
        let mut cfg = config(root.path(), &[FIXTURE_EMAIL]);
        cfg.git_refs = "--branches --tags".to_string();

        let data = LocalGitDataSource
            .gather(&august_window(), &cfg)
            .await
            .expect("a multi-token ref selector must still scan");
        assert!(data.facts.expect("facts").contains("FIF-201"));
    }

    #[tokio::test]
    async fn a_blank_git_refs_value_falls_back_to_all_refs() {
        let root = fixture();
        let mut cfg = config(root.path(), &[FIXTURE_EMAIL]);
        cfg.git_refs = "   ".to_string();

        let data = LocalGitDataSource
            .gather(&august_window(), &cfg)
            .await
            .expect("blank refs fall back to --all");
        assert!(data.facts.expect("facts").contains("FIF-202"));
    }

    // -- author filter -----------------------------------------------------

    #[test]
    fn a_configured_author_list_wins_over_the_machine_identity() {
        let filter = AuthorFilter::from_parts(
            &["dev@example.invalid".to_string()],
            &["machine@example.invalid".to_string()],
        )
        .expect("configured authors resolve");
        assert_eq!(filter.authors, vec!["dev@example.invalid"]);
        assert_eq!(filter.origin, AuthorOrigin::Configured);
    }

    #[test]
    fn an_empty_author_list_falls_back_to_the_machine_identity() {
        let filter = AuthorFilter::from_parts(&[], &["machine@example.invalid".to_string()])
            .expect("the machine identity resolves");
        assert_eq!(filter.authors, vec!["machine@example.invalid"]);
        assert_eq!(filter.origin, AuthorOrigin::GitIdentity);
        assert!(filter.origin_label().contains("standup_authors"));
    }

    #[test]
    fn a_list_of_blanks_counts_as_empty() {
        let filter = AuthorFilter::from_parts(
            &[String::new(), "   ".to_string()],
            &["machine@example.invalid".to_string()],
        )
        .expect("blanks fall through to the machine identity");
        assert_eq!(filter.origin, AuthorOrigin::GitIdentity);
    }

    #[test]
    fn no_configured_authors_and_no_machine_identity_is_misconfigured() {
        let error =
            AuthorFilter::from_parts(&[], &[]).expect_err("nothing to filter on must be an error");
        assert!(matches!(error, DataSourceError::Misconfigured(_)));
        assert!(error.to_string().contains("standup_authors"), "{error}");
    }

    /// git ORs repeated `--author` flags but treats a single value as a *basic*
    /// regex, where `|` is a literal — so a joined pattern matches nothing.
    #[test]
    fn every_author_gets_its_own_flag() {
        let filter = AuthorFilter::from_parts(&["a@x.invalid".into(), "b@x.invalid".into()], &[])
            .expect("resolve");
        assert_eq!(
            filter.args(),
            vec!["--author=a@x.invalid", "--author=b@x.invalid"]
        );
    }

    #[test]
    fn clean_authors_trims_drops_blanks_and_dedupes() {
        let raw = [
            "  dev@x.invalid ".to_string(),
            String::new(),
            "dev@x.invalid".to_string(),
            "   ".to_string(),
            "other@x.invalid".to_string(),
        ];
        assert_eq!(
            clean_authors(&raw),
            vec!["dev@x.invalid", "other@x.invalid"]
        );
    }

    // -- refs / parsing / rendering ----------------------------------------

    #[test]
    fn refs_args_defaults_to_all_and_splits_multiple_tokens() {
        assert_eq!(refs_args(""), vec!["--all"]);
        assert_eq!(refs_args("   \t "), vec!["--all"]);
        assert_eq!(refs_args("--branches"), vec!["--branches"]);
        assert_eq!(
            refs_args("--branches  --tags"),
            vec!["--branches", "--tags"]
        );
    }

    #[test]
    fn parse_subjects_reads_hash_day_and_subject() {
        let out = "abc123|2026-08-03|fix(checkout): FIF-201 rounding\n\
                   def456|2026-08-04|feat: FIF-202 retries";
        let commits = parse_subjects(out);
        assert_eq!(commits.len(), 2);
        assert_eq!(commits[0].day, "2026-08-03");
        assert_eq!(commits[1].subject, "feat: FIF-202 retries");
    }

    #[test]
    fn parse_subjects_ignores_blank_and_malformed_lines() {
        assert!(parse_subjects("").is_empty());
        assert!(parse_subjects("\n\n").is_empty());
        assert!(parse_subjects("no-pipes-here").is_empty());
    }

    #[test]
    fn a_section_header_lists_every_ticket_its_commits_mention() {
        let commits = vec![
            Commit {
                hash: "a".into(),
                day: "2026-08-03".into(),
                subject: "fix: FIF-201 and FIF-207 rounding".into(),
            },
            Commit {
                hash: "b".into(),
                day: "2026-08-03".into(),
                subject: "chore: FIF-201 follow-up".into(),
            },
        ];
        let section = render_section("acme-api", &commits);
        assert!(
            section.starts_with("### repo: acme-api / tickets: FIF-201, FIF-207 / commits (2):"),
            "a second ticket in one subject used to be dropped: {section}"
        );
        assert!(section.contains("- (FIF-201) fix: FIF-201 and FIF-207 rounding"));
    }

    #[test]
    fn a_section_without_tickets_says_none() {
        let commits = vec![Commit {
            hash: "a".into(),
            day: "2026-08-03".into(),
            subject: "chore: bump deps".into(),
        }];
        let section = render_section("acme-api", &commits);
        assert!(section.contains("tickets: (none)"), "{section}");
        assert!(section.contains("- (-) chore: bump deps"), "{section}");
    }

    #[test]
    fn dedup_preserves_first_seen_order() {
        let days = [
            "2026-08-03".to_string(),
            "2026-08-03".into(),
            "2026-08-01".into(),
        ];
        assert_eq!(dedup(&days), vec!["2026-08-03", "2026-08-01"]);
    }
}
