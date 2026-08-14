//! Single-run lock: `mkdir` + PID + 10-minute stale reclaim.
//!
//! See `docs/specs/pipeline.md` § Concurrency and `docs/architecture/04-state-machine.md`.
//!
//! Two compiles must never race on the same machine, so the lock is a directory:
//! `mkdir` is atomic on macOS, Linux and Windows, which removes the
//! check-then-create race an `O_CREAT` file lock would have.
//!
//! A held lock is **stale** when either
//!
//! 1. the PID recorded inside it is no longer running — immediately, even inside
//!    the timeout, because a crashed compile can never come back to release it; or
//! 2. it is older than [`STALE_TIMEOUT`] (10 minutes) — the fallback for platforms
//!    where the liveness probe cannot answer, and the escape hatch for a wedged
//!    process that is alive but stuck.
//!
//! Acquisition never queues: per the spec a `trigger` while another run holds the
//! lock fails fast with [`std::io::ErrorKind::AlreadyExists`], which the app maps
//! to `AppError::Lock`.

use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(any(unix, windows))]
use autostand_runlog::proc::{run_process, ProcSpec, StreamPolicy};

/// Step the liveness probe would be filed under — it never is; see
/// [`pid_is_running`].
#[cfg(any(unix, windows))]
const LOCK_STEP: &str = "compile";

/// A liveness probe that has not answered in this long is not going to.
#[cfg(any(unix, windows))]
const PROBE_TIMEOUT: Duration = Duration::from_secs(10);

/// Metadata file written inside the lock directory: PID on line 1, acquisition
/// time (Unix seconds) on line 2.
const PID_FILE: &str = "pid";

/// How long a lock may live before it is considered stale regardless of whether
/// its owner is still running. Matches `docs/specs/pipeline.md` (`LOCK_TIMEOUT`).
pub const STALE_TIMEOUT: Duration = Duration::from_secs(600);

/// RAII handle for a held run lock; releases the lock when dropped.
///
/// WHY a guard: a compile that panics, returns early, or is cancelled mid-`await`
/// would otherwise leave the lock directory behind and wedge every later run for
/// [`STALE_TIMEOUT`]. `Drop` runs on all of those paths, so the lock is released
/// as soon as the run stops owning it.
#[derive(Debug)]
pub struct LockGuard {
    /// Directory this guard owns and will remove on drop.
    path: PathBuf,
}

impl LockGuard {
    /// Path of the lock directory owned by this guard.
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        release(&self.path);
        tracing::debug!(lock = %self.path.display(), "run lock released");
    }
}

/// Acquire the run lock at `lock_dir`, stealing it if the current holder is stale.
///
/// Returns a [`LockGuard`] that releases the lock on drop. Fails immediately with
/// [`io::ErrorKind::AlreadyExists`] when a live run holds the lock — the scheduler
/// does not queue.
#[tracing::instrument(level = "debug", skip_all, fields(lock = %lock_dir.display()))]
pub async fn acquire(lock_dir: &Path) -> io::Result<LockGuard> {
    if let Some(parent) = lock_dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    match std::fs::create_dir(lock_dir) {
        Ok(()) => finish_acquire(lock_dir),
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            let Some(reason) = staleness(lock_dir).await else {
                tracing::debug!("run lock held by a live process; refusing");
                return Err(e);
            };
            tracing::warn!(reason, "stealing stale run lock");
            release(lock_dir);
            // If a competing process won the race to re-create the directory we
            // get `AlreadyExists` again and fail closed rather than double-run.
            std::fs::create_dir(lock_dir)?;
            finish_acquire(lock_dir)
        }
        Err(e) => Err(e),
    }
}

/// Release the lock by removing its directory.
///
/// Kept as a free function for callers that only have the path (and used by
/// [`LockGuard::drop`]). Prefer holding the guard: it cannot be forgotten.
pub fn release(lock_dir: &Path) {
    let _ = std::fs::remove_dir_all(lock_dir);
}

/// Write the PID metadata into a freshly created lock directory.
///
/// The guard is built *before* the write so that a failure here drops it and
/// removes the directory, instead of leaving an owner-less lock behind.
fn finish_acquire(lock_dir: &Path) -> io::Result<LockGuard> {
    let guard = LockGuard {
        path: lock_dir.to_path_buf(),
    };
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let pid = std::process::id();
    std::fs::write(lock_dir.join(PID_FILE), format!("{pid}\n{now}\n"))?;
    tracing::info!(pid, "run lock acquired");
    Ok(guard)
}

/// Contents of the lock's `pid` file.
#[derive(Debug, Default)]
struct LockMeta {
    /// PID of the process that acquired the lock, if parseable.
    pid: Option<u32>,
    /// When the lock was acquired, if the file carries a timestamp line.
    acquired_at: Option<SystemTime>,
}

/// Read and parse the lock metadata; unreadable or malformed files yield defaults
/// so the time-based fallback still applies.
fn read_meta(lock_dir: &Path) -> LockMeta {
    let Ok(raw) = std::fs::read_to_string(lock_dir.join(PID_FILE)) else {
        return LockMeta::default();
    };
    let mut lines = raw.lines();
    let pid = lines.next().and_then(|l| l.trim().parse::<u32>().ok());
    let acquired_at = lines
        .next()
        .and_then(|l| l.trim().parse::<u64>().ok())
        .map(|secs| UNIX_EPOCH + Duration::from_secs(secs));
    LockMeta { pid, acquired_at }
}

/// Why the lock at `lock_dir` is stale, or `None` if it is still legitimately held.
async fn staleness(lock_dir: &Path) -> Option<&'static str> {
    let meta = read_meta(lock_dir);
    // Owner is gone: reclaim now, no need to wait out the timeout. When the owner
    // is alive — or the probe could not answer — only the time rule can steal it.
    if let Some(pid) = meta.pid {
        if pid_is_running(pid).await == Some(false) {
            return Some("dead-pid");
        }
    }
    match age(lock_dir, &meta) {
        None => Some("unknown-age"),
        Some(a) if a > STALE_TIMEOUT => Some("timeout"),
        Some(_) => None,
    }
}

/// How long the lock has been held, from its recorded timestamp or, failing that,
/// the lock directory's mtime. `None` means neither source was readable.
fn age(lock_dir: &Path, meta: &LockMeta) -> Option<Duration> {
    let stamp = match meta.acquired_at {
        Some(t) => t,
        None => std::fs::metadata(lock_dir)
            .and_then(|m| m.modified())
            .ok()?,
    };
    // A timestamp in the future (clock skew) counts as age zero, never as stale.
    Some(SystemTime::now().duration_since(stamp).unwrap_or_default())
}

/// Probe whether `pid` is still running.
///
/// `Some(true)`/`Some(false)` when the platform answered; `None` when no answer
/// could be obtained, so the caller falls back to the time-based rule.
///
/// WHY a subprocess: the workspace pulls in no `libc`/`sysinfo` dependency, so
/// `kill(pid, 0)` is not callable directly. `kill -0` is POSIX-mandated and
/// present on every unix target we ship to.
///
/// Caveat: `kill -0` also fails with `EPERM` for a live process owned by another
/// user, which is indistinguishable from `ESRCH` here. The lock lives under the
/// invoking user's own state directory, so a foreign owner is not a real case.
///
/// [`StreamPolicy::Silent`]: this runs while *acquiring* the run lock, before any
/// run exists to log into, and a `kill -0` line would tell the user nothing they
/// could act on. It still goes through the workspace's single spawner so the
/// kill/timeout policy stays in one file.
#[cfg(unix)]
async fn pid_is_running(pid: u32) -> Option<bool> {
    let output = run_process(
        ProcSpec::new("kill", LOCK_STEP)
            .args(["-0", &pid.to_string()])
            .timeout(PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .ok()?;
    Some(output.success)
}

/// Windows variant of the liveness probe.
///
/// `tasklist` exits 0 whether or not the filter matched, so the answer has to come
/// from stdout: a matching row contains the quoted PID, a miss prints an INFO
/// banner instead.
#[cfg(windows)]
async fn pid_is_running(pid: u32) -> Option<bool> {
    let output = run_process(
        ProcSpec::new("tasklist", LOCK_STEP)
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .timeout(PROBE_TIMEOUT)
            .stream(StreamPolicy::Silent),
    )
    .await
    .ok()?;
    if !output.success {
        return None;
    }
    let needle = format!("\"{pid}\"");
    Some(output.stdout.contains(needle.as_str()))
}

/// Fallback for targets that are neither unix nor windows: no probe, so the
/// time-based staleness rule is the only one that applies.
#[cfg(not(any(unix, windows)))]
#[allow(clippy::unused_async)]
async fn pid_is_running(_pid: u32) -> Option<bool> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Write a lock directory by hand with the given `pid` line and age.
    fn plant_lock(lock_dir: &Path, pid_line: &str, age_secs: u64) {
        std::fs::create_dir_all(lock_dir).expect("mkdir lock");
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("epoch")
            .as_secs()
            - age_secs;
        std::fs::write(lock_dir.join(PID_FILE), format!("{pid_line}\n{stamp}\n"))
            .expect("write pid");
    }

    #[tokio::test]
    async fn acquires_and_writes_pid() {
        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("state").join("lock");
        let guard = acquire(&lock).await.expect("acquire");
        assert!(lock.is_dir());
        assert_eq!(guard.path(), lock.as_path());
        let meta = read_meta(&lock);
        assert_eq!(meta.pid, Some(std::process::id()));
        assert!(meta.acquired_at.is_some());
    }

    #[tokio::test]
    async fn guard_releases_on_drop() {
        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("lock");
        {
            let _guard = acquire(&lock).await.expect("acquire");
            assert!(lock.is_dir());
        }
        assert!(!lock.exists(), "drop must remove the lock directory");
        // And the slot is immediately re-acquirable.
        drop(acquire(&lock).await.expect("re-acquire"));
    }

    #[tokio::test]
    async fn refuses_lock_held_by_live_pid() {
        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("lock");
        let _held = acquire(&lock).await.expect("acquire");
        // Our own PID is by definition alive and the lock is seconds old.
        let err = acquire(&lock).await.expect_err("second acquire must fail");
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert!(lock.is_dir(), "a refused acquire must not remove the lock");
    }

    #[tokio::test]
    async fn reclaims_lock_older_than_timeout() {
        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("lock");
        // Live PID (ours) but well past the 10-minute timeout.
        plant_lock(&lock, &std::process::id().to_string(), 3600);
        assert_eq!(staleness(&lock).await, Some("timeout"));
        let guard = acquire(&lock).await.expect("stale lock must be reclaimed");
        assert_eq!(read_meta(guard.path()).pid, Some(std::process::id()));
    }

    #[tokio::test]
    async fn unparseable_pid_falls_back_to_time() {
        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("lock");
        plant_lock(&lock, "not-a-pid", 5);
        assert_eq!(staleness(&lock).await, None, "fresh lock stays held");
        acquire(&lock)
            .await
            .expect_err("fresh lock must be refused");

        plant_lock(&lock, "not-a-pid", 3600);
        assert_eq!(staleness(&lock).await, Some("timeout"));
        drop(acquire(&lock).await.expect("old lock must be reclaimed"));
    }

    #[tokio::test]
    async fn missing_pid_file_uses_directory_mtime() {
        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("lock");
        std::fs::create_dir_all(&lock).expect("mkdir");
        // Freshly created directory: not stale, so the acquire is refused.
        assert_eq!(staleness(&lock).await, None);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn reclaims_lock_held_by_dead_pid_inside_timeout() {
        // A process we spawn and reap is guaranteed to be gone afterwards, which
        // gives us a realistic dead PID without touching the wider system.
        let child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 0")
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawn");
        let dead_pid = child.id();
        let mut child = child;
        child.wait().expect("reap");

        let tmp = tempfile::tempdir().expect("tmp");
        let lock = tmp.path().join("lock");
        // Only 5 seconds old: the time rule alone would keep it held.
        plant_lock(&lock, &dead_pid.to_string(), 5);
        assert_eq!(staleness(&lock).await, Some("dead-pid"));
        let guard = acquire(&lock)
            .await
            .expect("dead-pid lock must be reclaimed");
        assert_eq!(read_meta(guard.path()).pid, Some(std::process::id()));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn probe_reports_self_as_running() {
        assert_eq!(pid_is_running(std::process::id()).await, Some(true));
    }
}
