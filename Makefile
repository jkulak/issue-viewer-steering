.DEFAULT_GOAL := help

COMPOSE := docker compose
DEV := $(COMPOSE) --profile dev run --rm dev
# A compose project of its own, so cleaning up after the tests never stops a running viewer.
E2E := $(COMPOSE) -p issue-viewer-e2e --profile e2e

.PHONY: help build dev-image mounts up down logs url test lint format smoke e2e check openapi lock

help: ## List the targets with a one-line description
	@awk 'BEGIN {FS = ":.*## "} /^[a-zA-Z0-9_-]+:.*## / {printf "  \033[36m%-10s\033[0m %s\n", $$1, $$2}' $(MAKEFILE_LIST)

build: ## Build the runtime and dev images
	$(COMPOSE) build app
	$(COMPOSE) --profile dev build dev

dev-image: ## Build the dev image if it is not there yet (code is mounted, so once is enough)
	@docker image inspect issue-viewer:dev >/dev/null 2>&1 || $(COMPOSE) --profile dev build dev

mounts: ## Write compose.override.yaml (the project mounts) from PROJECTS in .env
	$(COMPOSE) -f compose.yaml build app
	$(COMPOSE) -f compose.yaml run --rm --no-deps -T app python -m issue_viewer.mounts > compose.override.yaml.tmp
	mv compose.override.yaml.tmp compose.override.yaml

up: mounts ## Start the viewer on 127.0.0.1:$PORT with the projects from .env
	$(COMPOSE) up -d --wait app
	@echo "Issue viewer: http://$$($(COMPOSE) port app 8080)"

down: ## Stop the viewer
	$(COMPOSE) down

logs: ## Follow the viewer's log
	$(COMPOSE) logs -f app

url: ## Print the address the viewer listens on
	@echo "http://$$($(COMPOSE) port app 8080)"

test: dev-image ## Run the unit and web tests in Docker
	$(DEV) pytest

lint: dev-image ## Check style and formatting with ruff in Docker
	$(DEV) sh -c "ruff check . && ruff format --check ."

format: dev-image ## Fix what ruff can fix and format the code, in Docker
	$(DEV) sh -c "ruff check --fix . && ruff format ."

smoke: ## Run the smoke tests against the production image on synthetic issues
	$(E2E) build e2e-app e2e
	$(E2E) run --rm e2e pytest tests/smoke; status=$$?; $(E2E) down -v; exit $$status

e2e: ## Run the browser tests (Playwright) against the production image
	$(E2E) build e2e-app e2e
	$(E2E) run --rm e2e pytest tests/e2e; status=$$?; $(E2E) down -v; exit $$status

check: lint test smoke e2e ## Everything CI runs

openapi: dev-image ## Regenerate openapi.json from the app
	$(DEV) python -m issue_viewer.openapi > openapi.json.tmp
	mv openapi.json.tmp openapi.json

lock: ## Refresh uv.lock after a dependency change (then run make build)
	docker build --target tools -t issue-viewer:tools .
	docker run --rm -v "$(CURDIR)":/app -w /app issue-viewer:tools uv lock
