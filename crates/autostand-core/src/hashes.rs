//! Input hashing for the dirty check.
//!
//! See steps (j) `dirty_check` and (s) `persist_hash` of `docs/specs/pipeline.md`.
//!
//! Step (j) hashes the *redacted* pipeline inputs and compares the digest with the
//! one persisted by the previous clean LLM render; equal digests mean the standup
//! file is already up to date and the compile can skip straight to `Skip`.
//!
//! The digest is SHA-256, **not** `std::collections::hash_map::DefaultHasher`:
//! `DefaultHasher`'s algorithm is explicitly not guaranteed stable across Rust
//! releases (nor is it seed-stable across platforms), so a persisted digest would
//! silently stop matching after a toolchain bump and every run would look dirty.

use crate::Result;
use chrono::NaiveDate;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

/// Directory (relative to the state dir) holding one hash file per `(F, host)`.
const HASHES_SUBDIR: &str = "hashes";

/// Hash the pipeline inputs into a lowercase SHA-256 hex digest.
///
/// Stable across runs, processes, platforms and Rust releases — that is the whole
/// point, since the digest is persisted to disk and compared on the next run.
///
/// Each part is length-prefixed before being fed to the hasher so that the part
/// boundaries are part of the digest: `["ab", "c"]` and `["a", "bc"]` must not
/// collide, otherwise moving text between the facts and notes sections of the
/// inputs would look unchanged.
pub fn input_hash(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        let len = u64::try_from(part.len()).unwrap_or(u64::MAX);
        hasher.update(len.to_le_bytes());
        hasher.update(part.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

/// Path of the persisted hash for filing date `date` and `host`:
/// `<state_dir>/hashes/<F>-<host>.txt`.
pub fn hash_path(state_dir: &Path, date: NaiveDate, host: &str) -> PathBuf {
    state_dir
        .join(HASHES_SUBDIR)
        .join(format!("{}-{host}.txt", date.format("%Y-%m-%d")))
}

/// Read the persisted hash for `(date, host)`, or `None` when absent/unreadable.
///
/// A missing, empty or unreadable file yields `None`, which callers must treat as
/// "dirty" — never as "unchanged".
pub fn read_hash(state_dir: &Path, date: NaiveDate, host: &str) -> Option<String> {
    let raw = std::fs::read_to_string(hash_path(state_dir, date, host)).ok()?;
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Persist `hash` for `(date, host)` — step (s).
///
/// Creates `<state_dir>/hashes/` if needed, writes atomically (temp → fsync →
/// rename → fsync parent) via [`crate::fileops::write_atomic`], then tightens the
/// mode to `0600` on unix: the digest lives under the same state dir as the audit
/// sidecars and inherits their permission posture.
///
/// Only call this after a *clean* LLM render; persisting after a deterministic
/// fallback would make the next run skip a render that never actually happened.
#[tracing::instrument(skip_all, fields(date = %date, host = host))]
pub fn persist_hash(state_dir: &Path, date: NaiveDate, host: &str, hash: &str) -> Result<()> {
    let path = hash_path(state_dir, date, host);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::fileops::write_atomic(&path, &format!("{hash}\n"))?;
    set_mode_0600(&path)?;
    tracing::info!(path = %path.display(), "persisted input hash");
    Ok(())
}

/// Whether the persisted hash for `(date, host)` equals `hash` — step (j).
///
/// Returns `false` when no hash has been persisted yet, so a first run (or a run
/// after the state dir was cleared) always recompiles.
pub fn is_unchanged(state_dir: &Path, date: NaiveDate, host: &str, hash: &str) -> bool {
    read_hash(state_dir, date, host).is_some_and(|stored| stored == hash)
}

#[cfg(unix)]
fn set_mode_0600(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
#[allow(clippy::unnecessary_wraps)]
fn set_mode_0600(_path: &Path) -> Result<()> {
    // Windows has no POSIX mode bits; the state dir itself is user-scoped.
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const HOST: &str = "test-host";

    fn f() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 3).expect("valid date")
    }

    #[test]
    fn empty_input_matches_the_sha256_empty_vector() {
        // Proves the digest really is unsalted SHA-256, so it is reproducible by
        // anything else that hashes the same bytes.
        assert_eq!(
            input_hash(&[]),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn digest_is_lowercase_hex_of_fixed_width() {
        let hash = input_hash(&["facts", "notes"]);
        assert_eq!(hash.len(), 64);
        assert!(hash
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn same_inputs_hash_identically() {
        assert_eq!(
            input_hash(&["repo-a: FIF-1", "note: shipped"]),
            input_hash(&["repo-a: FIF-1", "note: shipped"])
        );
    }

    #[test]
    fn changed_input_changes_the_hash() {
        assert_ne!(
            input_hash(&["repo-a: FIF-1", "note: shipped"]),
            input_hash(&["repo-a: FIF-2", "note: shipped"])
        );
    }

    #[test]
    fn part_boundaries_are_part_of_the_digest() {
        assert_ne!(input_hash(&["ab", "c"]), input_hash(&["a", "bc"]));
        assert_ne!(input_hash(&["ab"]), input_hash(&["a", "b"]));
    }

    #[test]
    fn hash_path_is_state_dir_hashes_date_host_txt() {
        let path = hash_path(Path::new("/state"), f(), HOST);
        assert_eq!(path, Path::new("/state/hashes/2026-08-03-test-host.txt"));
    }

    #[test]
    fn missing_file_is_dirty() {
        let dir = TempDir::new().expect("tempdir");
        assert!(read_hash(dir.path(), f(), HOST).is_none());
        assert!(!is_unchanged(dir.path(), f(), HOST, &input_hash(&["x"])));
    }

    #[test]
    fn persisted_hash_is_detected_as_unchanged() {
        let dir = TempDir::new().expect("tempdir");
        let hash = input_hash(&["facts", "notes"]);
        persist_hash(dir.path(), f(), HOST, &hash).expect("persist");

        assert_eq!(read_hash(dir.path(), f(), HOST).as_deref(), Some(&*hash));
        assert!(is_unchanged(dir.path(), f(), HOST, &hash));
    }

    #[test]
    fn changed_inputs_are_detected_as_dirty() {
        let dir = TempDir::new().expect("tempdir");
        persist_hash(dir.path(), f(), HOST, &input_hash(&["facts", "notes"])).expect("persist");

        let next = input_hash(&["facts", "notes + one more bullet"]);
        assert!(!is_unchanged(dir.path(), f(), HOST, &next));
    }

    #[test]
    fn hashes_are_scoped_per_date_and_per_host() {
        let dir = TempDir::new().expect("tempdir");
        let hash = input_hash(&["facts"]);
        persist_hash(dir.path(), f(), HOST, &hash).expect("persist");

        let other_day = NaiveDate::from_ymd_opt(2026, 8, 4).expect("valid date");
        assert!(!is_unchanged(dir.path(), other_day, HOST, &hash));
        assert!(!is_unchanged(dir.path(), f(), "other-host", &hash));
    }

    #[test]
    fn persist_overwrites_a_previous_hash() {
        let dir = TempDir::new().expect("tempdir");
        let first = input_hash(&["v1"]);
        let second = input_hash(&["v2"]);
        persist_hash(dir.path(), f(), HOST, &first).expect("persist 1");
        persist_hash(dir.path(), f(), HOST, &second).expect("persist 2");

        assert!(is_unchanged(dir.path(), f(), HOST, &second));
        assert!(!is_unchanged(dir.path(), f(), HOST, &first));
    }

    #[test]
    fn empty_hash_file_is_dirty() {
        let dir = TempDir::new().expect("tempdir");
        let path = hash_path(dir.path(), f(), HOST);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(&path, "   \n").expect("write");

        assert!(read_hash(dir.path(), f(), HOST).is_none());
        assert!(!is_unchanged(dir.path(), f(), HOST, ""));
    }

    #[test]
    fn persist_leaves_no_temp_file_behind() {
        let dir = TempDir::new().expect("tempdir");
        persist_hash(dir.path(), f(), HOST, &input_hash(&["facts"])).expect("persist");

        let entries: Vec<_> = std::fs::read_dir(dir.path().join(HASHES_SUBDIR))
            .expect("read_dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(entries.len(), 1, "unexpected leftovers: {entries:?}");
    }

    #[cfg(unix)]
    #[test]
    fn persisted_hash_is_mode_0600() {
        use std::os::unix::fs::PermissionsExt;

        let dir = TempDir::new().expect("tempdir");
        persist_hash(dir.path(), f(), HOST, &input_hash(&["facts"])).expect("persist");

        let path = hash_path(dir.path(), f(), HOST);
        let mode = std::fs::metadata(&path)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
}
