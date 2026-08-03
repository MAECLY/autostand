//! Secrets redaction (regex-based, defense-in-depth).
//!
//! Applied pre-LLM (inputs never sent to provider) and pre-write (rendered body scrubbed).
//! See `docs/architecture/05-security.md`.

/// Redact known secret patterns from `input`, replacing with `[REDACTED]`.
pub fn redact(input: &str) -> String {
    let patterns: &[(&str, &str)] = &[
        // SSH/PGP private keys
        (
            r"-----BEGIN (?:RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----[\s\S]*?-----END (?:RSA |EC |OPENSSH |PGP )?PRIVATE KEY-----",
            "[REDACTED-KEY]",
        ),
        // GitHub tokens
        (r"gh[pousr]_[A-Za-z0-9]{36,}", "[REDACTED-TOKEN]"),
        (r"github_pat_[A-Za-z0-9_]{82}", "[REDACTED-TOKEN]"),
        // Anthropic
        (r"sk-ant-[A-Za-z0-9\-_]{20,}", "[REDACTED-TOKEN]"),
        // OpenAI
        (r"sk-[A-Za-z0-9]{20,}", "[REDACTED-TOKEN]"),
        // AWS
        (r"AKIA[0-9A-Z]{16}", "[REDACTED-AWS]"),
        // Slack
        (r"xox[baprs]-[A-Za-z0-9-]{10,}", "[REDACTED-SLACK]"),
        // Google API
        (r"AIza[0-9A-Za-z\-_]{35}", "[REDACTED-GOOGLE]"),
        // JWTs (three base64 segments)
        (
            r"eyJ[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}\.[A-Za-z0-9_-]{10,}",
            "[REDACTED-JWT]",
        ),
        // KEY=VALUE env secrets
        (
            r"(?i)(KEY|TOKEN|SECRET|PASSWORD|API_KEY)\s*=\s*\S{8,}",
            "[REDACTED-ENV]",
        ),
        // password: value
        (r"(?i)password\s*:\s*\S{8,}", "[REDACTED-PASSWORD]"),
    ];

    let mut result = input.to_string();
    for (pattern, replacement) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            result = re.replace_all(&result, *replacement).to_string();
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_github_token() {
        let input = "token: ghp_abcdef1234567890abcdef1234567890abcdef";
        let out = redact(input);
        assert!(out.contains("[REDACTED-TOKEN]"));
        assert!(!out.contains("ghp_"));
    }

    #[test]
    fn redacts_openai_key() {
        let input = "key: sk-abcdef1234567890abcdef1234567890";
        let out = redact(input);
        assert!(out.contains("[REDACTED-TOKEN]"));
        assert!(!out.contains("sk-abcdef"));
    }

    #[test]
    fn redacts_env_kv() {
        let input = "API_KEY=supersecretvalue123";
        let out = redact(input);
        assert!(out.contains("[REDACTED-ENV]"));
        assert!(!out.contains("supersecretvalue123"));
    }

    #[test]
    fn preserves_normal_text() {
        let input = "fixed bug in parser\n- reviewed PR #42";
        let out = redact(input);
        assert_eq!(out, input);
    }
}
