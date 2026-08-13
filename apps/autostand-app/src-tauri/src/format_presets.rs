//! Standup format presets — the `## OUTPUT` section templates sent to the LLM.
//!
//! Each preset molds the `## OUTPUT` block of [`crate::render::build_prompt`].
//! The deterministic renderer (`autostand_core::deterministic`) is unaffected —
//! presets only apply when the LLM produces the body.
//!
//! See `docs/specs/standup-formats.md` for the research behind each preset.

#![allow(clippy::needless_raw_string_hashes)]

use crate::commands::types::{StandupFormatConfig, StandupPreset, Verbosity};

/// Verbosity guidance appended to every preset's output instruction.
///
/// Each level carries a hard ceiling. Guidance phrased only in the positive
/// ("as normal") gave a 2B licence to write narrative paragraphs.
fn verbosity_line(verbosity: Verbosity) -> &'static str {
    match verbosity {
        Verbosity::Terse => {
            "Keep each section to one short line or a single bullet. \
             At most 12 words per bullet."
        }
        Verbosity::Standard => {
            "Use one bullet per distinct item. At most 20 words per bullet. \
             One sentence per bullet — never a paragraph."
        }
        Verbosity::Detailed => {
            "Use multiple bullets per section with brief context when useful. \
             At most 35 words per bullet, and never more than two sentences."
        }
    }
}

/// Constraints stated as prohibitions.
///
/// The positive instruction "return only the body" was not enough: a small
/// model handed a Markdown document reproduced its title, its subtitle and its
/// AUTO markers rather than answering. Naming each forbidden artifact
/// explicitly is what keeps it out.
const OUTPUT_PROHIBITIONS: &str = "Do NOT write any of the following:\n\
- a `# ` document title, or any date heading;\n\
- an italic subtitle line such as `_Work completed …_`;\n\
- HTML comments, in particular `<!-- AUTO:… -->` or `<!-- MANUAL:… -->`;\n\
- code fences (```) anywhere, including around the whole answer;\n\
- the same section header twice;\n\
- anything at all after the final section.\n";

/// Build the `## OUTPUT` section text for the given format configuration.
///
/// Returns the full block that replaces the fixed `## OUTPUT` section in
/// [`crate::render::build_prompt`]. When `config` is `None` (Det mode or no
/// format configured), the caller uses the default output block.
pub fn output_section(config: &StandupFormatConfig) -> String {
    let preset = preset_template(config.preset);
    let verbosity = verbosity_line(config.verbosity);

    let mut out = String::with_capacity(512);
    out.push_str("## OUTPUT\n");
    out.push_str("Return only the standup Markdown body: section headers and `- ` bullets. ");
    out.push_str("No preamble, no closing commentary, no code fences.\n\n");
    out.push_str(
        "Every bold section header shown in the selected structure is REQUIRED, in the exact order shown. \
Never omit an empty section: write `- None` beneath it. Every section must contain at least one `- ` bullet.\n\n",
    );
    out.push_str(OUTPUT_PROHIBITIONS);
    out.push('\n');
    out.push_str("Format your output using exactly this structure — the angle brackets are placeholders to replace, not text to keep:\n\n");
    out.push_str(preset);
    out.push_str("\n\n");
    out.push_str(verbosity);
    out.push('\n');

    if config.conventional {
        out.push_str("\nUse conventional-commit scopes where applicable: `feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`.\n");
    }

    if config.include_pr_review {
        out.push_str("\nInclude a trailing `**PR Review**` section with one bullet per PR reviewed: `repo #num — \"title\" (by author) — State`. Omit the section when no reviews were performed.\n");
    }
    if config.include_confidence {
        out.push_str("\nEnd the standup with a `**Confidence** — <1-5>` line reflecting confidence in the sprint goal.\n");
    } else {
        out.push_str("\nOmit the `**Confidence**` section.\n");
    }
    if config.include_risks {
        out.push_str("\nInclude a `**Risks**` section with forward-looking risks when material; omit when none.\n");
    } else {
        out.push_str("\nOmit the `**Risks**` section.\n");
    }

    out
}

/// Bold section headers each preset must put in the `## OUTPUT` template.
pub fn preset_section_markers(preset: StandupPreset) -> &'static [&'static str] {
    match preset {
        StandupPreset::ClassicScrum | StandupPreset::WalkingTimebox => {
            &["**Yesterday**", "**Today**", "**Blockers**"]
        }
        StandupPreset::FourQuestion => &[
            "**Yesterday**",
            "**Today**",
            "**Blockers**",
            "**Help needed**",
        ],
        StandupPreset::MadSadGlad => &["**Mad**", "**Sad**", "**Glad**"],
        StandupPreset::StartStopContinue => &["**Start**", "**Stop**", "**Continue**"],
        StandupPreset::KeepDropCreate => &["**Keep**", "**Drop**", "**Create**"],
        StandupPreset::FiveQuestion => &[
            "**Yesterday**",
            "**Today**",
            "**Blockers**",
            "**Sprint Goal confidence**",
            "**Team health**",
        ],
        StandupPreset::Spotify4q => &["**Did**", "**Doing**", "**Blocking**", "**Need**"],
        StandupPreset::AsyncStatus => &[
            "**Done yesterday**",
            "**Doing today**",
            "**Blockers**",
            "**FYI**",
        ],
        StandupPreset::WalkTheBoard => &[
            "**In-flight cards**",
            "**Aging / WIP violations**",
            "**Swarm needed?**",
        ],
        StandupPreset::Ytbr => &["**Yesterday**", "**Today**", "**Blockers**", "**Risks**"],
        StandupPreset::DecisionsCommitments => &[
            "**Fulfilled**",
            "**Committing today**",
            "**Decisions**",
            "**Blockers**",
        ],
        StandupPreset::OkrTied => &[
            "**Key Result**",
            "**Yesterday's delta**",
            "**Today**",
            "**Blockers**",
            "**Confidence**",
        ],
    }
}

/// Every documented standup preset, in the same order as the Settings grid.
pub fn all_presets() -> [StandupPreset; 13] {
    [
        StandupPreset::ClassicScrum,
        StandupPreset::FourQuestion,
        StandupPreset::MadSadGlad,
        StandupPreset::StartStopContinue,
        StandupPreset::KeepDropCreate,
        StandupPreset::FiveQuestion,
        StandupPreset::Spotify4q,
        StandupPreset::AsyncStatus,
        StandupPreset::WalkingTimebox,
        StandupPreset::WalkTheBoard,
        StandupPreset::Ytbr,
        StandupPreset::DecisionsCommitments,
        StandupPreset::OkrTied,
    ]
}

/// The preset-specific template text.
fn preset_template(preset: StandupPreset) -> &'static str {
    match preset {
        StandupPreset::ClassicScrum => CLASSIC_SCRUM,
        StandupPreset::FourQuestion => FOUR_QUESTION,
        StandupPreset::MadSadGlad => MAD_SAD_GLAD,
        StandupPreset::StartStopContinue => START_STOP_CONTINUE,
        StandupPreset::KeepDropCreate => KEEP_DROP_CREATE,
        StandupPreset::FiveQuestion => FIVE_QUESTION,
        StandupPreset::Spotify4q => SPOTIFY_4Q,
        StandupPreset::AsyncStatus => ASYNC_STATUS,
        StandupPreset::WalkingTimebox => WALKING_TIMEBOX,
        StandupPreset::WalkTheBoard => WALK_THE_BOARD,
        StandupPreset::Ytbr => YTBR,
        StandupPreset::DecisionsCommitments => DECISIONS_COMMITMENTS,
        StandupPreset::OkrTied => OKR_TIED,
    }
}

const CLASSIC_SCRUM: &str = r#"**Yesterday**
- <what you did>

**Today**
- <what you plan to do>

**Blockers**
- <impediments, or "None">"#;

const FOUR_QUESTION: &str = r#"**Yesterday**
- <what you did>

**Today**
- <what you plan to do>

**Blockers**
- <impediments, or "None">

**Help needed**
- <help you need or can offer>"#;

const MAD_SAD_GLAD: &str = r#"**Mad**
- <frustrations / impediments>

**Sad**
- <demotivators / losses>

**Glad**
- <wins / things to celebrate>"#;

const START_STOP_CONTINUE: &str = r#"**Start**
- <what we should start doing>

**Stop**
- <what we should stop doing>

**Continue**
- <what is working well>"#;

const KEEP_DROP_CREATE: &str = r#"**Keep**
- <practices to keep>

**Drop**
- <practices to drop>

**Create**
- <new things to try>"#;

const FIVE_QUESTION: &str = r#"**Yesterday**
- <what you did>

**Today**
- <what you plan to do>

**Blockers**
- <impediments, or "None">

**Sprint Goal confidence**
- <1-5>

**Team health**
- <1-5 or one-word mood>"#;

const SPOTIFY_4Q: &str = r#"**Did**
- <what I did since last standup>

**Doing**
- <what I am working on now>

**Blocking**
- <what is blocking me>

**Need**
- <help / decisions / input from others>"#;

const ASYNC_STATUS: &str = r#"**Done yesterday**
- <completed work>

**Doing today**
- <planned work>

**Blockers**
- <active impediments, @-mention owners>

**FYI**
- <non-blocking context for the team>"#;

const WALKING_TIMEBOX: &str = r#"**Yesterday**
- <one line max>

**Today**
- <one line max>

**Blockers**
- <one line max, or "None">"#;

const WALK_THE_BOARD: &str = r#"**In-flight cards**
- <card>: status / next step / blocker

**Aging / WIP violations**
- <card>: aging N days

**Swarm needed?**
- <card>: who pairs"#;

const YTBR: &str = r#"**Yesterday**
- <completed work>

**Today**
- <planned work>

**Blockers**
- <active impediments, or "None">

**Risks**
- <anticipated impediments / deadlines at risk>"#;

const DECISIONS_COMMITMENTS: &str = r#"**Fulfilled**
- <commitment kept since last standup>

**Committing today**
- <new commitment with a deadline>

**Decisions**
- <decisions made or needed>

**Blockers**
- <what blocks the commitment, or "None">"#;

const OKR_TIED: &str = r#"**Key Result**
- <KR I am advancing today>

**Yesterday's delta**
- <metric moved>

**Today**
- <what I am doing to move the KR>

**Blockers**
- <what blocks the KR, or "None">

**Confidence**
- <1-5 on hitting the KR this quarter>"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_uses_classic_scrum() {
        let config = StandupFormatConfig::default();
        let section = output_section(&config);
        assert!(section.contains("**Yesterday**"));
        assert!(section.contains("**Today**"));
        assert!(section.contains("**Blockers**"));
    }

    #[test]
    fn each_preset_embeds_its_section_markers() {
        for preset in all_presets() {
            let section = output_section(&StandupFormatConfig {
                preset,
                ..StandupFormatConfig::default()
            });
            for marker in preset_section_markers(preset) {
                assert!(
                    section.contains(marker),
                    "{preset:?} is missing {marker}:\n{section}"
                );
            }
        }
    }

    #[test]
    fn every_preset_requires_all_headers_and_a_bullet_for_empty_sections() {
        for preset in all_presets() {
            let section = output_section(&StandupFormatConfig {
                preset,
                ..StandupFormatConfig::default()
            });
            assert!(section.contains("Every bold section header"));
            assert!(section.contains("Never omit an empty section"));
            assert!(section.contains("`- None`"));
            assert!(section.contains("at least one `- ` bullet"));
        }
    }

    #[test]
    fn each_preset_produces_a_distinct_template() {
        let mut seen = std::collections::HashSet::new();
        for preset in all_presets() {
            let config = StandupFormatConfig {
                preset,
                ..StandupFormatConfig::default()
            };
            let section = output_section(&config);
            assert!(!seen.contains(&section), "preset {preset:?} is not unique");
            seen.insert(section);
        }
        assert_eq!(seen.len(), 13);
    }

    #[test]
    fn verbosity_line_changes_with_level() {
        assert_ne!(
            verbosity_line(Verbosity::Terse),
            verbosity_line(Verbosity::Standard)
        );
        assert_ne!(
            verbosity_line(Verbosity::Standard),
            verbosity_line(Verbosity::Detailed)
        );
    }

    #[test]
    fn conventional_flag_adds_scope_guidance() {
        let config = StandupFormatConfig {
            conventional: true,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("conventional-commit"));
    }

    #[test]
    fn output_section_always_has_no_preamble_rule() {
        let config = StandupFormatConfig::default();
        let section = output_section(&config);
        assert!(section.contains("No preamble"));
    }

    #[test]
    fn spotify_4q_uses_did_doing_blocking_need() {
        let config = StandupFormatConfig {
            preset: StandupPreset::Spotify4q,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("**Did**"));
        assert!(section.contains("**Doing**"));
        assert!(section.contains("**Blocking**"));
        assert!(section.contains("**Need**"));
    }

    #[test]
    fn okr_tied_includes_key_result_and_confidence() {
        let config = StandupFormatConfig {
            preset: StandupPreset::OkrTied,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("**Key Result**"));
        assert!(section.contains("**Confidence**"));
    }

    #[test]
    fn include_pr_review_toggle_adds_pr_review_guidance() {
        let config = StandupFormatConfig {
            include_pr_review: true,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("**PR Review**"));
    }

    #[test]
    fn include_pr_review_off_omits_pr_review_guidance() {
        let config = StandupFormatConfig {
            include_pr_review: false,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(!section.contains("**PR Review**"));
    }

    #[test]
    fn include_confidence_toggle_adds_confidence_guidance() {
        let config = StandupFormatConfig {
            include_confidence: true,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("**Confidence** — <1-5>"));
    }

    #[test]
    fn include_confidence_off_emits_omit_override() {
        let config = StandupFormatConfig {
            include_confidence: false,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("Omit the `**Confidence**` section."));
    }

    #[test]
    fn include_risks_toggle_adds_risks_guidance() {
        let config = StandupFormatConfig {
            include_risks: true,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("`**Risks**` section"));
    }

    #[test]
    fn include_risks_off_emits_omit_override() {
        let config = StandupFormatConfig {
            include_risks: false,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("Omit the `**Risks**` section."));
    }

    #[test]
    fn all_toggles_on_emit_all_guidance() {
        let config = StandupFormatConfig {
            include_pr_review: true,
            include_confidence: true,
            include_risks: true,
            conventional: true,
            ..StandupFormatConfig::default()
        };
        let section = output_section(&config);
        assert!(section.contains("conventional-commit"));
        assert!(section.contains("**PR Review**"));
        assert!(section.contains("**Confidence** — <1-5>"));
        assert!(section.contains("`**Risks**` section"));
    }

    /// The prompt used to say "no code fences" and then show the structure
    /// wrapped in one. Small models copied what they saw.
    #[test]
    fn no_preset_template_is_wrapped_in_a_code_fence() {
        for preset in [
            StandupPreset::ClassicScrum,
            StandupPreset::FourQuestion,
            StandupPreset::MadSadGlad,
            StandupPreset::StartStopContinue,
            StandupPreset::KeepDropCreate,
            StandupPreset::FiveQuestion,
            StandupPreset::Spotify4q,
            StandupPreset::AsyncStatus,
            StandupPreset::WalkingTimebox,
            StandupPreset::WalkTheBoard,
            StandupPreset::Ytbr,
            StandupPreset::DecisionsCommitments,
            StandupPreset::OkrTied,
        ] {
            let template = preset_template(preset);
            assert!(
                !template.contains("```"),
                "{preset:?} template still carries a fence"
            );
        }
    }

    #[test]
    fn output_section_names_every_forbidden_artifact() {
        let section = output_section(&StandupFormatConfig::default());
        for needle in [
            "Do NOT write any of the following",
            "document title",
            "_Work completed",
            "<!-- AUTO:",
            "code fences",
            "the same section header twice",
        ] {
            assert!(section.contains(needle), "missing constraint: {needle}");
        }
    }

    #[test]
    fn every_verbosity_states_a_hard_ceiling() {
        for verbosity in [Verbosity::Terse, Verbosity::Standard, Verbosity::Detailed] {
            assert!(
                verbosity_line(verbosity).contains("At most"),
                "{verbosity:?} has no ceiling"
            );
        }
    }
}
