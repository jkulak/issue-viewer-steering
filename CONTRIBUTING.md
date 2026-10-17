# Contributing

## Work

- Work is tracked in GitHub issues, one task per issue, whose body holds the description, the
  acceptance criteria and the Definition of Done. An issue labelled `backlog` waits until it is
  picked.
- Every change goes on a branch named after its issue, `N-short-slug`, and reaches `main` through
  a pull request. Nothing is committed or pushed to `main` directly.
- A pull request says what changed and why, and closes its issue with `Closes #N`.
- There is no review step; the Definition of Done is the gate. Once it is met, the pull request
  is merged with `gh pr merge --squash`, or `--rebase` when every commit stands on its own.
  `main` is one straight line: the repository allows no merge commits, and a merged branch is
  deleted.

## Definition of Done

- [ ] `make check` passes
- [ ] `openapi.json` regenerated with `make openapi` and committed, after a change to the routes
- [ ] `CHANGELOG.md` updated
- [ ] No new dependency that has not been approved
- [ ] Nothing secret or personal in the diff, checked rather than assumed

## Code

- Rust (the version `rust-toolchain.toml` pins), axum, minijinja templates, plain CSS and
  JavaScript with no build step. Templates and static files are compiled into the binary.
- `make lint` must pass: rustfmt with its defaults, clippy with warnings as errors.
- Configuration comes from the environment only. `.env.example` lists every variable; `.env`
  stays out of git.
- A new dependency, runtime or dev, is a decision: name it, say why, who maintains it and under
  what licence, and get it approved before adding it.
- The pages must not load anything from outside the viewer.
- Comments explain why, not what.
- A commit message is a short imperative subject, around 50 characters, with no prefix and no
  full stop. A body of two or three plain sentences follows only when the why does not fit in
  the subject. One logical change per commit.

## Tests

- Everything has tests: unit (next to the code), web (`tests/web`, the router through its HTTP
  interface), smoke (`tests/smoke`, the release binary over HTTP) and end-to-end (`tests/e2e`,
  Chromium via Playwright). Smoke and end-to-end tests are Python and run in Docker.
- Test names read as sentences:
  `ticking_a_checkbox_changes_one_character`, not `task_2`.
- Test data is synthetic. No real page, name, e-mail address or hostname goes into a test, a
  fixture, the docs or a commit.
- `make check` runs every lint and test there is. There is no CI: the viewer only runs on this
  machine, so `make check` passing here is what a merge goes on.

## Releases

- CalVer, `YYYY.MM.PATCH`, tagged `vYYYY.MM.PATCH`. The version lives in `Cargo.toml`, the changes
  in `CHANGELOG.md`.
- After a change to the routes, `make openapi` regenerates `openapi.json`; a test fails when the
  committed file is out of date.
