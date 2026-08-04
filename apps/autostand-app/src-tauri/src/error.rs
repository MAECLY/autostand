//! Application-level error type shared by all Tauri IPC commands.
//!
//! `AppError` serializes to `{ code, message }` so the frontend can branch on `code`.
//! See `docs/tauri/02-ipc-contracts.md` § Error handling.

use serde::Serialize;
use thiserror::Error;

/// Single error enum for all IPC command failures.
///
/// The `code` tag is the lowercase prefix of each variant's `Display` string
/// (`not_found: …` ⇒ `"not_found"`), which is what the contract's
/// `handleInvokeError` and the `pipeline-error` event payload branch on
/// (`code: "llm"`). Without `rename_all` serde would emit the Rust variant name
/// (`"NotFound"`) and every frontend `code` comparison would silently miss.
#[derive(Debug, Error, Serialize)]
#[serde(tag = "code", content = "message", rename_all = "snake_case")]
pub enum AppError {
    /// Configuration load/save/validate failure.
    #[error("config: {0}")]
    Config(String),
    /// Filesystem / IO failure.
    #[error("io: {0}")]
    Io(String),
    /// Git CLI invocation failure.
    #[error("git: {0}")]
    Git(String),
    /// LLM provider failure (CLI/API).
    #[error("llm: {0}")]
    Llm(String),
    /// Pipeline lock contention (another compile in progress).
    #[error("lock: {0}")]
    Lock(String),
    /// Resource not found (file, slug, provider, etc).
    #[error("not_found: {0}")]
    NotFound(String),
    /// Invalid input (slug, cron, provider id, date).
    #[error("invalid: {0}")]
    Invalid(String),
}

impl From<std::io::Error> for AppError {
    fn from(err: std::io::Error) -> Self {
        Self::Io(err.to_string())
    }
}

impl From<serde_json::Error> for AppError {
    fn from(err: serde_json::Error) -> Self {
        Self::Config(format!("serde: {err}"))
    }
}

#[cfg(test)]
mod tests {
    use super::AppError;

    /// Serialize an error and return its `code` / `message` pair.
    fn wire(err: &AppError) -> (String, String) {
        let value = serde_json::to_value(err).expect("AppError serializes");
        let obj = value.as_object().expect("AppError is a JSON object");
        assert_eq!(
            obj.len(),
            2,
            "contract is exactly {{ code, message }}: {value}"
        );
        (
            obj["code"].as_str().expect("code is a string").to_string(),
            obj["message"]
                .as_str()
                .expect("message is a string")
                .to_string(),
        )
    }

    #[test]
    fn serializes_to_code_message_object() {
        let (code, message) = wire(&AppError::Llm("provider timed out".into()));
        assert_eq!(code, "llm");
        assert_eq!(message, "provider timed out");
    }

    #[test]
    fn codes_are_snake_case_not_variant_names() {
        // Regression guard: the serde default would emit "NotFound" here and the
        // frontend's `code === "not_found"` branch would never fire.
        assert_eq!(wire(&AppError::NotFound("nope".into())).0, "not_found");
    }

    #[test]
    fn every_variant_code_matches_its_display_prefix() {
        let variants = [
            AppError::Config(String::new()),
            AppError::Io(String::new()),
            AppError::Git(String::new()),
            AppError::Llm(String::new()),
            AppError::Lock(String::new()),
            AppError::NotFound(String::new()),
            AppError::Invalid(String::new()),
        ];
        let codes: Vec<String> = variants.iter().map(|e| wire(e).0).collect();
        assert_eq!(
            codes,
            ["config", "io", "git", "llm", "lock", "not_found", "invalid"]
        );
        for (variant, code) in variants.iter().zip(&codes) {
            let display = variant.to_string();
            assert_eq!(
                display.split(':').next().unwrap_or_default(),
                code,
                "Display prefix and serde code must not drift"
            );
        }
    }

    #[test]
    fn io_errors_map_to_the_io_code() {
        let err = AppError::from(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "missing file",
        ));
        let (code, message) = wire(&err);
        assert_eq!(code, "io");
        assert!(message.contains("missing file"), "message was {message}");
    }

    #[test]
    fn serde_errors_map_to_the_config_code() {
        let err = serde_json::from_str::<u32>("not json").expect_err("invalid JSON");
        assert_eq!(wire(&AppError::from(err)).0, "config");
    }
}
