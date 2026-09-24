# Changelog

Versions follow CalVer, `YYYY.MM.PATCH`, tagged `vYYYY.MM.PATCH`.

## Unreleased

### Added

- A GitHub-like viewer for issue files (`NNN-slug.md` with front matter), for several projects
  listed in `PROJECTS`; a `:ro` entry is mounted read-only and shown without write controls.
- Issue list with open and closed tabs, GitHub's search syntax (`is:`, `state:`, `reason:`,
  `label:`, `-label:`, `no:label`, `sort:`, free text), label and sort menus, task progress,
  blocked markers, comment counts and pagination.
- Issue page with GitHub-flavoured Markdown, comments as a timeline, labels, blocked-by and
  blocking relationships, task progress and the file's path.
- Editing in place: new issues from a template, title, description, labels, checkboxes, comments,
  close as completed or not planned, reopen. Only the edited part of a file changes, writes are
  atomic, and edits based on a stale copy are refused.
- The `README.md` index table follows the issues the viewer changes, one row at a time.
- Open pages refresh when a file changes on disk.
- Protection for a login-less local app: 127.0.0.1 only, Host allowlist, same-origin checks and a
  CSRF token on every write, a strict Content Security Policy, nothing loaded from elsewhere.
- Docker, compose and Makefile; unit, web, smoke and Playwright tests; GitHub Actions CI on pull
  requests, tags and manual runs; published `openapi.json`.
