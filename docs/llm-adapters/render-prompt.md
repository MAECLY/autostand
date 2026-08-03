# The Render Prompt

The render prompt is the **system prompt** fed to every LLM provider when rendering a standup. It defines the standup format, the source-data hierarchy, and the hard rules the model must obey. It is **provider-agnostic** — it contains no provider-specific instructions, no mention of which model is rendering, and no "you are Claude / you are GPT" framings. Each adapter is responsible for putting this text into the right field of its own request shape.

## Purpose

- Give the model the rules of the standup format in one place.
- Establish the authoritative order of the gathered activity sources so the model never trusts narrative notes over git facts.
- Forbid the most common failure modes: claiming commit work that didn't happen, exposing secrets, attributing work to AI, and the "no work done" hallucination when there is in fact work.
- Make accumulation safe across re-renders: bullets from a previous render that the new data doesn't cover are re-injected, and the model is told not to duplicate them.

## Full prompt text

```
You are a daily standup compiler. Given structured activity data, produce a clean Markdown standup.

## Source hierarchy (most authoritative first)
1. GIT FACTS — committed work. Authoritative for what was committed and when.
2. GITHUB — PRs opened/merged, reviews given. Authoritative for PR activity.
3. EDITED FILES (Claude Code / OpenCode / Codex / Gemini CLI / Grok CLI) — non-commit file work attributed to repos. Use repo basenames.
4. NOTES (.remember) — narrative non-commit work. LAST RESORT after scrubbing. Never claim committed work.

## Rules
- Past tense, concrete, English.
- One section per repo: `**<repo-name> — [TICKET](<jira_base>/TICKET) — <title>**` followed by `- ` bullets.
- Jira key is the only link. Repo name is plain text.
- Non-repo work goes under `**General — <topic>**` or `**<Spike name>**`.
- Trailing `**PR Review**` section (one bullet per PR reviewed): `repo #num — "title" (by author) — State`. Omit if empty.
- NEVER claim work was committed/pushed/merged if it's only in notes.
- NEVER include secrets, API keys, tokens, passwords.
- NEVER attribute to AI. Write as if the human did the work.
- NEVER say "no work done" if FACTS or NOTES have content.
- Accumulate: if a previous render had bullets not covered by the new data, they will be re-injected — do not duplicate them.
- Jira base URL: {JIRA_BASE}
```

## Template variables

| Variable     | Substituted from            | Required |
|--------------|------------------------------|----------|
| `{JIRA_BASE}`| `config.jira.base_url`       | Yes      |

Substitution happens in `autostand-core` before the prompt is handed to the adapter, so adapters always receive a fully-resolved string. If `{JIRA_BASE}` is unresolved (no Jira configured), the literal token is left in place and the model will see it; the pipeline surfaces a config warning in that case rather than aborting the render.

## How it is passed to each provider

| Provider | Where the render prompt goes                                            |
|----------|--------------------------------------------------------------------------|
| Claude   | `system` field of the Messages API body.                                 |
| OpenAI   | `messages[0]` with `role: "system"`.                                     |
| Gemini   | `systemInstruction.parts[0].text`.                                       |
| Grok     | `messages[0]` with `role: "system"` (OpenAI-compat shape).               |
| Ollama   | `system` field of the native `/api/chat` body (or a leading `system` message in OpenAI-compat mode). |
| Codex CLI / Claude CLI / Gemini CLI / Grok CLI | Prepended to the user prompt (CLIs that accept a system-prompt flag use it; otherwise the render prompt is concatenated above the user prompt with a separator). |

The user prompt (the structured activity data) is always the **user** role or the trailing positional CLI argument — never mixed into the system prompt.

## Versioning

- The canonical text is stored at `crates/autostand-core/src/render_prompt.txt` and embedded at compile time via `include_str!("render_prompt.txt")`. This guarantees the binary always ships with a matching prompt and there is no runtime file lookup.
- The embedded string is exposed as `autostand_core::RENDER_PROMPT: &str`.
- For advanced users, the prompt can be overridden via `config.llm.render_prompt_override: Option<PathBuf>` — if set, autostand reads and substitutes that file at render time instead of the embedded string. This lets users iterate on the prompt without rebuilding, but the embedded version is always the fallback if the override file is missing or unreadable.
- The prompt is intentionally short and rule-dense. Changes to it should be reviewed carefully — it is the single biggest lever on render quality, and a subtle wording change can shift model behavior across all five providers at once.