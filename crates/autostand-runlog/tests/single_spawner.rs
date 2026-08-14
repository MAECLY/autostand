//! The guard that makes "every process goes through the Terminal" true.
//!
//! `AGENTS.md` § Processes says nothing outside this crate may call
//! `std::process::Command` / `tokio::process::Command`. A convention nobody
//! checks lasts one pull request, so this test greps the workspace's production
//! Rust for `Command::new` and fails on anything that is not on the allowlist
//! below — with the reason it is there.
//!
//! # What is deliberately not scanned
//!
//! * **`#[cfg(test)] mod …` blocks.** A unit test's fixture (`git init` in a
//!   temp dir) is not a user action, runs in a synchronous `#[test]`, and has no
//!   Terminal to appear in. Test code is skipped, not exempted: the scanner
//!   truncates a file at its test module and keeps scanning everything above it.
//! * **`tests/` directories.** Same reason, one level up.

use std::path::{Path, PathBuf};

/// The one API this whole test exists to protect.
const NEEDLE: &str = "Command::new";

/// Production files allowed to spawn directly, each with the reason.
///
/// Adding a row is a deliberate act that must survive review. Removing one when
/// the file is migrated is enforced from the other side: the test fails if an
/// allowlisted file no longer contains a direct spawn.
const ALLOWED: &[(&str, &str)] = &[
    (
        "crates/autostand-runlog/src/proc.rs",
        "the spawner itself — this is the one place the call is allowed to exist",
    ),
    (
        "crates/autostand-core/src/host.rs",
        "host-slug detection (scutil/hostnamectl). It runs *inside* `Run::open`, \
         before any sink exists, so there is nothing to log into; and routing it \
         would put tokio + autostand-runlog into the domain crate, which today \
         depends on no other crate in this workspace",
    ),
    (
        "crates/autostand-local-llm/src/main.rs",
        "the sidecar is a separate process with no run log of its own: its parent \
         already reports it as `local model` through `run_process_piped`, and its \
         stdout is the standup body. Adding an async runtime to a deliberately \
         dependency-light binary would buy no visibility",
    ),
];

/// Directories that hold production Rust.
const ROOTS: &[&str] = &["crates", "apps/autostand-app/src-tauri/src"];

#[test]
fn nothing_outside_the_spawner_starts_a_process() {
    let root = workspace_root();
    let mut offenders: Vec<String> = Vec::new();

    for entry in ROOTS.iter().flat_map(|dir| rust_files(&root.join(dir))) {
        let relative = entry
            .strip_prefix(&root)
            .unwrap_or(&entry)
            .to_string_lossy()
            .replace('\\', "/");
        if ALLOWED.iter().any(|(path, _)| *path == relative) {
            continue;
        }
        let source = std::fs::read_to_string(&entry).expect("read a workspace source file");
        for (number, line) in production_code(&source).lines().enumerate() {
            if line.contains(NEEDLE) {
                offenders.push(format!("{relative}:{}", number + 1));
            }
        }
    }

    assert!(
        offenders.is_empty(),
        "these call sites spawn a process without going through \
         `autostand_runlog::proc`, so their work is invisible in the Terminal and \
         outside the argv/stderr redaction policy:\n  {}",
        offenders.join("\n  ")
    );
}

/// An allowlist entry that no longer describes reality is worse than none.
#[test]
fn every_exception_still_exists_and_is_still_an_exception() {
    let root = workspace_root();
    for (path, reason) in ALLOWED {
        let full = root.join(path);
        let source = std::fs::read_to_string(&full)
            .unwrap_or_else(|_| panic!("allowlisted file {path} no longer exists ({reason})"));
        assert!(
            source.contains(NEEDLE),
            "{path} no longer spawns a process directly — delete its row from ALLOWED"
        );
    }
}

/// The workspace root, from this crate's manifest directory.
fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/autostand-runlog sits two levels below the workspace root")
        .to_path_buf()
}

/// Every `.rs` file under `dir`, skipping build output and test directories.
fn rust_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    collect(dir, &mut out);
    out.sort();
    out
}

fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        if path.is_dir() {
            // `target` is build output; `tests` is test code, see the module doc.
            if name == "target" || name == "tests" || name.starts_with('.') {
                continue;
            }
            collect(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// `source` with its `#[cfg(test)] mod …` block (and everything after) removed.
///
/// Relies on the convention `AGENTS.md` states and every module in this
/// workspace follows: unit tests live in a single `#[cfg(test)] mod tests { … }`
/// at the end of the file.
fn production_code(source: &str) -> &str {
    let lines: Vec<&str> = source.lines().collect();
    for (index, line) in lines.iter().enumerate() {
        if line.trim() != "#[cfg(test)]" {
            continue;
        }
        let follows_a_module = lines[index + 1..]
            .iter()
            .find(|next| !next.trim().is_empty())
            .is_some_and(|next| next.trim_start().starts_with("mod "));
        if !follows_a_module {
            continue;
        }
        let offset = lines[..index]
            .iter()
            .map(|kept| kept.len() + 1)
            .sum::<usize>();
        return &source[..offset.min(source.len())];
    }
    source
}

#[cfg(test)]
mod tests {
    use super::production_code;

    #[test]
    fn a_trailing_test_module_is_not_scanned() {
        let source = "fn real() {}\n#[cfg(test)]\nmod tests {\n    Command::new(\"git\");\n}\n";
        assert_eq!(production_code(source), "fn real() {}\n");
    }

    #[test]
    fn a_cfg_test_attribute_on_a_plain_item_does_not_truncate() {
        // Only a test *module* ends the production part; a `#[cfg(test)]` helper
        // must not hide the code that follows it.
        let source = "#[cfg(test)]\nfn helper() {}\nfn real() { Command::new(\"git\"); }\n";
        assert!(production_code(source).contains("real"));
    }

    #[test]
    fn a_file_without_tests_is_returned_whole() {
        let source = "fn only() {}\n";
        assert_eq!(production_code(source), source);
    }
}
