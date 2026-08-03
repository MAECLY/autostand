//! `GitHub` data source (via `gh` `CLI`).
//! See `docs/data-sources/02-github.md`.

use std::fmt::Write as _;
use std::process::Command as StdCommand;

use async_trait::async_trait;
use chrono::NaiveDate;
use serde::Deserialize;
use serde_json::Value;

use super::helpers::{extract_ticket_keys, run_cmd, window_contains};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// GitHub `gh` CLI data source.
pub struct GithubDataSource;

#[derive(Debug, Deserialize)]
struct PrSummary {
    number: u64,
    title: String,
    #[serde(default)]
    state: String,
    #[serde(default)]
    #[allow(dead_code)]
    url: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    created_at: Option<String>,
    #[serde(default)]
    closed_at: Option<String>,
    #[serde(default)]
    repository: Option<RepoField>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RepoField {
    WithName { name_with_owner: String },
    Str(String),
}

impl RepoField {
    fn name(&self) -> String {
        match self {
            Self::WithName { name_with_owner } => name_with_owner.clone(),
            Self::Str(s) => s.clone(),
        }
    }
}

#[async_trait]
impl DataSource for GithubDataSource {
    fn id(&self) -> &'static str {
        "github"
    }
    fn display_name(&self) -> &'static str {
        "GitHub (gh CLI)"
    }
    fn is_available(&self) -> bool {
        StdCommand::new("gh")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success())
    }

    #[allow(clippy::too_many_lines)]
    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        if !is_available_sync() {
            return Ok(SourceData::default());
        }
        let org = &config.gh_pr_org;
        if org.is_empty() {
            return Ok(SourceData::default());
        }

        let opened = match run_cmd(
            "gh",
            &[
                "search",
                "prs",
                "--author=@me",
                &format!("--owner={org}"),
                "--json=number,title,createdAt,closedAt,state,repository",
            ],
            60,
        )
        .await
        {
            Ok(s) => parse_prs(&s),
            Err(e) => {
                tracing::warn!("github: search prs --author=@me failed: {e}");
                Vec::new()
            }
        };

        let reviewed = match run_cmd(
            "gh",
            &[
                "search",
                "prs",
                &format!("--reviewed-by={}", config.gh_reviewer),
                &format!("--owner={org}"),
                "--json=number,title,repository,state,closedAt",
            ],
            60,
        )
        .await
        {
            Ok(s) => parse_prs(&s),
            Err(e) => {
                tracing::warn!("github: search prs --reviewed-by failed: {e}");
                Vec::new()
            }
        };

        let mut prs_opened: Vec<&PrSummary> = Vec::new();
        let mut prs_merged: Vec<&PrSummary> = Vec::new();
        for pr in &opened {
            if let Some(created) = pr.created_at.as_deref() {
                if let Some(d) = parse_gh_date(created) {
                    if window_contains(window, d) {
                        prs_opened.push(pr);
                    }
                }
            }
            if pr.state.to_uppercase() == "MERGED" {
                if let Some(closed) = pr.closed_at.as_deref() {
                    if let Some(d) = parse_gh_date(closed) {
                        if window_contains(window, d) {
                            prs_merged.push(pr);
                        }
                    }
                }
            }
        }
        if prs_opened.len() > config.gh_max_prs {
            prs_opened.truncate(config.gh_max_prs);
        }
        if prs_merged.len() > config.gh_max_prs {
            prs_merged.truncate(config.gh_max_prs);
        }

        let mut review_comments: Vec<String> = Vec::new();
        let max_prs = config.gh_max_prs.max(1);
        for pr in reviewed.iter().take(max_prs) {
            let repo = pr
                .repository
                .as_ref()
                .map(RepoField::name)
                .unwrap_or_default();
            if repo.is_empty() {
                continue;
            }
            let num = pr.number;
            for endpoint in [
                format!("api /repos/{repo}/pulls/{num}/comments"),
                format!("api /repos/{repo}/issues/{num}/comments"),
            ] {
                let parts: Vec<&str> = endpoint.split(' ').collect();
                if let Ok(body) = run_cmd("gh", &parts, 60).await {
                    if let Ok(arr) = serde_json::from_str::<Vec<Value>>(&body) {
                        for c in arr {
                            let user = c
                                .get("user")
                                .and_then(|u| u.get("login"))
                                .and_then(|l| l.as_str())
                                .unwrap_or("");
                            if user != config.gh_reviewer {
                                continue;
                            }
                            let created =
                                c.get("created_at").and_then(|v| v.as_str()).unwrap_or("");
                            if let Some(d) = parse_gh_date(created) {
                                if !window_contains(window, d) {
                                    continue;
                                }
                            }
                            let body_text = c.get("body").and_then(|v| v.as_str()).unwrap_or("");
                            let truncated = truncate(body_text, config.gh_comment_len);
                            review_comments.push(format!("- {repo} #{num}: \"{truncated}\""));
                        }
                    }
                }
            }
        }

        let mut tickets: Vec<String> = Vec::new();
        for pr in opened.iter().chain(reviewed.iter()) {
            for k in extract_ticket_keys(&pr.title) {
                if !tickets.contains(&k) {
                    tickets.push(k);
                }
            }
        }

        let mut block = String::from("## GITHUB ACTIVITY\n");
        block.push_str("### PRs opened\n");
        if prs_opened.is_empty() {
            block.push_str("_(none)_\n");
        } else {
            for pr in prs_opened {
                let repo = pr
                    .repository
                    .as_ref()
                    .map(RepoField::name)
                    .unwrap_or_default();
                let created = pr
                    .created_at
                    .as_deref()
                    .and_then(parse_gh_date)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                let _ = writeln!(
                    block,
                    "- {} #{} — \"{}\" (opened {})",
                    repo, pr.number, pr.title, created
                );
            }
        }
        block.push_str("### PRs merged\n");
        if prs_merged.is_empty() {
            block.push_str("_(none)_\n");
        } else {
            for pr in prs_merged {
                let repo = pr
                    .repository
                    .as_ref()
                    .map(RepoField::name)
                    .unwrap_or_default();
                let merged = pr
                    .closed_at
                    .as_deref()
                    .and_then(parse_gh_date)
                    .map(|d| d.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                let _ = writeln!(
                    block,
                    "- {} #{} — \"{}\" (merged {})",
                    repo, pr.number, pr.title, merged
                );
            }
        }
        block.push_str("### Review comments\n");
        if review_comments.is_empty() {
            block.push_str("_(none)_\n");
        } else {
            for c in review_comments {
                let _ = writeln!(block, "{c}");
            }
        }
        let _ = writeln!(block, "<!-- GH-RECENT-TICKETS: {} -->", tickets.join(","));

        Ok(SourceData {
            facts: None,
            notes: None,
            enrichment: Some(block),
            files: Vec::new(),
        })
    }
}

fn is_available_sync() -> bool {
    StdCommand::new("gh")
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success())
}

fn parse_prs(s: &str) -> Vec<PrSummary> {
    serde_json::from_str::<Vec<PrSummary>>(s).unwrap_or_default()
}

fn parse_gh_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s.get(..10)?, "%Y-%m-%d").ok()
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(3)).collect();
        t.push_str("...");
        t
    }
}
