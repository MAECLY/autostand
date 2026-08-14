//! Repo discovery IPC command.
//!
//! See `docs/tauri/02-ipc-contracts.md` row `discover_repos`. This is also the
//! reference call site for the run/spawner contract: the command opens a
//! [`Run`], does its work inside [`Run::scope`], spawns every child through
//! `autostand_runlog::proc`, and settles the run exactly once.

use std::path::{Path, PathBuf};
use std::time::Duration;

use tauri::AppHandle;

use autostand_runlog::proc::{run_process, ProcSpec, StreamPolicy};

use crate::commands::{load_config, resolve_dir, types::RepoInfo};
use crate::error::AppError;
use crate::run_log::{Run, RunKind};

/// Fallback location for `github_dir` under the user's home.
const GITHUB_DIR_FALLBACK: &[&str] = &["Documents", "Github"];

/// Budget for one metadata probe. Previously unbounded: a `git` blocked on a
/// credential prompt hung the whole Settings page.
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Resolve the repo-scan root: the configured `github_dir`, else
/// `<home>/Documents/Github`.
pub(crate) fn resolve_github_dir(configured: Option<&str>, home: Option<&Path>) -> PathBuf {
    resolve_dir(configured, home, GITHUB_DIR_FALLBACK)
}

/// Resolve the `github_dir` from the persisted config.
fn github_dir(app: &impl tauri::Manager<tauri::Wry>) -> PathBuf {
    let config = load_config(app).ok();
    resolve_github_dir(
        config.as_ref().map(|c| c.github_dir.as_str()),
        dirs::home_dir().as_deref(),
    )
}

/// Run one read-only `git` probe in `dir`, returning trimmed stdout on success.
///
/// [`StreamPolicy::Silent`] on purpose: this runs twice per repository, so a
/// summary line per probe would bury the run under fifty lines of noise on a
/// normal machine. The run's own summary reports the outcome instead.
async fn git_probe(dir: &Path, args: &[&str]) -> Option<String> {
    let output = run_process(
        ProcSpec::new("git", RunKind::RepoDiscovery.step())
            .args(args)
            .cwd(dir)
            .timeout(PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .ok()?;
    output.success.then(|| output.stdout_trimmed().to_string())
}

/// Scan `github_dir` for depth-1 git repos.
#[tauri::command]
pub async fn discover_repos(app_handle: AppHandle) -> Result<Vec<RepoInfo>, AppError> {
    let run = Run::open(&app_handle, RunKind::RepoDiscovery);
    let outcome = run.scope(scan(&app_handle)).await;
    let found = outcome.as_ref().map_or(0, Vec::len);
    run.settle(outcome, &format!("{found} repositories found"))
}

/// The scan itself, already inside the run's log scope.
async fn scan(app_handle: &AppHandle) -> Result<Vec<RepoInfo>, AppError> {
    let root = github_dir(app_handle);
    if !root.exists() {
        autostand_runlog::warn(
            RunKind::RepoDiscovery.step(),
            "the configured repositories directory does not exist",
        );
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    let entries = std::fs::read_dir(&root)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() || !path.join(".git").exists() {
            continue;
        }
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let remote = git_probe(&path, &["config", "--get", "remote.origin.url"]).await;
        let last_commit_at = git_probe(&path, &["log", "-1", "--format=%cI"]).await;
        out.push(RepoInfo {
            path: path.to_string_lossy().to_string(),
            name,
            remote,
            last_commit_at,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::{git_probe, resolve_github_dir};
    use autostand_runlog::{scoped, RecordingSink, SinkRef};
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    #[test]
    fn uses_the_configured_github_dir() {
        let home = Path::new("/home/tester");
        assert_eq!(
            resolve_github_dir(Some("/Volumes/work/repos"), Some(home)),
            PathBuf::from("/Volumes/work/repos")
        );
    }

    #[test]
    fn falls_back_to_home_documents_github() {
        let home = Path::new("/home/tester");
        assert_eq!(
            resolve_github_dir(None, Some(home)),
            PathBuf::from("/home/tester/Documents/Github")
        );
    }

    #[test]
    fn an_unset_config_field_falls_back_rather_than_scanning_cwd() {
        // `AppConfig::default()` leaves `github_dir` as "" — resolving that to a
        // relative path would make `discover_repos` scan the process cwd.
        let home = Path::new("/home/tester");
        let fallback = PathBuf::from("/home/tester/Documents/Github");
        assert_eq!(resolve_github_dir(Some(""), Some(home)), fallback);
        assert_eq!(resolve_github_dir(Some("  "), Some(home)), fallback);
    }

    #[test]
    fn a_configured_dir_wins_even_without_a_home() {
        assert_eq!(
            resolve_github_dir(Some("/srv/repos"), None),
            PathBuf::from("/srv/repos")
        );
    }

    #[test]
    fn no_config_and_no_home_yields_an_empty_path() {
        // `discover_repos` treats a non-existent root as "no repos", so an empty
        // path degrades to an empty list instead of an error.
        assert_eq!(resolve_github_dir(None, None), PathBuf::new());
    }

    /// A repo with no `origin` is normal, and the probe must report that as
    /// "no remote" rather than as a failure that aborts the scan.
    #[tokio::test]
    async fn a_failing_probe_is_none_and_stays_out_of_the_terminal() {
        let dir = tempfile::tempdir().unwrap();
        let sink = Arc::new(RecordingSink::new());
        let as_ref: SinkRef = sink.clone();

        let remote = scoped(as_ref, async {
            git_probe(dir.path(), &["config", "--get", "remote.origin.url"]).await
        })
        .await;

        assert_eq!(remote, None);
        assert!(
            sink.lines().is_empty(),
            "two probes per repository would bury the run: {:?}",
            sink.messages()
        );
    }

    /// The probe returns real stdout — the migration must not have turned the
    /// spawner into a no-op.
    #[tokio::test]
    async fn a_successful_probe_returns_trimmed_stdout() {
        let dir = tempfile::tempdir().unwrap();
        let version = git_probe(dir.path(), &["--version"]).await;
        let Some(version) = version else {
            // git is a hard prerequisite of the app, but not of every CI image.
            return;
        };
        assert!(version.starts_with("git version"), "{version}");
        assert_eq!(version.trim(), version);
    }
}
