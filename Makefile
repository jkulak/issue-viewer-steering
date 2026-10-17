.DEFAULT_GOAL := help

CARGO := cargo
BINARY := target/release/issue-viewer

.PHONY: help build run open test lint format smoke e2e e2e-image check openapi clean

help: ## List the targets with a one-line description
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build the release binary, target/release/issue-viewer
	$(CARGO) build --release --locked

run: build ## Start the viewer with the settings in .env and print its address
	$(BINARY)

open: build ## Start the viewer and open its address in the browser
	$(BINARY) --open

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

check: lint test smoke e2e ## Everything CI runs

openapi: ## Regenerate openapi.json from the route table
	$(CARGO) run --quiet --locked --example openapi > openapi.json.tmp
	mv openapi.json.tmp openapi.json

clean: ## Remove the build output
	$(CARGO) clean
