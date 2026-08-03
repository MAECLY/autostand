//! Run lock (mkdir-based with PID + 10min stale timeout).
//! See `docs/architecture/04-state-machine.md`.

use std::path::Path;

/// Acquire a lock at `lock_dir` by creating it + writing PID.
/// Returns Ok if acquired; Err if already held and not stale.
pub fn acquire(lock_dir: &Path) -> Result<(), std::io::Error> {
    std::fs::create_dir_all(lock_dir.parent().unwrap_or(Path::new(".")))?;
    match std::fs::create_dir(lock_dir) {
        Ok(()) => {
            std::fs::write(lock_dir.join("pid"), std::process::id().to_string())?;
            Ok(())
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            if is_stale(lock_dir) {
                // Reclaim stale lock
                let _ = std::fs::remove_dir_all(lock_dir);
                std::fs::create_dir(lock_dir)?;
                std::fs::write(lock_dir.join("pid"), std::process::id().to_string())?;
                Ok(())
            } else {
                Err(e)
            }
        }
        Err(e) => Err(e),
    }
}

/// Release the lock by removing the directory.
pub fn release(lock_dir: &Path) {
    let _ = std::fs::remove_dir_all(lock_dir);
}

/// Check if the lock is stale (older than 10 minutes).
fn is_stale(lock_dir: &Path) -> bool {
    use std::time::SystemTime;
    let Ok(metadata) = std::fs::metadata(lock_dir) else {
        return true;
    };
    let Ok(modified) = metadata.modified() else {
        return true;
    };
    let elapsed = SystemTime::now()
        .duration_since(modified)
        .unwrap_or_default();
    elapsed.as_secs() > 600
}
