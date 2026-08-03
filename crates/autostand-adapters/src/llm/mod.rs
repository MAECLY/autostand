//! LLM provider adapters. See `docs/llm-adapters/`.

pub mod claude;
pub mod detect;
pub mod gemini;
pub mod grok;
pub mod helpers;
pub mod ollama;
pub mod openai;
pub mod traits;

pub use detect::detect_cli_binary;
pub use traits::{
    CliInfo, LlmAdapter, LlmError, ProviderConfig, ProviderMode, RenderModeUsed, RenderOutput,
    TestResult,
};

/// Return all 5 provider adapters as trait objects.
pub fn registry() -> Vec<Box<dyn LlmAdapter>> {
    vec![
        Box::new(claude::ClaudeAdapter::default()),
        Box::new(ollama::OllamaAdapter::default()),
        Box::new(openai::OpenAiAdapter::default()),
        Box::new(gemini::GeminiAdapter::default()),
        Box::new(grok::GrokAdapter::default()),
    ]
}
