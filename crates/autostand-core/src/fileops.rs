//! Atomic file operations for standup `.md` files.
//!
//! See `docs/specs/standup-file-format.md` and App Script `fileops.py`.
//! Provides write-then-rename + fsync atomic writes and surgical AUTO/MANUAL edits.

use crate::format;
use crate::standup::StandupFile;
use crate::Result;
use std::io::Write;
use std::path::Path;

/// Write `content` to `path` atomically: write to `path.tmp`, fsync, rename, fsync parent.
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let tmp_path = path.with_extension("md.tmp");
    {
        let mut file = std::fs::File::create(&tmp_path)?;
        file.write_all(content.as_bytes())?;
        file.sync_all()?;
    }
    std::fs::rename(&tmp_path, path)?;
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
    Ok(())
}

/// Surgically edit a standup `.md`: replace/insert the AUTO block for `host`.
///
/// Reads the existing file (if any) via `format::parse`, replaces the AUTO block
/// for `host` with `body` (collapsing duplicates), inserts it before the MANUAL
/// region, updates the title + subtitle, and writes atomically. Builds a skeleton
/// if the file is missing.
pub fn set_auto(
    file_path: &Path,
    host: &str,
    title: &str,
    subtitle: &str,
    body: &str,
) -> Result<()> {
    let content = std::fs::read_to_string(file_path);
    let mut file: StandupFile = match content {
        Ok(c) => {
            if c.trim().is_empty() {
                build_skeleton(host, title, subtitle, body)
            } else {
                let mut parsed = format::parse(&c)?;
                parsed.title = title.to_string();
                parsed.subtitle = subtitle.to_string();
                set_auto_block(&mut parsed, host, body);
                parsed
            }
        }
        Err(_) => build_skeleton(host, title, subtitle, body),
    };

    ensure_auto_block(&mut file, host, body);
    let rendered = format::render(&file);
    write_atomic(file_path, &rendered)
}

fn set_auto_block(file: &mut StandupFile, host: &str, body: &str) {
    if let Some(block) = file.auto_blocks.iter_mut().find(|b| b.host == host) {
        block.body = body.to_string();
    } else {
        file.auto_blocks.push(crate::standup::AutoBlock {
            host: host.to_string(),
            body: body.to_string(),
        });
    }
}

fn ensure_auto_block(file: &mut StandupFile, host: &str, body: &str) {
    if !file.auto_blocks.iter().any(|b| b.host == host) {
        file.auto_blocks.push(crate::standup::AutoBlock {
            host: host.to_string(),
            body: body.to_string(),
        });
    }
}

fn build_skeleton(host: &str, title: &str, subtitle: &str, body: &str) -> StandupFile {
    StandupFile {
        date: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"),
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        auto_blocks: vec![crate::standup::AutoBlock {
            host: host.to_string(),
            body: body.to_string(),
        }],
        manual: crate::standup::ManualRegion::default(),
    }
}

/// Append `item` to the MANUAL region (before `<!-- MANUAL:END -->`).
///
/// Builds a skeleton if the file is missing. Writes atomically.
pub fn add_manual(file_path: &Path, item: &str) -> Result<()> {
    let content = std::fs::read_to_string(file_path);
    let mut file: StandupFile = match content {
        Ok(c) if !c.trim().is_empty() => format::parse(&c)?,
        _ => build_skeleton("default", "Untitled", "_._", ""),
    };

    if !file.manual.body.is_empty() {
        file.manual.body.push('\n');
    }
    file.manual.body.push_str(item);

    let rendered = format::render(&file);
    write_atomic(file_path, &rendered)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "autostand-fileops-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::create_dir_all(&dir);
        dir.join(name)
    }

    #[test]
    fn write_atomic_roundtrip() {
        let path = temp_path("roundtrip.md");
        write_atomic(&path, "hello world\n").expect("write");
        let read = std::fs::read_to_string(&path).expect("read");
        assert_eq!(read, "hello world\n");
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_auto_creates_skeleton() {
        let path = temp_path("skeleton.md");
        set_auto(
            &path,
            "host-A",
            "August 03, 2026",
            "_Work completed._",
            "**repo — FIF-1 — x**\n- did work\n",
        )
        .expect("set");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("# Daily Standup — August 03, 2026"));
        assert!(content.contains("AUTO:host-A:START"));
        assert!(content.contains("did work"));
        assert!(content.contains("MANUAL:START"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn set_auto_replaces_block() {
        let path = temp_path("replace.md");
        set_auto(&path, "host-A", "Day 1", "_._", "first body\n").expect("set1");
        set_auto(&path, "host-A", "Day 2", "_._", "second body\n").expect("set2");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("Day 2"));
        assert!(!content.contains("Day 1"));
        assert!(content.contains("second body"));
        assert!(!content.contains("first body"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn add_manual_appends() {
        let path = temp_path("manual.md");
        set_auto(&path, "host-A", "Title", "_._", "auto body\n").expect("set");
        add_manual(&path, "- attended meeting").expect("add");
        let content = std::fs::read_to_string(&path).expect("read");
        assert!(content.contains("attended meeting"));
        assert!(content.contains("MANUAL:START"));
        assert!(content.contains("auto body"));
        let _ = std::fs::remove_file(&path);
    }
}
