//! Shared helper: detect a `CLI` binary on `PATH` and get its version.

use super::CliInfo;
use autostand_runlog::proc::{run_process, ProcSpec, StreamPolicy};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Step a detection probe is filed under, matching `RunKind::CliDetect`.
const DETECT_STEP: &str = "cli_detect";

/// A `--version` that has not answered in this long is not going to.
///
/// Previously unbounded: a provider CLI blocked on an interactive login prompt
/// hung the Settings page forever.
const DETECT_TIMEOUT: Duration = Duration::from_secs(15);

/// Detect a CLI binary by name on PATH; return path + version if found.
pub async fn detect_cli_binary(name: &str) -> Option<CliInfo> {
    let path = which(name)?;
    detect_cli_at(&path).await
}

/// Detect a CLI at an explicit path (returns path + version).
pub async fn detect_cli_at(path: &Path) -> Option<CliInfo> {
    let version = get_version(path).await.unwrap_or_default();
    Some(CliInfo {
        path: path.to_path_buf(),
        version,
    })
}

fn which(name: &str) -> Option<PathBuf> {
    // Not a plain `PATH` walk: an installed app is launched by Finder, the Dock
    // or a `.desktop` entry, and inherits none of the directories the provider
    // CLIs install into. See `autostand_runlog::which`.
    autostand_runlog::which::find_program(name)
}

/// Ask a detected binary for its version.
///
/// [`StreamPolicy::Silent`]: detection sweeps every provider binary the app
/// knows about (and the Settings page re-runs it on every visit), so a summary
/// line per probe would be pure noise. The command that asked for the detection
/// reports the result.
async fn get_version(path: &Path) -> Option<String> {
    // `AUTOSTAND_RENDER=1` on every provider-binary spawn, not just renders
    // (`AGENTS.md`: "anti-recursion env set on CLI subprocess calls"). A
    // `--version` probe writes no session today, but a provider CLI that
    // starts recording one would turn detection into work autostand ingests.
    let out = run_process(
        ProcSpec::new(path, DETECT_STEP)
            .arg("--version")
            .env("AUTOSTAND_RENDER", "1")
            .timeout(DETECT_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .ok()?;
    out.success.then(|| out.stdout_trimmed().to_string())
}
