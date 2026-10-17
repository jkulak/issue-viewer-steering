.DEFAULT_GOAL := help

CARGO := cargo
BINARY := target/release/issue-viewer

.PHONY: help how env build run ui tui test lint format smoke e2e e2e-image check openapi clean

help: ## List the targets with a one-line description
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

how: ## How to use the viewer, step by step
	@echo '1. make env      create .env from .env.example, then put the issue directories in PROJECTS'
	@echo '2. make ui       build the viewer, start it and open it in the browser; Ctrl+C stops it'
	@echo '3. make run      the same without the browser; the first line it prints is the address'
	@echo '4. make check    before a merge: lint, unit, web, smoke and browser tests'
	@echo '5. make openapi  after changing a route: regenerate openapi.json and commit it'

env: ## Create .env from .env.example, leaving an existing .env alone
	@if [ -e .env ]; then echo '.env already exists, left as it is'; \
	else cp .env.example .env && echo 'Created .env: put the issue directories in PROJECTS'; fi

build: ## Build the release binary, target/release/issue-viewer
	$(CARGO) build --release --locked

run: build ## Start the viewer with the settings in .env and print its address
	$(BINARY)

ui: build ## Start the viewer and open its address in the browser
	$(BINARY) --open

tui: ui ## The same as make ui

test: ## Run the unit and web tests
	$(CARGO) test --locked

lint: ## Check formatting and lints (rustfmt, clippy with warnings as errors)
	$(CARGO) fmt --check
	$(CARGO) clippy --locked --all-targets -- -D warnings

format: ## Format the code with rustfmt
	$(CARGO) fmt

e2e-image: ## Build the Playwright image the smoke and browser tests run in
	docker build -t issue-viewer:e2e tests/e2e

smoke: build e2e-image ## Run the smoke tests against the release binary on synthetic issues
	tests/e2e/run.sh smoke

e2e: build e2e-image ## Run the browser tests (Playwright, in Docker) against the release binary
	tests/e2e/run.sh e2e

check: lint test smoke e2e ## Everything that must pass before a merge

openapi: ## Regenerate openapi.json from the route table
	$(CARGO) run --quiet --locked --example openapi > openapi.json.tmp
	mv openapi.json.tmp openapi.json

clean: ## Remove the build output
	$(CARGO) clean
