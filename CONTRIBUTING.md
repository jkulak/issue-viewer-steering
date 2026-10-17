# Contributing

## Work

- Work is tracked in `tasks/NNN-slug.md`, one file per issue, in the same format this app reads.
  `tasks/` is kept out of git.
- Every change goes on a branch named after its task, `NNN-short-slug`, and reaches `main` through
  a pull request that a person reviews and merges. Nothing is committed or pushed to `main`
  directly.
- A pull request says what changed and why, and names its task file.

## Code

- Rust (the version `rust-toolchain.toml` pins), axum, minijinja templates, plain CSS and
  JavaScript with no build step. Templates and static files are compiled into the binary.
- `make lint` must pass: rustfmt with its defaults, clippy with warnings as errors.
- Configuration comes from the environment only. `.env.example` lists every variable; `.env`
  stays out of git.
- A new dependency, runtime or dev, is a decision: name it, say why, who maintains it and under
  what licence, and get it approved before adding it.
- The pages must not load anything from outside the viewer.
- Comments and commit messages explain why, not what.

## Tests

- Everything has tests: unit (next to the code), web (`tests/web`, the router through its HTTP
  interface), smoke (`tests/smoke`, the release binary over HTTP) and end-to-end (`tests/e2e`,
  Chromium via Playwright). Smoke and end-to-end tests are Python and run in Docker.
- Test names read as sentences:
  `ticking_a_checkbox_changes_one_character`, not `task_2`.
- Test data is synthetic. No real page, name, e-mail address or hostname goes into a test, a
  fixture, the docs or a commit.
- `make check` runs what CI runs.

## Releases

- CalVer, `YYYY.MM.PATCH`, tagged `vYYYY.MM.PATCH`. The version lives in `Cargo.toml`, the changes
  in `CHANGELOG.md`.
- After a change to the routes, `make openapi` regenerates `openapi.json`; a test fails when the
  committed file is out of date.
