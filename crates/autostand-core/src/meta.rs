//! Meta-work filter: detect standup-tooling self-references.
//!
//! Prevents the standup from self-reporting its own development.

/// Default meta-work regex (pipe-separated).
pub const META_REGEX: &str = r"(?i)(standup|daily-standup|compile\.sh|scrub|auto-compile|render-prompt|anti-backdate|accumulate|redact-secrets|fileops|standup-accumulate|standup_meta)";

/// Check if a line is meta-work (standup tooling self-reference).
pub fn is_meta_work(line: &str, meta_extra: Option<&str>) -> bool {
    let combined = if let Some(extra) = meta_extra {
        format!("{META_REGEX}|({extra})")
    } else {
        META_REGEX.to_string()
    };
    match regex::Regex::new(&combined) {
        Ok(re) => re.is_match(line),
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_standup_references() {
        assert!(is_meta_work("fixed compile.sh bug", None));
        assert!(is_meta_work("added scrub logic", None));
        assert!(is_meta_work("STANDUP render", None));
    }

    #[test]
    fn ignores_normal_work() {
        assert!(!is_meta_work("fixed FIF-133 bug in parser", None));
        assert!(!is_meta_work("refactored auth module", None));
    }

    #[test]
    fn respects_meta_extra() {
        assert!(!is_meta_work("my-custom-tooling", None));
        assert!(is_meta_work("my-custom-tooling", Some("my-custom-tooling")));
    }
}
