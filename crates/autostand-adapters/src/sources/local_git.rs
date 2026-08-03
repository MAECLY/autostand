//! Local `Git` data source (authoritative commits).
//! See `docs/data-sources/01-local-git.md`.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use async_trait::async_trait;
use chrono::Duration;

use super::helpers::{extract_ticket_keys, run_cmd, scan_repos};
use super::{DataSource, DataSourceConfig, DataSourceError, DateWindow, SourceData};

/// Local git commit scanner.
pub struct LocalGitDataSource;

#[async_trait]
impl DataSource for LocalGitDataSource {
    fn id(&self) -> &'static str {
        "local-git"
    }
    fn display_name(&self) -> &'static str {
        "Local Git"
    }
    fn is_available(&self) -> bool {
        DataSourceConfig::from_env().github_dir.is_dir()
    }

    #[allow(clippy::too_many_lines)]
    async fn gather(
        &self,
        window: &DateWindow,
        config: &DataSourceConfig,
    ) -> Result<SourceData, DataSourceError> {
        if !config.github_dir.is_dir() {
            return Ok(SourceData::default());
        }
        let repos = scan_repos(&config.github_dir);
        if repos.is_empty() {
            return Ok(SourceData::default());
        }

        let author_re = if config.authors.is_empty() {
            String::new()
        } else {
            config.authors.join("|")
        };
        let refs = if config.git_refs.is_empty() {
            "--all"
        } else {
            config.git_refs.as_str()
        };

        let since = window.start.format("%Y-%m-%d").to_string();
        let until = (window.end + Duration::days(1))
            .format("%Y-%m-%d")
            .to_string();

        let mut sections: Vec<String> = Vec::new();
        let mut ticket_days: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for repo in &repos {
            let name = repo
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("?")
                .to_string();

            if !author_re.is_empty() {
                let since_arg = format!("--since={since}");
                let until_arg = format!("--until={until}");
                let author_arg = format!("--author={author_re}");
                let subject_args: Vec<&str> = vec![
                    "-C",
                    repo.to_str().unwrap_or("."),
                    "log",
                    refs,
                    "--no-merges",
                    &since_arg,
                    &until_arg,
                    &author_arg,
                    "--format=%H|%cd|%s",
                    "--date=short",
                ];
                match run_cmd("git", &subject_args, 30).await {
                    Ok(out) if !out.is_empty() => {
                        let commits = parse_subjects(&out);
                        if !commits.is_empty() {
                            let keys: Vec<String> = extract_ticket_keys(&out)
                                .into_iter()
                                .filter(|k| !commits.iter().any(|c| c.subject.contains(k)))
                                .chain(commits.iter().filter_map(|c| {
                                    extract_ticket_keys(&c.subject).into_iter().next()
                                }))
                                .collect();
                            let dedup_keys = dedup(&keys);
                            sections.push(render_section(&name, &dedup_keys, &commits));
                        }
                    }
                    Ok(_) => {}
                    Err(e) => tracing::warn!("local-git: git log {} failed: {e}", name),
                }
            }

            let full_args: Vec<&str> = vec![
                "-C",
                repo.to_str().unwrap_or("."),
                "log",
                refs,
                "--format=%cd|%s",
                "--date=short",
            ];
            if let Ok(out) = run_cmd("git", &full_args, 30).await {
                for line in out.lines() {
                    let mut it = line.splitn(2, '|');
                    let day = it.next().unwrap_or("").trim().to_string();
                    let subj = it.next().unwrap_or("").to_string();
                    for k in extract_ticket_keys(&subj) {
                        ticket_days.entry(k).or_default().push(day.clone());
                    }
                }
            }
        }

        if sections.is_empty() {
            return Ok(SourceData::default());
        }

        let mut facts = sections.join("\n\n");
        if !ticket_days.is_empty() {
            let map: Vec<String> = ticket_days
                .iter()
                .map(|(k, days)| {
                    let uniq = dedup(days);
                    format!("{k}={}", uniq.join(","))
                })
                .collect();
            let _ = write!(facts, "\n\n<!-- TICKET_DAYS: {} -->", map.join(","));
        }

        Ok(SourceData {
            facts: Some(facts),
            notes: None,
            enrichment: None,
            files: Vec::new(),
        })
    }
}

#[derive(Debug, Clone)]
struct Commit {
    #[allow(dead_code)]
    hash: String,
    #[allow(dead_code)]
    day: String,
    subject: String,
}

fn parse_subjects(out: &str) -> Vec<Commit> {
    out.lines()
        .filter_map(|l| {
            let mut it = l.splitn(3, '|');
            let hash = it.next()?.trim().to_string();
            let day = it.next()?.trim().to_string();
            let subject = it.next()?.trim().to_string();
            Some(Commit { hash, day, subject })
        })
        .collect()
}

fn dedup<T: AsRef<str> + Clone>(items: &[T]) -> Vec<String> {
    let mut out = Vec::new();
    for it in items {
        let s = it.as_ref().to_string();
        if !out.contains(&s) {
            out.push(s);
        }
    }
    out
}

fn render_section(repo: &str, tickets: &[String], commits: &[Commit]) -> String {
    let tickets_str = if tickets.is_empty() {
        "(none)".to_string()
    } else {
        tickets.join(", ")
    };
    let mut s = format!(
        "### repo: {repo} / tickets: {tickets_str} / commits ({}):\n",
        commits.len()
    );
    for c in commits {
        let key = extract_ticket_keys(&c.subject)
            .into_iter()
            .next()
            .unwrap_or_else(|| "-".to_string());
        let _ = writeln!(s, "- ({}) {}", key, c.subject);
    }
    s
}
