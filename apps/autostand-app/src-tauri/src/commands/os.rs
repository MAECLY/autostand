//! OS shell integration IPC commands.
//!
//! See `docs/tauri/02-ipc-contracts.md` row `open_in_file_manager`.

use std::path::{Path, PathBuf};

use crate::error::AppError;

/// Validate `raw` as a directory that may be handed to the OS shell.
///
/// The guard runs in the backend rather than the UI because `open_path`
/// delegates to the platform shell and the shell is not fussy: a relative or
/// blank string resolves against the app's working directory (some arbitrary
/// folder the user never pointed at), and a *file* path launches whichever
/// application owns that extension instead of a window the user can browse.
/// Both are surprising enough to be worth an error code instead of a launch.
pub(crate) fn ensure_openable_dir(raw: &str) -> Result<PathBuf, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(AppError::Invalid("path is empty".into()));
    }

    let path = Path::new(trimmed);
    if !path.is_absolute() {
        return Err(AppError::Invalid(format!(
            "path is not absolute: {trimmed}"
        )));
    }

    let metadata = std::fs::metadata(path)
        .map_err(|e| AppError::NotFound(format!("path does not exist: {trimmed} ({e})")))?;
    if !metadata.is_dir() {
        return Err(AppError::Invalid(format!(
            "path is not a directory: {trimmed}"
        )));
    }

    Ok(path.to_path_buf())
}

/// Open a directory in the OS file manager (Finder, Explorer, `xdg-open`).
#[tauri::command]
pub async fn open_in_file_manager(path: String) -> Result<(), AppError> {
    let dir = ensure_openable_dir(&path)?;
    tracing::info!(path = %dir.display(), "opening directory in the OS file manager");
    tauri_plugin_opener::open_path(&dir, None::<&str>)
        .map_err(|e| AppError::Io(format!("open in file manager: {e}")))
}

#[cfg(test)]
mod tests {
    use super::ensure_openable_dir;
    use crate::error::AppError;

    /// Serialized `code` for an error, as the frontend sees it.
    fn code(err: &AppError) -> String {
        serde_json::to_value(err).expect("AppError serializes")["code"]
            .as_str()
            .expect("code is a string")
            .to_string()
    }

    #[test]
    fn accepts_an_existing_absolute_directory() {
        let dir = tempfile::tempdir().expect("tempdir");
        let resolved = ensure_openable_dir(&dir.path().to_string_lossy()).expect("existing dir");
        assert_eq!(resolved, dir.path());
    }

    #[test]
    fn trims_surrounding_whitespace_before_resolving() {
        let dir = tempfile::tempdir().expect("tempdir");
        let padded = format!("  {}\n", dir.path().display());
        assert_eq!(
            ensure_openable_dir(&padded).expect("padded dir"),
            dir.path()
        );
    }

    #[test]
    fn rejects_blank_input_as_invalid() {
        for raw in ["", "   ", "\t\n"] {
            let err = ensure_openable_dir(raw).expect_err("blank input");
            assert_eq!(code(&err), "invalid", "input {raw:?} must be invalid");
        }
    }

    #[test]
    fn rejects_a_relative_path_as_invalid() {
        for raw in ["Documents/Github", "./dailies", "~/Sync"] {
            let err = ensure_openable_dir(raw).expect_err("relative path");
            assert_eq!(code(&err), "invalid", "input {raw:?} must be invalid");
        }
    }

    #[test]
    fn reports_a_missing_directory_as_not_found() {
        let dir = tempfile::tempdir().expect("tempdir");
        let missing = dir.path().join("does-not-exist");
        let err = ensure_openable_dir(&missing.to_string_lossy()).expect_err("missing dir");
        assert_eq!(code(&err), "not_found");
    }

    #[test]
    fn rejects_a_file_as_invalid() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("2026-08-13.md");
        std::fs::write(&file, "# standup").expect("write file");

        let err = ensure_openable_dir(&file.to_string_lossy()).expect_err("regular file");
        assert_eq!(code(&err), "invalid");
    }

    #[test]
    fn error_messages_quote_the_offending_path() {
        let err = ensure_openable_dir("relative/dir").expect_err("relative path");
        assert!(
            err.to_string().contains("relative/dir"),
            "message was {err}"
        );
    }
}
