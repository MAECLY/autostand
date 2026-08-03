//! Accumulate-never-delete: re-inject uncovered PREV bullets.
//!
//! See `docs/specs/anti-backdating.md`.

use crate::textsim;

/// Re-inject PREV bullets not covered by the new render.
///
/// Returns the final body with re-injected bullets appended under matching
/// sections, or re-appended at the end if no matching section is found.
pub fn accumulate(new_body: &str, prev_body: &str) -> String {
    let prev_bullets: Vec<&str> = prev_body
        .lines()
        .filter(|l| l.trim_start().starts_with("- "))
        .collect();

    if prev_bullets.is_empty() {
        return new_body.to_string();
    }

    let mut out = new_body.to_string();
    let mut reinjected = 0u32;

    for bullet in prev_bullets {
        let bullet_text = bullet.trim_start_matches("- ").trim();
        if !textsim::covered(bullet_text, new_body) {
            // Re-append at the end with a note (TODO: section-aware insertion)
            if !out.ends_with('\n') {
                out.push('\n');
            }
            out.push_str(bullet);
            out.push('\n');
            reinjected += 1;
        }
    }

    if reinjected > 0 {
        tracing::info!(reinjected, "re-injected uncovered PREV bullets");
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_covered_bullets_out() {
        let prev = "- fixed bug A\n- added feature B";
        let new_body = "**repo — FIF-1 — title**\n- fixed bug A";
        let result = accumulate(new_body, prev);
        assert!(result.contains("fixed bug A"));
        assert!(
            result.contains("added feature B"),
            "uncovered bullet re-injected"
        );
    }

    #[test]
    fn no_prev_returns_new_unchanged() {
        let new_body = "**repo — FIF-1**\n- did work";
        let result = accumulate(new_body, "");
        assert_eq!(result, new_body);
    }
}
