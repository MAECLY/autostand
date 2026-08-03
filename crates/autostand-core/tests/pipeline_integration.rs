use autostand_core::pipeline::{compile_file, CompileInputs, CoreRenderModeUsed, RenderMode};
use chrono::NaiveDate;
use std::collections::HashMap;
use std::fs;
use tempfile::TempDir;

fn base_inputs(home: &TempDir) -> CompileInputs {
    fs::create_dir_all(home.path().join("Github/test-repo/.git")).unwrap();
    let dailies = home.path().join("dailies");
    let state = home.path().join("state");
    fs::create_dir_all(&dailies).unwrap();
    fs::create_dir_all(&state).unwrap();

    CompileInputs {
        facts: "### repo: test-repo / tickets: FIF-133 / commits (2):\n- (FIF-133) feat: add foo\n- (FIF-133) fix: bar\n".into(),
        notes: "- Drafted design spec for queue processor\n".into(),
        github: None,
        conv: None,
        prrev: None,
        claude_files: vec![],
        prev_auto_body: String::new(),
        forbidden_tickets: vec![],
        covered_tickets: vec!["FIF-133".into()],
        ticket_days: HashMap::new(),
        skew: vec![],
        jira_base: "https://example.atlassian.net/browse".into(),
        render_mode: RenderMode::Det,
        llm_body: None,
        host: "test-host".into(),
        state_dir: state,
        dailies_dir: dailies,
        file_date: NaiveDate::from_ymd_opt(2026, 8, 4).unwrap(),
        title: "August 04, 2026".into(),
        subtitle: "Work completed August 03, 2026.".into(),
    }
}

#[test]
fn pipeline_compile_det_hermetic() {
    let home = TempDir::new().expect("temp home");
    let inputs = base_inputs(&home);

    let out = compile_file(&inputs).expect("compile_file ok");
    assert_eq!(out.render_used, CoreRenderModeUsed::Det);
    assert!(!out.fellback);
    assert!(out.audit_path.is_some());

    let file_path = inputs.dailies_dir.join("2026-08-04.md");
    let body = fs::read_to_string(&file_path).expect("output file exists");
    assert!(body.contains("# Daily Standup — August 04, 2026"));
    assert!(body.contains("<!-- AUTO:test-host:START -->"));
    assert!(body.contains("<!-- AUTO:test-host:END -->"));
    assert!(body.contains("test-repo"));
    assert!(body.contains("FIF-133"));
    assert!(body.contains("Drafted design spec"));

    let audit_path = inputs.state_dir.join("audit/2026-08-04-test-host.json");
    assert!(
        audit_path.exists(),
        "audit sidecar exists at {audit_path:?}"
    );
}

#[test]
fn pipeline_accumulate_re_injects() {
    let home = TempDir::new().expect("temp home");
    let mut inputs = base_inputs(&home);
    inputs.prev_auto_body = "- Migrated database schema for legacy billing\n".into();

    let out = compile_file(&inputs).expect("compile_file ok");
    assert!(
        out.accumulated_count >= 1,
        "at least one prev bullet re-injected"
    );

    let file_path = inputs.dailies_dir.join("2026-08-04.md");
    let body = fs::read_to_string(&file_path).expect("output file exists");
    assert!(body.contains("Migrated database schema for legacy billing"));
}
