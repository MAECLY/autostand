# autostand — task runner.
#
# `make` on its own lists every target. The Makefile is a thin, discoverable
# layer over the real tooling (cargo, pnpm, tauri); nothing here hides what it
# runs, so anything can still be invoked directly.
#
# Written for GNU Make 3.81 — the version macOS ships — so no 4.x-only features.

SHELL := /bin/bash
.SHELLFLAGS := -eu -o pipefail -c

.DEFAULT_GOAL := help

# Sentinel: pnpm rewrites this on every install, so it is newer than any
# manifest exactly when the tree is in sync.
NODE_MODULES := node_modules/.modules.yaml
MANIFESTS := package.json pnpm-lock.yaml

CARGO ?= cargo
PNPM ?= pnpm

# Date for `make compile` / `make standup`; override with `make compile DATE=2026-08-01`.
DATE ?=
COMPILE_ARGS := $(if $(DATE),--compile --date $(DATE),--compile)

.PHONY: help
help: ## List every target
	@echo "autostand — make <target>"
	@echo
	@grep -hE '^[a-zA-Z0-9_-]+:.*?## ' $(MAKEFILE_LIST) \
		| sort \
		| awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-18s\033[0m %s\n", $$1, $$2}'
	@echo
	@echo "  Run 'make check' before pushing — it is exactly what CI runs."

# ── Setup ───────────────────────────────────────────────────────────────────

$(NODE_MODULES): $(MANIFESTS)
	$(PNPM) install
	@touch $@

.PHONY: install
install: $(NODE_MODULES) ## Install JS dependencies (idempotent)

.PHONY: setup
setup: install ## First-time setup: JS deps, Rust build, Playwright browser
	$(CARGO) build --workspace
	$(PNPM) --filter autostand-app exec playwright install chromium
	@echo "Ready. 'make dev' starts the app."

# ── Develop ─────────────────────────────────────────────────────────────────

.PHONY: dev
dev: $(NODE_MODULES) ## Run the desktop app with hot reload (Vite + Rust)
	$(PNPM) tauri dev

.PHONY: dev-web
dev-web: $(NODE_MODULES) ## Run only the Vite dev server, no Tauri window (UI work)
	$(PNPM) --filter autostand-app dev

.PHONY: dev-landing
dev-landing: $(NODE_MODULES) ## Run the marketing landing page dev server
	$(PNPM) --filter landing dev

.PHONY: storybook
storybook: $(NODE_MODULES) ## Run Storybook on :6006
	$(PNPM) storybook

# ── Build ───────────────────────────────────────────────────────────────────

.PHONY: build
build: $(NODE_MODULES) ## Build the desktop bundles for this platform
	$(PNPM) tauri build

.PHONY: build-web
build-web: $(NODE_MODULES) ## Build the three web surfaces (app, landing, Storybook)
	$(PNPM) build:web

.PHONY: build-rust
build-rust: ## Build every Rust crate
	$(CARGO) build --workspace

# ── Test ────────────────────────────────────────────────────────────────────

.PHONY: test
test: test-rust test-web ## Run the Rust and frontend unit suites

.PHONY: test-rust
test-rust: ## Run the Rust test suite
	$(CARGO) test --workspace

.PHONY: test-web
test-web: $(NODE_MODULES) ## Run the frontend unit tests (vitest)
	$(PNPM) test

.PHONY: test-e2e
test-e2e: $(NODE_MODULES) ## Run both Playwright suites (app + landing)
	$(PNPM) --filter autostand-app test:e2e
	$(PNPM) --filter landing test:e2e

# ── Quality ─────────────────────────────────────────────────────────────────

.PHONY: fmt
fmt: ## Format Rust sources in place
	$(CARGO) fmt --all

.PHONY: lint
lint: $(NODE_MODULES) ## Lint everything (clippy + eslint)
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(PNPM) lint

.PHONY: typecheck
typecheck: $(NODE_MODULES) ## Typecheck the three JS packages
	$(PNPM) typecheck

.PHONY: audit
audit: ## Audit Rust dependencies for advisories
	@command -v cargo-audit >/dev/null 2>&1 || { \
		echo "cargo-audit missing; installing"; $(CARGO) install cargo-audit --locked; }
	$(CARGO) audit

.PHONY: check
check: $(NODE_MODULES) ## Everything CI runs — do this before pushing
	$(CARGO) fmt --all --check
	$(CARGO) clippy --workspace --all-targets -- -D warnings
	$(CARGO) test --workspace
	$(MAKE) audit
	$(PNPM) install --frozen-lockfile
	$(PNPM) lint
	$(PNPM) typecheck
	$(PNPM) test
	$(PNPM) build:web
	$(MAKE) test-e2e
	@echo "All green."

# ── Run the product ─────────────────────────────────────────────────────────

.PHONY: compile
compile: ## Compile a standup headlessly, as the scheduler does (DATE=YYYY-MM-DD optional)
	$(CARGO) run --quiet -p autostand-app -- $(COMPILE_ARGS)

# ── Assets + housekeeping ───────────────────────────────────────────────────

.PHONY: brand
brand: ## Regenerate the logo suite, app icons and the OG card
	python3 tests/make-wordmark.py
	python3 tests/make-icons.py
	python3 tests/make-ico.py

.PHONY: versions
versions: ## Check the version is consistent across every manifest
	python3 tests/verify-version-consistency.py

.PHONY: docs
docs: ## Build and open the Rust API docs
	$(CARGO) doc --workspace --no-deps --open

.PHONY: clean
clean: ## Remove build output (keeps node_modules and the cargo cache)
	rm -rf apps/autostand-app/dist apps/landing/dist apps/landing/.astro \
		design-system/storybook-static apps/landing/e2e/.artifacts \
		apps/autostand-app/test-results

.PHONY: clean-all
clean-all: clean ## Also remove node_modules and the Rust target directory
	rm -rf target node_modules apps/*/node_modules design-system/node_modules
