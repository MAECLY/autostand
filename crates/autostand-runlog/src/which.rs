//! Finding a program, for a process that did not come from a shell.
//!
//! An app launched from Finder, the Dock, a `.desktop` entry or the Start menu
//! does not inherit the `PATH` its user sees in a terminal. On macOS launchd
//! hands it `/usr/bin:/bin:/usr/sbin:/sbin`; nothing installed by Homebrew, npm,
//! bun, cargo or a version manager is on it. So a `PATH` walk that works
//! perfectly in `cargo run` reports every provider CLI as missing the moment the
//! same build is installed and double-clicked — which is exactly what 1.0.0 did:
//! Claude, Codex, Grok, Gemini and Ollama all read "CLI not found", while the
//! bundled local runtime, which needs no `PATH` at all, was found.
//!
//! The search here is the inherited `PATH` first, then the conventional install
//! locations for this OS, then the per-user ones under `$HOME`. Order matters:
//! whatever the user's own environment says still wins, and these are only ever
//! consulted for names it did not already resolve.
//!
//! This is deliberately a filesystem probe and never a login shell. Asking
//! `$SHELL -lc 'echo $PATH'` would also catch exotic version managers, but it
//! runs the user's startup files — an arbitrary program, at app start, that can
//! block — to answer a question a directory listing answers well enough. A setup
//! this file cannot see is what the per-provider explicit path setting is for.

use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Extensions Windows treats as executable, in the order `PATHEXT` lists them.
///
/// Not read from `PATHEXT` itself: the interesting entries there for our
/// purposes are always these three, and a `.COM`/`.VBS` match would be a
/// surprise rather than a feature.
#[cfg(windows)]
const WINDOWS_EXECUTABLE_SUFFIXES: [&str; 3] = ["exe", "cmd", "bat"];

/// Locate an executable by name, searching beyond the inherited `PATH`.
///
/// Returns the first match, or `None` when nothing by that name is executable
/// anywhere the search covers.
pub fn find_program(name: &str) -> Option<PathBuf> {
    find_program_in(name, search_path())
}

/// [`find_program`] against an explicit list of directories.
///
/// Tests use this so they never mutate the process environment, which every
/// other test in the binary is reading at the same time.
pub fn find_program_in<'a>(
    name: &str,
    dirs: impl IntoIterator<Item = &'a PathBuf>,
) -> Option<PathBuf> {
    for dir in dirs {
        let direct = dir.join(name);
        if direct.is_file() {
            return Some(direct);
        }
        #[cfg(windows)]
        for suffix in WINDOWS_EXECUTABLE_SUFFIXES {
            let candidate = dir.join(format!("{name}.{suffix}"));
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

/// Every directory a program might be found in, computed once per process.
///
/// Cached because it is a handful of `exists()` calls and the answer cannot
/// change while the app runs — `PATH` is read at spawn and the install locations
/// are fixed by the OS.
pub fn search_path() -> &'static [PathBuf] {
    static SEARCH_PATH: OnceLock<Vec<PathBuf>> = OnceLock::new();
    SEARCH_PATH.get_or_init(|| {
        build_search_path(
            std::env::var_os("PATH").as_deref(),
            home_dir(),
            system_install_dirs(),
            user_install_dirs(),
        )
    })
}

/// The search path as a `PATH`-shaped string, for handing to a child process.
///
/// A CLI autostand launches usually needs to find its own helpers — `gh` shells
/// out to `git`, `claude` to `node` — and it inherits this process's stunted
/// `PATH` unless it is given a better one.
pub fn search_path_env() -> OsString {
    std::env::join_paths(search_path()).unwrap_or_else(|_| {
        // Only reachable if a directory contains the platform separator, which
        // it cannot on any OS we build for. Falling back to the inherited value
        // keeps a child process working rather than handing it nothing.
        std::env::var_os("PATH").unwrap_or_default()
    })
}

/// Build the ordered, de-duplicated search path.
///
/// Every input is a parameter, including the conventional locations, so a test
/// describes a whole machine rather than inheriting the one it runs on — this
/// machine really does have a `codex` in `/opt/homebrew/bin`, and a test that
/// let the real list through was asserting against it instead of its fixture.
fn build_search_path(
    path_env: Option<&OsStr>,
    home: Option<PathBuf>,
    system: &[&str],
    user: &[&str],
) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = Vec::new();
    let mut push = |dir: PathBuf, require_exists: bool| {
        // A directory the user already has on PATH is kept even when it does not
        // exist yet: their environment is a statement of intent, and one that
        // appears later still resolves. The conventional locations are the
        // opposite — listing every one unconditionally would mean probing a
        // dozen absent directories for every lookup.
        if require_exists && !dir.is_dir() {
            return;
        }
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    };

    if let Some(path_env) = path_env {
        for dir in std::env::split_paths(path_env) {
            push(dir, false);
        }
    }

    for dir in system {
        push(PathBuf::from(dir), true);
    }

    if let Some(home) = home {
        for relative in user {
            push(home.join(relative), true);
        }
    }

    dirs
}

/// Machine-wide locations a package manager installs binaries into.
const fn system_install_dirs() -> &'static [&'static str] {
    #[cfg(target_os = "macos")]
    {
        // Homebrew on Apple Silicon, Homebrew on Intel, MacPorts, then the
        // system directories launchd does provide — listed so the result is a
        // complete PATH even when the inherited one was empty.
        &[
            "/opt/homebrew/bin",
            "/usr/local/bin",
            "/opt/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/sbin",
            "/sbin",
        ]
    }
    #[cfg(target_os = "linux")]
    {
        // /var/lib/flatpak/exports/bin and the snap directory carry CLIs the
        // distro package manager never sees.
        &[
            "/usr/local/bin",
            "/usr/bin",
            "/bin",
            "/usr/local/sbin",
            "/usr/sbin",
            "/snap/bin",
            "/var/lib/flatpak/exports/bin",
        ]
    }
    #[cfg(windows)]
    {
        // Nothing machine-wide to add: the Windows installers that matter write
        // into per-user directories, and System32 is always on the inherited
        // PATH even for a process started from the Start menu.
        &[]
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
    {
        &["/usr/local/bin", "/usr/bin", "/bin"]
    }
}

/// Locations under `$HOME`, relative to it.
///
/// This is where the provider CLIs actually land: `claude` and `codex` install
/// through npm or their own installer, `grok` through bun, and a Rust-built tool
/// through cargo — none of which touch a system directory.
const fn user_install_dirs() -> &'static [&'static str] {
    #[cfg(windows)]
    {
        &[
            "AppData\\Roaming\\npm",
            "AppData\\Local\\Programs",
            "AppData\\Local\\Microsoft\\WinGet\\Links",
            ".bun\\bin",
            ".cargo\\bin",
            ".local\\bin",
        ]
    }
    #[cfg(not(windows))]
    {
        &[
            ".local/bin",
            "bin",
            ".bun/bin",
            ".cargo/bin",
            ".deno/bin",
            ".npm-global/bin",
            ".yarn/bin",
            ".claude/local",
        ]
    }
}

/// The user's home directory, without pulling in a dependency for it.
fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var_os("USERPROFILE")
            .map(PathBuf::from)
            .filter(|home| !home.as_os_str().is_empty())
    }
    #[cfg(not(windows))]
    {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .filter(|home| !home.as_os_str().is_empty())
    }
}

/// True when `path` is a file, treating a broken symlink as absent.
///
/// Exposed for callers that already hold a path and only need it validated.
pub fn is_executable_file(path: &Path) -> bool {
    path.is_file()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// The per-user locations the fixtures below are built around. Spelled out
    /// rather than taken from `user_install_dirs()` so the fixture and the
    /// assertion cannot drift apart on a platform the test is not running on.
    const USER_DIRS: &[&str] = &[".local/bin", ".bun/bin"];

    /// A directory tree standing in for an installed machine.
    fn fixture() -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        for relative in ["shell-path", "home/.local/bin", "home/.bun/bin"] {
            fs::create_dir_all(root.path().join(relative)).expect("mkdir");
        }
        root
    }

    fn touch(path: &Path) {
        fs::write(path, b"#!/bin/sh\n").expect("write");
    }

    #[test]
    fn inherited_path_comes_first() {
        let root = fixture();
        let inherited = root.path().join("shell-path");
        let user = root.path().join("home/.local/bin");
        touch(&inherited.join("claude"));
        touch(&user.join("claude"));

        let dirs = build_search_path(
            Some(inherited.clone().as_os_str()),
            Some(root.path().join("home")),
            &[],
            USER_DIRS,
        );

        assert_eq!(
            find_program_in("claude", &dirs),
            Some(inherited.join("claude")),
            "the user's own PATH has to win over a location we guessed",
        );
    }

    #[test]
    fn finds_a_binary_the_inherited_path_never_mentions() {
        // The reported 1.0.0 bug: launchd hands the app a PATH with none of the
        // directories a provider CLI installs into.
        let root = fixture();
        let user = root.path().join("home/.bun/bin");
        touch(&user.join("grok"));

        let dirs = build_search_path(
            Some(root.path().join("shell-path").as_os_str()),
            Some(root.path().join("home")),
            &[],
            USER_DIRS,
        );

        assert_eq!(find_program_in("grok", &dirs), Some(user.join("grok")));
    }

    #[test]
    fn works_with_no_inherited_path_at_all() {
        let root = fixture();
        touch(&root.path().join("home/.local/bin/codex"));

        let dirs = build_search_path(None, Some(root.path().join("home")), &[], USER_DIRS);

        assert_eq!(
            find_program_in("codex", &dirs),
            Some(root.path().join("home/.local/bin/codex")),
        );
    }

    #[test]
    fn absent_directories_are_not_searched() {
        let root = fixture();
        let dirs = build_search_path(None, Some(root.path().join("home")), &[], USER_DIRS);

        assert!(
            dirs.iter().all(|dir| dir.is_dir()),
            "a guessed directory that does not exist is a stat call on every \
             lookup, forever: {dirs:?}",
        );
    }

    #[test]
    fn a_directory_is_never_searched_twice() {
        let root = fixture();
        let user = root.path().join("home/.local/bin");
        let repeated = std::env::join_paths([&user, &user]).expect("join");

        let dirs = build_search_path(
            Some(repeated.as_os_str()),
            Some(root.path().join("home")),
            &[],
            USER_DIRS,
        );

        assert_eq!(
            dirs.iter().filter(|dir| **dir == user).count(),
            1,
            "duplicates multiply the work of every miss",
        );
    }

    #[test]
    fn a_directory_is_not_mistaken_for_a_program() {
        let root = fixture();
        fs::create_dir_all(root.path().join("shell-path/claude")).expect("mkdir");

        let dirs = build_search_path(
            Some(root.path().join("shell-path").as_os_str()),
            None,
            &[],
            &[],
        );

        assert_eq!(find_program_in("claude", &dirs), None);
    }

    /// The exact shape of the 1.0.0 report: a launchd PATH, and every provider
    /// CLI installed where launchd never looks.
    #[test]
    fn a_launchd_path_still_finds_the_provider_clis() {
        let root = fixture();
        let home = root.path().join("home");
        for (dir, program) in [
            (".local/bin", "claude"),
            (".bun/bin", "grok"),
            (".local/bin", "codex"),
            (".local/bin", "ollama"),
        ] {
            touch(&home.join(dir).join(program));
        }
        // What launchd actually hands a double-clicked app on macOS. None of
        // these directories exist in the fixture, which is the point: the
        // inherited entries are kept, and they find nothing.
        let launchd = std::env::join_paths(["/usr/bin", "/bin", "/usr/sbin", "/sbin"]).unwrap();

        let dirs = build_search_path(
            Some(launchd.as_os_str()),
            Some(home.clone()),
            &[],
            USER_DIRS,
        );

        for (dir, program) in [
            (".local/bin", "claude"),
            (".bun/bin", "grok"),
            (".local/bin", "codex"),
            (".local/bin", "ollama"),
        ] {
            assert_eq!(
                find_program_in(program, &dirs),
                Some(home.join(dir).join(program)),
                "{program} reported as missing, which is the bug this file exists for",
            );
        }
    }

    #[test]
    fn a_missing_home_is_not_an_error() {
        let root = fixture();
        let dirs = build_search_path(
            Some(root.path().join("shell-path").as_os_str()),
            None,
            &[],
            &[],
        );
        assert!(dirs.contains(&root.path().join("shell-path")));
    }
}
