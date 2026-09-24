# Contributing

## Work

- Work is tracked in `tasks/NNN-slug.md`, one file per issue, in the same format this app reads.
  `tasks/` is kept out of git.
- Every change goes on a branch named after its task, `NNN-short-slug`, and reaches `main` through
  a pull request that a person reviews and merges. Nothing is committed or pushed to `main`
  directly.
- A pull request says what changed and why, and names its task file.

## Code

- Python 3.13, FastAPI, Jinja2 templates, plain CSS and JavaScript with no build step.
- Configuration comes from the environment only. `.env.example` lists every variable; `.env`
  stays out of git.
- A new dependency, runtime or dev, is a decision: name it, say why, who maintains it and under
  what licence, and get it approved before adding it.
- The pages must not load anything from outside the viewer.
- Comments and commit messages explain why, not what.

## Tests

- Everything has tests: unit, web (the app through its HTTP interface), smoke (the production
  image) and end-to-end (Chromium via Playwright).
- Test names read as sentences:
  `test_ticking_a_checkbox_changes_one_character`, not `test_task_2`.
- Test data is synthetic. No real page, name, e-mail address or hostname goes into a test, a
  fixture, the docs or a commit.
- `make check` runs what CI runs.

## Releases

- CalVer, `YYYY.MM.PATCH`, tagged `vYYYY.MM.PATCH`. The version lives in `pyproject.toml`, the
  changes in `CHANGELOG.md`.
- After a change to the routes, `make openapi` regenerates `openapi.json`; a test fails when the
  committed file is out of date.
