//! Read-only lookups in a VS Code-style `state.vscdb` key/value store.
//!
//! Cursor and Devin are both VS Code forks, and both leave their signed-in
//! session in the editor's `globalStorage` `SQLite` database under a single
//! `ItemTable(key, value)` row. `OpenUsage` reaches it by spawning
//! `/usr/bin/sqlite3 -readonly`; this crate already links `rusqlite`, so the
//! same read happens in-process with `SQLITE_OPEN_READ_ONLY` — no subprocess, no
//! shell quoting, and the key is bound as a parameter instead of being escaped
//! into a statement.
//!
//! Read-only is not a convention here, it is the open flag: this module has no
//! write path, and `sqlite3` would refuse one anyway. An expired session found
//! in here is reported, never rotated.

use std::path::{Path, PathBuf};

use rusqlite::types::ValueRef;
use rusqlite::{Connection, OpenFlags};

use crate::usage::model::UsageError;

/// The one table every VS Code fork keeps its global state in.
const ITEM_TABLE_QUERY: &str = "SELECT value FROM ItemTable WHERE key = ?1 LIMIT 1";

/// Candidate `state.vscdb` paths for an editor, in the order they should be tried.
///
/// `app` is the editor's own directory name (`"Cursor"`, `"Devin"`). Each
/// platform stores it somewhere different, so all plausible locations for the
/// host are returned and the caller takes the first that exists — which keeps a
/// provider's discovery code identical on macOS, Windows and Linux.
#[must_use]
pub fn state_db_paths(app: &str) -> Vec<PathBuf> {
    let relative = format!("{app}/User/globalStorage/state.vscdb");
    let mut candidates = Vec::new();

    if cfg!(target_os = "macos") {
        if let Some(path) =
            super::files::home_relative(&format!("Library/Application Support/{relative}"))
        {
            candidates.push(path);
        }
    }
    if cfg!(target_os = "windows") {
        if let Some(appdata) = super::files::env_text("APPDATA") {
            candidates.push(PathBuf::from(appdata).join(&relative));
        }
    }
    // The XDG layout is where a Linux build lands, and it is also where a
    // portable install can end up on any host, so it is always a candidate.
    if let Some(config_home) = super::files::env_text("XDG_CONFIG_HOME") {
        candidates.push(PathBuf::from(config_home).join(&relative));
    } else if let Some(path) = super::files::home_relative(&format!(".config/{relative}")) {
        candidates.push(path);
    }

    candidates
}

/// Read one `ItemTable` value, or `Ok(None)` when the key is absent.
///
/// A missing database file is also `Ok(None)` — the editor is simply not
/// installed. Anything else (a locked database, a denied read, a schema that no
/// longer has `ItemTable`) is [`UsageError::CredentialStoreUnavailable`]: a
/// failed lookup is not proof that the user signed out, and reporting it as one
/// would send them to re-authenticate for nothing.
///
/// Blocking. Call it from [`tokio::task::spawn_blocking`], never on the runtime
/// thread.
pub fn read_item(path: &Path, key: &str) -> Result<Option<String>, UsageError> {
    if !path.is_file() {
        return Ok(None);
    }
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let connection = Connection::open_with_flags(path, flags)
        .map_err(|_| UsageError::CredentialStoreUnavailable)?;
    let mut statement = connection
        .prepare(ITEM_TABLE_QUERY)
        .map_err(|_| UsageError::CredentialStoreUnavailable)?;
    let mut rows = statement
        .query([key])
        .map_err(|_| UsageError::CredentialStoreUnavailable)?;
    let Some(row) = rows
        .next()
        .map_err(|_| UsageError::CredentialStoreUnavailable)?
    else {
        return Ok(None);
    };
    let cell = row
        .get_ref(0)
        .map_err(|_| UsageError::CredentialStoreUnavailable)?;
    Ok(cell_text(cell))
}

/// Read one `ItemTable` value from the first candidate database that exists.
///
/// Blocking, for the same reason as [`read_item`].
pub fn read_item_from_any(candidates: &[PathBuf], key: &str) -> Result<Option<String>, UsageError> {
    for path in candidates {
        if let Some(value) = read_item(path, key)? {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

/// A cell as trimmed, non-empty text. `TEXT` and `BLOB` both occur in the wild —
/// VS Code writes strings, some forks write UTF-8 blobs — and every other type
/// is not a credential.
fn cell_text(value: ValueRef<'_>) -> Option<String> {
    let text = match value {
        ValueRef::Text(bytes) | ValueRef::Blob(bytes) => {
            String::from_utf8_lossy(bytes).trim().to_string()
        }
        _ => return None,
    };
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::{read_item, read_item_from_any, state_db_paths};
    use crate::usage::model::UsageError;
    use rusqlite::Connection;
    use std::path::{Path, PathBuf};

    fn write_state_db(path: &Path, rows: &[(&str, &str)]) {
        let connection = Connection::open(path).unwrap();
        connection
            .execute(
                "CREATE TABLE ItemTable (key TEXT PRIMARY KEY, value BLOB)",
                [],
            )
            .unwrap();
        for (key, value) in rows {
            connection
                .execute(
                    "INSERT INTO ItemTable (key, value) VALUES (?1, ?2)",
                    [key, value],
                )
                .unwrap();
        }
    }

    #[test]
    fn reads_a_stored_item() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        write_state_db(
            &path,
            &[("cursorAuth/accessToken", "  header.payload.sig  ")],
        );

        assert_eq!(
            read_item(&path, "cursorAuth/accessToken").unwrap(),
            Some("header.payload.sig".to_string())
        );
    }

    #[test]
    fn an_absent_key_and_an_absent_file_both_read_as_no_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        write_state_db(&path, &[("other/key", "value")]);

        assert_eq!(read_item(&path, "cursorAuth/accessToken").unwrap(), None);
        assert_eq!(
            read_item(&dir.path().join("missing.vscdb"), "any").unwrap(),
            None
        );
    }

    #[test]
    fn an_empty_value_is_missing_rather_than_an_empty_credential() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        write_state_db(&path, &[("cursorAuth/accessToken", "   ")]);

        assert_eq!(read_item(&path, "cursorAuth/accessToken").unwrap(), None);
    }

    #[test]
    fn a_database_without_the_expected_table_is_a_store_failure_not_a_logout() {
        // Reporting "not logged in" for a schema change would tell the user to
        // sign in again for a problem signing in cannot fix.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("state.vscdb");
        Connection::open(&path)
            .unwrap()
            .execute("CREATE TABLE Unrelated (key TEXT)", [])
            .unwrap();

        assert_eq!(
            read_item(&path, "cursorAuth/accessToken"),
            Err(UsageError::CredentialStoreUnavailable)
        );
    }

    #[test]
    fn the_first_candidate_that_has_the_key_wins() {
        let dir = tempfile::tempdir().unwrap();
        let first = dir.path().join("first.vscdb");
        let second = dir.path().join("second.vscdb");
        write_state_db(&first, &[("other", "x")]);
        write_state_db(&second, &[("wanted", "found")]);

        let candidates = vec![dir.path().join("missing.vscdb"), first, second];
        assert_eq!(
            read_item_from_any(&candidates, "wanted").unwrap(),
            Some("found".to_string())
        );
    }

    #[test]
    fn candidate_paths_are_editor_scoped_and_never_empty_on_a_host_with_a_home() {
        let candidates = state_db_paths("Cursor");
        assert!(!candidates.is_empty());
        for path in &candidates {
            assert!(
                path.ends_with(PathBuf::from("User/globalStorage/state.vscdb")),
                "{path:?}"
            );
            assert!(path.to_string_lossy().contains("Cursor"), "{path:?}");
        }
    }
}
