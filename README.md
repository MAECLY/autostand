# autostand

Cross-platform daily standup automation. Tauri v2 desktop app (Rust + React) that gathers work activity from multiple data sources, renders prose via pluggable AI providers, and writes structured Markdown standup files.

## Status

🚧 In development. See [`docs/`](docs/README.md) for full architecture and specs.

## Features

- **Cross-platform**: Windows, macOS, Linux (Tauri v2).
- **5 AI providers**: Claude, Ollama, OpenAI/Codex, Gemini, Grok — CLI-first with API fallback.
- **8 data sources**: local git, GitHub (`gh` CLI), Claude Code sessions, Remember plugin, OpenCode, Codex, Gemini CLI, Grok CLI.
- **Anti-backdating**: git owns committed work; notes are scrubbed; phantoms detected via audit.
- **Accumulate-never-delete**: previous bullets re-injected if uncovered by new renders.
- **Two-machine sync**: per-host AUTO blocks + union merge driver.
- **Self-healing**: missed runs fill from durable disk data.
- **Design system**: Tailwind v4 tokens + shadcn/ui + Storybook 8, reusable on landing pages.

## Quick start

```bash
git clone https://github.com/MAECLY/autostand.git
cd autostand
pnpm install
pnpm tauri dev
```

Requires Rust (stable), Node 20+, pnpm 9+, and at least one LLM CLI for rendering.

## Documentation

Full docs live in [`docs/`](docs/README.md): architecture, Tauri setup, LLM adapters, data sources, design system, dev guide, user guide, and specs.

## License

MIT