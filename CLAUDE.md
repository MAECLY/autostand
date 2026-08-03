# CLAUDE.md

Si necesitas generar o crear un script de prueba, test o lo que sea que necesites crear, hazlo dentro de `tests/`.

## Quick reference

- Monorepo Tauri v2 + Rust workspace + React/Vite/Tailwind v4/shadcn.
- Build: `cargo build --workspace` + `pnpm install` + `pnpm tauri dev`.
- Test: `cargo test --workspace` + `pnpm test`.
- Lint: `cargo clippy --workspace -- -D warnings` + `pnpm lint` + `pnpm typecheck`.
- Docs: `docs/README.md` is the master index.
- Original App Script: `~/Sync/Github_Dailies` — DO NOT MODIFY.
- See `AGENTS.md` for full conventions.