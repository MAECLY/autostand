//! Standup file format parser/writer.
//!
//! Grammar (see `docs/specs/standup-file-format.md`):
//! ```text
//! # Daily Standup — <Title>
//! _<subtitle>_
//!
//! <!-- AUTO:<host>:START -->
//! <body>
//! <!-- AUTO:<host>:END -->
//!
//! <!-- MANUAL:START -->
//! <body>
//! <!-- MANUAL:END -->
//! ```

use crate::standup::{AutoBlock, ManualRegion, StandupFile};
use crate::Result;

const AUTO_START_PREFIX: &str = "<!-- AUTO:";
const AUTO_START_SUFFIX: &str = ":START -->";
const AUTO_END_PREFIX: &str = "<!-- AUTO:";
const AUTO_END_SUFFIX: &str = ":END -->";
const MANUAL_START: &str = "<!-- MANUAL:START -->";
const MANUAL_END: &str = "<!-- MANUAL:END -->";

/// Parse a standup file from its raw text content.
pub fn parse(input: &str) -> Result<StandupFile> {
    let mut lines = input.lines().peekable();

    // Title: "# Daily Standup — <Title>"
    let title = lines
        .next()
        .ok_or_else(|| crate::Error::Format("empty file".into()))?
        .trim_start_matches("# Daily Standup — ")
        .trim()
        .to_string();

    // Subtitle: italic line starting with "_"
    let subtitle = lines
        .next()
        .filter(|l| l.trim().starts_with('_'))
        .unwrap_or("")
        .trim()
        .to_string();

    let mut auto_blocks: Vec<AutoBlock> = Vec::new();
    let mut manual = ManualRegion::default();

    while let Some(line) = lines.next() {
        let trimmed = line.trim();
        if let Some(host) = parse_auto_start(trimmed) {
            let mut body = String::new();
            let end_marker = format!("{AUTO_END_PREFIX}{host}{AUTO_END_SUFFIX}");
            for inner in lines.by_ref() {
                if inner.trim() == end_marker {
                    break;
                }
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(inner);
            }
            // Collapse duplicate AUTO blocks for the same host (union merge safety).
            if let Some(existing) = auto_blocks.iter_mut().find(|b| b.host == host) {
                if existing.body.is_empty() {
                    existing.body = body;
                }
            } else {
                auto_blocks.push(AutoBlock { host, body });
            }
        } else if trimmed == MANUAL_START {
            let mut body = String::new();
            for inner in lines.by_ref() {
                if inner.trim() == MANUAL_END {
                    break;
                }
                if !body.is_empty() {
                    body.push('\n');
                }
                body.push_str(inner);
            }
            manual = ManualRegion { body };
        }
    }

    Ok(StandupFile {
        date: chrono::NaiveDate::from_ymd_opt(1970, 1, 1).expect("epoch"),
        title,
        subtitle,
        auto_blocks,
        manual,
    })
}

fn parse_auto_start(line: &str) -> Option<String> {
    line.strip_prefix(AUTO_START_PREFIX)
        .and_then(|rest| rest.strip_suffix(AUTO_START_SUFFIX))
        .map(std::string::ToString::to_string)
}

/// Render a standup file back to its text form.
pub fn render(file: &StandupFile) -> String {
    use std::fmt::Write as _;
    let mut out = String::new();
    let _ = writeln!(out, "# Daily Standup — {}", file.title);
    let _ = writeln!(out, "{}\n", file.subtitle);

    for block in &file.auto_blocks {
        let _ = writeln!(out, "{AUTO_START_PREFIX}{}{AUTO_START_SUFFIX}", block.host);
        if !block.body.is_empty() {
            out.push_str(&block.body);
            out.push('\n');
        }
        let _ = writeln!(out, "{AUTO_END_PREFIX}{}{AUTO_END_SUFFIX}\n", block.host);
    }

    out.push_str(MANUAL_START);
    out.push('\n');
    if !file.manual.body.is_empty() {
        out.push_str(&file.manual.body);
        out.push('\n');
    }
    out.push_str(MANUAL_END);
    out.push('\n');

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        let input = "\
# Daily Standup — August 03, 2026
_Work completed August 01–02, 2026._

<!-- AUTO:MacStudio-de-Miguel:START -->
**autostand — FIF-133 — Tauri port**
- Did the thing
<!-- AUTO:MacStudio-de-Miguel:END -->

<!-- MANUAL:START -->
- Attended meeting
<!-- MANUAL:END -->
";
        let file = parse(input).expect("parse");
        assert_eq!(file.title, "August 03, 2026");
        assert_eq!(file.auto_blocks.len(), 1);
        assert_eq!(file.auto_blocks[0].host, "MacStudio-de-Miguel");
        assert!(file.auto_blocks[0].body.contains("Did the thing"));
        assert!(file.manual.body.contains("Attended meeting"));

        let rendered = render(&file);
        assert!(rendered.contains("August 03, 2026"));
        assert!(rendered.contains("MacStudio-de-Miguel"));
        assert!(rendered.contains("Attended meeting"));
    }

    #[test]
    fn collapse_duplicate_auto_blocks() {
        let input = "\
# Daily Standup — X
_._

<!-- AUTO:host:START -->
first
<!-- AUTO:host:END -->

<!-- AUTO:host:START -->
second
<!-- AUTO:host:END -->

<!-- MANUAL:START -->
<!-- MANUAL:END -->
";
        let file = parse(input).expect("parse");
        assert_eq!(file.auto_blocks.len(), 1, "duplicates collapsed");
        assert_eq!(file.auto_blocks[0].body, "first");
    }
}
