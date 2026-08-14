//! Live provider × 13-preset matrices for HTTP APIs and coding CLIs.
//!
//! Double-gated so the default `cargo test --workspace` never hits the
//! network: compile with `--features live-e2e` **and** set
//! `AUTOSTAND_E2E_LIVE=1`. A missing API key skips that provider's 13 cells
//! instead of failing the job.

#![cfg(feature = "live-e2e")]

use autostand_adapters::llm::{
    claude::ClaudeAdapter, grok::GrokAdapter, ollama::OllamaAdapter, openai::OpenAiAdapter,
    LlmAdapter, LocalRuntimePolicy, ProviderConfig, ProviderMode,
};
use autostand_app::commands::types::StandupFormatConfig;
use autostand_app::format_presets::{all_presets, preset_section_markers};
use autostand_app::render::{build_prompt, system_prompt_for, validate_render, PromptInputs};

const FACTS: &str = "### repo: autostand-core / tickets: FIF-136 / commits (1):\n- (FIF-136) Wired the compile pipeline";
const NOTES: &str = "- Pairing session on the render prompt\n- Risk: the release deadline may slip without review help";

struct LiveProvider {
    id: &'static str,
    env_vars: &'static [&'static str],
    model: &'static str,
    adapter: Box<dyn LlmAdapter>,
}

fn live_providers() -> [LiveProvider; 4] {
    [
        LiveProvider {
            id: "claude",
            env_vars: &["CLAUDE_API_KEY", "ANTHROPIC_API_KEY"],
            model: "sonnet",
            adapter: Box::new(ClaudeAdapter::default()),
        },
        LiveProvider {
            id: "openai",
            env_vars: &["OPENAI_API_KEY"],
            model: "gpt-4o-mini",
            adapter: Box::new(OpenAiAdapter::default()),
        },
        LiveProvider {
            id: "grok",
            env_vars: &["GROK_API_KEY", "XAI_API_KEY"],
            model: "grok-4.5",
            adapter: Box::new(GrokAdapter::default()),
        },
        LiveProvider {
            id: "ollama",
            env_vars: &[],
            model: "llama3.2",
            adapter: Box::new(OllamaAdapter::default()),
        },
    ]
}

fn live_cli_providers() -> [LiveProvider; 3] {
    [
        LiveProvider {
            id: "claude",
            env_vars: &[],
            model: "sonnet",
            adapter: Box::new(ClaudeAdapter::default()),
        },
        LiveProvider {
            id: "openai",
            env_vars: &[],
            // Blank is intentional: the signed-in Codex account selects its
            // compatible configured default.
            model: "",
            adapter: Box::new(OpenAiAdapter::default()),
        },
        LiveProvider {
            id: "grok",
            env_vars: &[],
            model: "grok-4.5",
            adapter: Box::new(GrokAdapter::default()),
        },
    ]
}

fn first_env(names: &[&str]) -> Option<String> {
    names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
}

fn provider_ready(provider: &LiveProvider) -> Option<ProviderConfig> {
    if provider.id == "ollama" {
        // A local daemon is optional; skip unless the caller asked for it.
        if std::env::var("OLLAMA_HOST")
            .ok()
            .filter(|v| !v.is_empty())
            .is_none()
            && std::env::var("AUTOSTAND_E2E_OLLAMA").ok().as_deref() != Some("1")
        {
            return None;
        }
        return Some(ProviderConfig {
            mode: ProviderMode::ApiOnly,
            model: provider.model.to_string(),
            cli_path: None,
            api_key: None,
            api_base_url: std::env::var("OLLAMA_HOST").ok(),
            timeout_secs: 60,
            local_runtime_policy: LocalRuntimePolicy::default(),
        });
    }
    let key = first_env(provider.env_vars)?;
    Some(ProviderConfig {
        mode: ProviderMode::ApiOnly,
        model: provider.model.to_string(),
        cli_path: None,
        api_key: Some(key),
        api_base_url: None,
        timeout_secs: 60,
        local_runtime_policy: LocalRuntimePolicy::default(),
    })
}

fn stub_prompt(format: &StandupFormatConfig) -> String {
    build_prompt(&PromptInputs {
        facts: FACTS,
        github: None,
        notes: NOTES,
        conv: None,
        prrev: None,
        jira_base: "https://example.atlassian.net/browse",
        file_date: "2026-08-03",
        range_start: "2026-08-01",
        range_end: "2026-08-02",
        prev_auto: None,
        format: Some(format),
    })
}

fn full_preset_config(
    preset: autostand_app::commands::types::StandupPreset,
) -> StandupFormatConfig {
    use autostand_app::commands::types::StandupPreset;

    StandupFormatConfig {
        preset,
        include_pr_review: false,
        include_confidence: matches!(preset, StandupPreset::FiveQuestion | StandupPreset::OkrTied),
        include_risks: preset == StandupPreset::Ytbr,
        ..StandupFormatConfig::default()
    }
}

fn selected_cli_provider(id: &str) -> bool {
    let selected = std::env::var("AUTOSTAND_E2E_CLI_PROVIDERS")
        .ok()
        .filter(|value| !value.trim().is_empty());
    match selected {
        None => true,
        Some(value) => value
            .split(',')
            .map(str::trim)
            .any(|selected| selected == id),
    }
}

fn selected_preset(preset: autostand_app::commands::types::StandupPreset) -> bool {
    let selected = std::env::var("AUTOSTAND_E2E_PRESETS")
        .ok()
        .filter(|value| !value.trim().is_empty());
    match selected {
        None => true,
        Some(value) => value
            .split(',')
            .map(str::trim)
            .any(|selected| selected == format!("{preset:?}")),
    }
}

fn validate_live_output(
    provider: &str,
    preset: autostand_app::commands::types::StandupPreset,
    body: &str,
) -> Result<(), String> {
    validate_render(body, &["FIF-136".to_string()], &[]).map_err(|error| {
        format!("{provider} × {preset:?} pipeline validation failed: {error}:\n{body}")
    })?;

    let missing = preset_section_markers(preset)
        .iter()
        .filter(|marker| !body.contains(**marker))
        .copied()
        .collect::<Vec<_>>();
    if missing.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{provider} × {preset:?} is missing required sections {missing:?}:\n{body}"
        ))
    }
}

#[tokio::test]
async fn provider_preset_matrix() {
    if std::env::var("AUTOSTAND_E2E_LIVE").ok().as_deref() != Some("1") {
        eprintln!("skipping live matrix: set AUTOSTAND_E2E_LIVE=1 to run");
        return;
    }

    let system = system_prompt_for("https://example.atlassian.net/browse");
    let mut ran = 0_u32;
    let mut skipped = 0_u32;

    for provider in live_providers() {
        let Some(config) = provider_ready(&provider) else {
            eprintln!(
                "skip provider {}: no key / ollama not requested",
                provider.id
            );
            skipped += 13;
            continue;
        };

        for preset in all_presets() {
            let format = StandupFormatConfig {
                preset,
                ..StandupFormatConfig::default()
            };
            let prompt = stub_prompt(&format);
            match provider.adapter.render(&prompt, &system, &config).await {
                Ok(output) => {
                    let markers = preset_section_markers(preset);
                    let hit = markers.iter().any(|marker| output.body.contains(marker));
                    assert!(
                        hit,
                        "{} × {preset:?} produced no expected section ({markers:?}):\n{}",
                        provider.id, output.body
                    );
                    ran += 1;
                }
                Err(err) => {
                    panic!("{} × {preset:?} failed: {err}", provider.id);
                }
            }
        }
    }

    eprintln!("live matrix: {ran} cells ran, {skipped} skipped");
}

#[tokio::test]
async fn cli_provider_preset_matrix() {
    if std::env::var("AUTOSTAND_E2E_LIVE").ok().as_deref() != Some("1")
        || std::env::var("AUTOSTAND_E2E_CLI").ok().as_deref() != Some("1")
    {
        eprintln!("skipping live CLI matrix: set AUTOSTAND_E2E_LIVE=1 and AUTOSTAND_E2E_CLI=1");
        return;
    }

    let system = system_prompt_for("https://example.atlassian.net/browse");
    let mut failures = Vec::new();
    let mut ran = 0_u32;

    for provider in live_cli_providers()
        .into_iter()
        .filter(|provider| selected_cli_provider(provider.id))
    {
        let config = ProviderConfig {
            mode: ProviderMode::CliOnly,
            model: provider.model.to_string(),
            cli_path: None,
            api_key: None,
            api_base_url: None,
            timeout_secs: 180,
            local_runtime_policy: LocalRuntimePolicy::default(),
        };

        for preset in all_presets()
            .into_iter()
            .filter(|preset| selected_preset(*preset))
        {
            let prompt = stub_prompt(&full_preset_config(preset));
            match provider.adapter.render(&prompt, &system, &config).await {
                Ok(output) => match validate_live_output(provider.id, preset, &output.body) {
                    Ok(()) => eprintln!(
                        "PASS {} × {preset:?} — {} ms, model {}",
                        provider.id, output.latency_ms, output.model
                    ),
                    Err(error) => {
                        eprintln!("FAIL {error}");
                        failures.push(error);
                    }
                },
                Err(error) => {
                    let failure = format!("{} × {preset:?} render failed: {error}", provider.id);
                    eprintln!("FAIL {failure}");
                    failures.push(failure);
                }
            }
            ran += 1;
        }
    }

    assert!(
        ran > 0,
        "no CLI providers selected by AUTOSTAND_E2E_CLI_PROVIDERS"
    );
    assert!(
        failures.is_empty(),
        "live CLI matrix had {} failure(s):\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    eprintln!("live CLI matrix: {ran} cells passed");
}
