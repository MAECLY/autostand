//! Fuzzy text similarity (shared by accumulate + audit).
//!
//! Single source of truth so "covered" means the same thing in compile and audit.

/// Normalize text: lowercase, collapse whitespace, strip punctuation.
pub fn norm(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Extract significant words (length > 3, non-stopwords).
pub fn sig(text: &str) -> Vec<String> {
    const STOPWORDS: &[&str] = &[
        "the", "and", "for", "are", "but", "not", "you", "all", "any", "can", "her", "was", "one",
        "our", "out", "has", "have", "had", "did", "done", "with", "this", "that", "from", "into",
        "over", "under", "than", "then",
    ];
    norm(text)
        .split_whitespace()
        .filter(|w| w.len() > 3 && !STOPWORDS.contains(w))
        .map(String::from)
        .collect()
}

/// Check if `needle` is covered by `haystack` (significant-word overlap).
pub fn covered(needle: &str, haystack: &str) -> bool {
    best_match(needle, haystack).is_some_and(|(_, score)| score >= 0.5)
}

/// Find the best matching line in `haystack` for `needle`.
/// Returns `(matched_line, score)` or `None`.
#[allow(clippy::cast_precision_loss)]
pub fn best_match<'a>(needle: &str, haystack: &'a str) -> Option<(&'a str, f64)> {
    let needle_sig = sig(needle);
    if needle_sig.is_empty() {
        return None;
    }
    haystack
        .lines()
        .map(|line| {
            let line_sig = sig(line);
            if line_sig.is_empty() {
                return (line, 0.0);
            }
            let overlap = needle_sig.iter().filter(|w| line_sig.contains(w)).count();
            let score = overlap as f64 / needle_sig.len() as f64;
            (line, score)
        })
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covered_high_overlap() {
        assert!(covered("fixed bug in parser", "fixed bug in parser module"));
        assert!(covered("fixed bug parser", "the parser bug was fixed"));
    }

    #[test]
    fn not_covered_low_overlap() {
        assert!(!covered("fixed bug in parser", "attended meeting at noon"));
    }

    #[test]
    fn norm_strips_punctuation() {
        assert_eq!(norm("Hello, World!"), "hello world");
    }
}
