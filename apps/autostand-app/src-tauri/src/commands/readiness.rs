//! Cold-start readiness of the authoritative `local-git` source.
//!
//! See `docs/tauri/02-ipc-contracts.md` row `get_standup_readiness` and
//! `docs/data-sources/01-local-git.md` § Author resolution.
//!
//! `AppConfig` derives `Default`, so a fresh install carries `github_dir: ""`
//! and `standup_authors: []`. Both of those degrade quietly — the scan root
//! falls back to `~/Documents/Github`, the author filter falls back to the
//! machine's git identity — and a user whose fallbacks miss gets an empty
//! standup with nothing on screen explaining why. This command answers the
//! three questions that decide whether the gather step can produce facts at
//! all: where it will look, whether anything is there, and whose commits it
//! will match.
//!
//! The author cascade is *not* reimplemented here: `clean_authors` and
//! `detect_git_identity` are the very functions `AuthorFilter::resolve` uses, so
//! what Settings reports and what the pipeline does cannot drift.

use std::path::Path;

use autostand_adapters::sources::helpers::scan_repos;
use autostand_adapters::sources::local_git::{clean_authors, detect_git_identity};
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

use crate::commands::{load_config, repos::resolve_github_dir};
use crate::error::AppError;

/// Which step of local-git's author cascade this machine lands on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AuthorSource {
    /// `standup_authors` holds at least one non-blank entry.
    Configured,
    /// `standup_authors` is empty, so the machine's git identity is used.
    GitIdentity,
    /// Neither is available — local-git reports `Misconfigured` instead of gathering.
    None,
}

/// What the gather step will do, and what is missing if it will do nothing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StandupReadiness {
    /// Scan root local-git will actually read (the configured value or its fallback).
    pub github_dir: String,
    /// Whether that root is a directory right now.
    pub github_dir_exists: bool,
    /// Repos directly under it — the same depth-1 scan local-git performs.
    pub repo_count: usize,
    /// `standup_authors`, trimmed, deduped and blank-free.
    pub configured_authors: Vec<String>,
    /// This machine's `git config` identity, offered as the Settings suggestion.
    pub git_identity: Option<String>,
    /// The values that will become `git log --author=…` flags.
    pub effective_authors: Vec<String>,
    /// Where [`StandupReadiness::effective_authors`] came from.
    pub author_source: AuthorSource,
    /// Whether local-git can produce facts at all.
    pub ready: bool,
}

/// Build the report from already-probed inputs.
///
/// Split from the command so the precedence rules are testable without a Tauri
/// app, a filesystem or this machine's git configuration.
fn evaluate(
    github_dir: &Path,
    github_dir_exists: bool,
    repo_count: usize,
    configured: &[String],
    identity: Option<&str>,
) -> StandupReadiness {
    let configured_authors = clean_authors(configured);
    let identity = identity
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string);

    let (effective_authors, author_source) = if configured_authors.is_empty() {
        identity.clone().map_or_else(
            || (Vec::new(), AuthorSource::None),
            |value| (vec![value], AuthorSource::GitIdentity),
        )
    } else {
        (configured_authors.clone(), AuthorSource::Configured)
    };

    StandupReadiness {
        github_dir: github_dir.to_string_lossy().to_string(),
        github_dir_exists,
        repo_count,
        configured_authors,
        git_identity: identity,
        ready: github_dir_exists && repo_count > 0 && !effective_authors.is_empty(),
        effective_authors,
        author_source,
    }
}

/// Report whether local-git can gather anything, and what is missing if not.
#[tauri::command]
pub async fn get_standup_readiness(app_handle: AppHandle) -> Result<StandupReadiness, AppError> {
    let config = load_config(&app_handle).ok();
    let github_dir = resolve_github_dir(
        config.as_ref().map(|c| c.github_dir.as_str()),
        dirs::home_dir().as_deref(),
    );
    let exists = github_dir.is_dir();
    let repo_count = if exists {
        scan_repos(&github_dir).len()
    } else {
        0
    };
    let configured = config.map(|c| c.standup_authors).unwrap_or_default();
    // Probed even when authors are configured: Settings offers it as an extra
    // identity to add, not only as the empty-list fallback.
    let identity = detect_git_identity().await.into_iter().next();

    Ok(evaluate(
        &github_dir,
        exists,
        repo_count,
        &configured,
        identity.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{evaluate, AuthorSource};
    use std::path::Path;

    fn root() -> &'static Path {
        Path::new("/home/tester/Documents/Github")
    }

    /// The state every fresh install starts in: `AppConfig::default()` plus a
    /// machine that does have a git identity. It must not read as "ready" merely
    /// because the fallback exists — there is still nothing to scan.
    #[test]
    fn a_missing_scan_root_is_never_ready() {
        let report = evaluate(root(), false, 0, &[], Some("dev@example.invalid"));
        assert!(!report.ready);
        assert_eq!(report.repo_count, 0);
        assert_eq!(report.author_source, AuthorSource::GitIdentity);
    }

    #[test]
    fn a_scan_root_without_repos_is_never_ready() {
        let report = evaluate(root(), true, 0, &["dev@example.invalid".into()], None);
        assert!(!report.ready);
        assert_eq!(report.author_source, AuthorSource::Configured);
    }

    #[test]
    fn configured_authors_win_over_the_machine_identity() {
        let report = evaluate(
            root(),
            true,
            3,
            &["dev@example.invalid".into()],
            Some("machine@example.invalid"),
        );
        assert!(report.ready);
        assert_eq!(report.effective_authors, vec!["dev@example.invalid"]);
        assert_eq!(report.author_source, AuthorSource::Configured);
        // Still reported, so Settings can offer it as a second identity to add.
        assert_eq!(
            report.git_identity.as_deref(),
            Some("machine@example.invalid")
        );
    }

    #[test]
    fn an_empty_author_list_falls_back_to_the_machine_identity() {
        let report = evaluate(root(), true, 3, &[], Some("machine@example.invalid"));
        assert!(report.ready);
        assert_eq!(report.effective_authors, vec!["machine@example.invalid"]);
        assert_eq!(report.author_source, AuthorSource::GitIdentity);
        assert!(report.configured_authors.is_empty());
    }

    /// The one case local-git turns into `Misconfigured`: nothing to filter on.
    #[test]
    fn no_authors_and_no_identity_is_not_ready() {
        let report = evaluate(root(), true, 3, &[], None);
        assert!(!report.ready);
        assert_eq!(report.author_source, AuthorSource::None);
        assert!(report.effective_authors.is_empty());
        assert!(report.git_identity.is_none());
    }

    /// Blanks are what a cleared Settings row leaves behind; they must not
    /// count as a configured author, exactly as `AuthorFilter` treats them.
    #[test]
    fn blank_authors_do_not_count_as_configured() {
        let report = evaluate(
            root(),
            true,
            3,
            &[String::new(), "   ".into()],
            Some("machine@example.invalid"),
        );
        assert!(report.configured_authors.is_empty());
        assert_eq!(report.author_source, AuthorSource::GitIdentity);
    }

    #[test]
    fn configured_authors_are_trimmed_and_deduped() {
        let report = evaluate(
            root(),
            true,
            1,
            &["  dev@x.invalid ".into(), "dev@x.invalid".into()],
            None,
        );
        assert_eq!(report.configured_authors, vec!["dev@x.invalid"]);
        assert_eq!(report.effective_authors, vec!["dev@x.invalid"]);
    }

    #[test]
    fn a_blank_git_identity_counts_as_no_identity() {
        let report = evaluate(root(), true, 1, &[], Some("   "));
        assert_eq!(report.author_source, AuthorSource::None);
        assert!(report.git_identity.is_none());
    }

    /// The wire strings the frontend branches on.
    #[test]
    fn the_author_source_serializes_kebab_case() {
        for (source, expected) in [
            (AuthorSource::Configured, "configured"),
            (AuthorSource::GitIdentity, "git-identity"),
            (AuthorSource::None, "none"),
        ] {
            assert_eq!(serde_json::to_value(source).unwrap(), expected);
        }
    }
}
