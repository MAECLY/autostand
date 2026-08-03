//! Pure-sync pipeline orchestrator.
//!
//! gather → scrub → anti-backdate → render → accumulate → redact → write → audit.
//!
//! This is the pure domain function: it receives already-gathered inputs and does
//! NOT call any adapters (LLM/data sources). The async Tauri app layer is responsible
//! for gathering and for invoking LLM providers; it hands the results to `compile_file`.

use crate::audit::{self, AuditData, DateRange};
use crate::fileops;
use crate::redact;
use crate::scrub;
use crate::Result;
use crate::{accumulate, deterministic};

use chrono::NaiveDate;
use std::collections::HashMap;
use std::path::PathBuf;

/// Which render path the caller wants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderMode {
    /// Always deterministic (no LLM).
    Det,
    /// Always LLM (error if no body supplied).
    Llm,
    /// Try LLM, fall back to deterministic.
    Auto,
}

/// Which renderer actually produced the body (reported in the audit sidecar).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreRenderModeUsed {
    /// Deterministic renderer.
    Det,
    /// LLM renderer (clean, validated).
    Llm,
}

/// Gathered inputs handed to `compile_file` by the async app layer.
pub struct CompileInputs {
    /// FACTS block string from local-git (repo/tickets/commits).
    pub facts: String,
    /// Narrative notes (today-*.md / now.md), pre-scrub.
    pub notes: String,
    /// GitHub activity enrichment string (optional).
    pub github: Option<String>,
    /// Claude Code conversation digest (optional).
    pub conv: Option<String>,
    /// PR review enrichment string (optional).
    pub prrev: Option<String>,
    /// Files Claude Code wrote (for audit sidecar).
    pub claude_files: Vec<String>,
    /// Prior AUTO block body for this host (for accumulate).
    pub prev_auto_body: String,
    /// Forbidden (cross-day) tickets from provenance.
    pub forbidden_tickets: Vec<String>,
    /// Covered tickets (already in git/github) from provenance.
    pub covered_tickets: Vec<String>,
    /// Ticket → commit days map (from all-git-tickets).
    pub ticket_days: HashMap<String, Vec<NaiveDate>>,
    /// SKEW records from provenance.
    pub skew: Vec<audit::SkewRecord>,
    /// Jira base URL, e.g. `https://org.atlassian.net/browse`.
    pub jira_base: String,
    /// Desired render mode.
    pub render_mode: RenderMode,
    /// Pre-rendered LLM body (supplied by the async caller when `render_mode` != Det).
    pub llm_body: Option<String>,
    /// Host slug.
    pub host: String,
    /// State directory (for audit sidecar + hashes).
    pub state_dir: PathBuf,
    /// Dailies directory (where `<date>.md` files live).
    pub dailies_dir: PathBuf,
    /// Filing date `F`.
    pub file_date: NaiveDate,
    /// Title for the standup file (e.g. "August 03, 2026").
    pub title: String,
    /// Subtitle (e.g. "_Work completed August 01–02, 2026._").
    pub subtitle: String,
}

/// Outputs returned by `compile_file`.
pub struct CompileOutputs {
    /// Final rendered + accumulated + redacted body.
    pub body: String,
    /// Which renderer produced the body.
    pub render_used: CoreRenderModeUsed,
    /// True if Auto mode fell back to deterministic.
    pub fellback: bool,
    /// Number of PREV bullets re-injected by accumulate.
    pub accumulated_count: u32,
    /// Path to the audit sidecar JSON, if written.
    pub audit_path: Option<PathBuf>,
}

/// Run the pure-sync compile for one filing date.
///
/// Steps: scrub → redact → render → accumulate → redact → write → audit.
pub fn compile_file(inputs: &CompileInputs) -> Result<CompileOutputs> {
    let clean_notes = scrub::scrub_notes(
        &inputs.notes,
        &inputs.forbidden_tickets,
        &inputs.covered_tickets,
        None,
    );

    let clean_conv = inputs
        .conv
        .as_ref()
        .map(|c| scrub::scrub_notes(c, &inputs.forbidden_tickets, &inputs.covered_tickets, None));

    let facts_redacted = redact::redact(&inputs.facts);
    let notes_redacted = redact::redact(&clean_notes);
    let github_redacted = inputs.github.as_ref().map(|g| redact::redact(g));
    let conv_redacted = clean_conv.as_ref().map(|c| redact::redact(c));
    let prrev_redacted = inputs.prrev.as_ref().map(|p| redact::redact(p));

    let (rendered, render_used, fellback) = render(
        &inputs.render_mode,
        inputs.llm_body.as_deref(),
        &facts_redacted,
        github_redacted.as_deref(),
        &notes_redacted,
        conv_redacted.as_deref(),
        prrev_redacted.as_deref(),
        &inputs.jira_base,
        &inputs.covered_tickets,
    )?;

    let accumulated = accumulate::accumulate(&rendered, &inputs.prev_auto_body);
    let accumulated_count = count_reinjected(&rendered, &accumulated, &inputs.prev_auto_body);

    let final_body = redact::redact(&accumulated);

    let file_path = inputs.dailies_dir.join(format!("{}.md", inputs.file_date));
    fileops::set_auto(
        &file_path,
        &inputs.host,
        &inputs.title,
        &inputs.subtitle,
        &final_body,
    )?;

    let audit_data = AuditData {
        file: inputs.file_date.to_string(),
        host: inputs.host.clone(),
        rendered_at: chrono::Utc::now(),
        window: DateRange {
            start: inputs.file_date,
            end: inputs.file_date,
        },
        forbidden_tickets: inputs.forbidden_tickets.clone(),
        covered_tickets: inputs.covered_tickets.clone(),
        skew: inputs.skew.clone(),
        ticket_days: inputs.ticket_days.clone(),
        render_mode: render_mode_label(&inputs.render_mode),
        render_used: render_used_label(&render_used, fellback),
        provider: None,
        model: None,
        fellback,
        hash: inputs_hash(&facts_redacted, &notes_redacted),
        accumulated_count,
    };
    let audit_path = audit::write_sidecar(&audit_data, &inputs.state_dir).ok();

    Ok(CompileOutputs {
        body: final_body,
        render_used,
        fellback,
        accumulated_count,
        audit_path,
    })
}

#[allow(clippy::too_many_arguments)]
fn render(
    mode: &RenderMode,
    llm_body: Option<&str>,
    facts: &str,
    github: Option<&str>,
    notes: &str,
    conv: Option<&str>,
    prrev: Option<&str>,
    jira_base: &str,
    covered_tickets: &[String],
) -> Result<(String, CoreRenderModeUsed, bool)> {
    match mode {
        RenderMode::Det => {
            let body = deterministic::render_det(facts, github, notes, conv, prrev, jira_base)?;
            Ok((body, CoreRenderModeUsed::Det, false))
        }
        RenderMode::Llm => {
            let body = llm_body
                .filter(|b| !b.trim().is_empty())
                .ok_or_else(|| crate::Error::Pipeline("llm_body required for Llm mode".into()))?;
            Ok((body.to_string(), CoreRenderModeUsed::Llm, false))
        }
        RenderMode::Auto => {
            if let Some(body) = llm_body.map(str::trim).filter(|b| !b.is_empty()) {
                if auto_valid(body, covered_tickets) {
                    return Ok((body.to_string(), CoreRenderModeUsed::Llm, false));
                }
            }
            let body = deterministic::render_det(facts, github, notes, conv, prrev, jira_base)?;
            Ok((body, CoreRenderModeUsed::Det, true))
        }
    }
}

fn auto_valid(body: &str, covered_tickets: &[String]) -> bool {
    let has_section = body
        .lines()
        .any(|l| l.trim_start().starts_with("**") && l.ends_with("**"));
    let has_bullet = body.lines().any(|l| l.trim_start().starts_with("- "));
    let mentions_ticket = covered_tickets.iter().any(|t| body.contains(t));
    let lower = body.to_lowercase();
    let has_no_work = lower.contains("no work") || lower.contains("nothing to report");
    has_section && has_bullet && mentions_ticket && !has_no_work
}

fn count_reinjected(rendered: &str, accumulated: &str, prev_auto_body: &str) -> u32 {
    if prev_auto_body.is_empty() {
        return 0;
    }
    let prev_bullets: Vec<&str> = prev_auto_body
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .collect();
    let rendered_bullets: Vec<&str> = rendered
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .collect();
    let accumulated_bullets: Vec<&str> = accumulated
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .collect();

    let mut count = 0u32;
    for bullet in &prev_bullets {
        let in_rendered = rendered_bullets.iter().any(|r| r == bullet);
        let in_accumulated = accumulated_bullets.iter().any(|a| a == bullet);
        if !in_rendered && in_accumulated {
            count += 1;
        }
    }
    count
}

fn render_mode_label(mode: &RenderMode) -> String {
    match mode {
        RenderMode::Det => "det".into(),
        RenderMode::Llm => "llm".into(),
        RenderMode::Auto => "auto".into(),
    }
}

fn render_used_label(used: &CoreRenderModeUsed, fellback: bool) -> String {
    match used {
        CoreRenderModeUsed::Det => {
            if fellback {
                "llm_fallback".into()
            } else {
                "det".into()
            }
        }
        CoreRenderModeUsed::Llm => "llm".into(),
    }
}

fn inputs_hash(facts: &str, notes: &str) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    facts.hash(&mut hasher);
    notes.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base_inputs() -> CompileInputs {
        let dir = std::env::temp_dir().join(format!(
            "autostand-pipeline-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let dailies = dir.join("dailies");
        let state = dir.join("state");
        let _ = std::fs::create_dir_all(&dailies);
        let _ = std::fs::create_dir_all(&state);
        CompileInputs {
            facts: "### repo: autostand / tickets: FIF-133 / commits (1):\n- (FIF-133) Implemented core model\n".into(),
            notes: "- Drafted design spec\n".into(),
            github: None,
            conv: None,
            prrev: None,
            claude_files: vec![],
            prev_auto_body: String::new(),
            forbidden_tickets: vec![],
            covered_tickets: vec!["FIF-133".into()],
            ticket_days: HashMap::new(),
            skew: vec![],
            jira_base: "https://jira.example.com/browse".into(),
            render_mode: RenderMode::Det,
            llm_body: None,
            host: "test-host".into(),
            state_dir: state,
            dailies_dir: dailies,
            file_date: NaiveDate::from_ymd_opt(2026, 8, 3).unwrap(),
            title: "August 03, 2026".into(),
            subtitle: "_Work completed August 01–02, 2026._".into(),
        }
    }

    #[test]
    fn compile_det_mode() {
        let inputs = base_inputs();
        let out = compile_file(&inputs).expect("compile");
        assert_eq!(out.render_used, CoreRenderModeUsed::Det);
        assert!(!out.fellback);
        assert!(out.body.contains("FIF-133"));
        let file = std::fs::read_to_string(inputs.dailies_dir.join("2026-08-03.md")).expect("read");
        assert!(file.contains("AUTO:test-host:START"));
        assert!(file.contains("Drafted design spec"));
        let _ = std::fs::remove_dir_all(inputs.state_dir.parent().unwrap());
    }

    #[test]
    fn compile_auto_falls_back_to_det() {
        let mut inputs = base_inputs();
        inputs.render_mode = RenderMode::Auto;
        inputs.llm_body = Some("nothing to report".into());
        let out = compile_file(&inputs).expect("compile");
        assert!(out.fellback);
        assert_eq!(out.render_used, CoreRenderModeUsed::Det);
        let _ = std::fs::remove_dir_all(inputs.state_dir.parent().unwrap());
    }

    #[test]
    fn accumulate_re_injects_prev() {
        let mut inputs = base_inputs();
        inputs.prev_auto_body = "- Refactored queue processor\n".into();
        let out = compile_file(&inputs).expect("compile");
        assert!(out.accumulated_count >= 1);
        assert!(out.body.contains("Refactored queue processor"));
        let _ = std::fs::remove_dir_all(inputs.state_dir.parent().unwrap());
    }
}
