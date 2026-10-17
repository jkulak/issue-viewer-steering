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
- Makefile; unit, web, smoke and Playwright tests; published `openapi.json`.
- `make how` lists the steps of using the viewer, `make env` creates `.env` from `.env.example`,
  and `make ui` (or `make tui`) starts the viewer and opens it in the browser.
- **Open**: with `OPEN_ROOT` set, a folder picker on every page opens any issue directory under
  that root as a project until the viewer restarts, read-only when the root ends in `:ro`.
- **Board**: a kanban board for all projects, a chosen few, or one, with Backlog, Selected for
  development, In progress, Testing, Review, Blocked and Done columns taken from status labels,
  and the columns that wait for a decision marked. Cards show the git branches, worktrees,
  commits ahead and uncommitted changes for their issue, an issue without a status is in progress
  while its branch is worked on, and worktrees no issue claims are listed apart. Dragging a card
  or picking a column from its menu rewrites its status label and closes or reopens it.
- **GitHub on the board**: `GITHUB_REPOS` (`owner/name`, `owner/name=/checkout`, `owner/*`) adds
  GitHub issues and pull requests to the board, read through the signed-in `gh` CLI with no token
  in the viewer. Labels and closed state place them as they do issue files; an open pull request
  that closes an issue puts it in Review, or In progress while it is a draft or has changes
  requested; a pull request that closes nothing shown is its own card; a numbered branch in the
  checkout counts as work. Cards link to GitHub and cannot be moved. GitHub is read in the
  background while a board is open, every `GITHUB_REFRESH_SECONDS` (120) at most, and a
  repository that cannot be read keeps its last cards and says why.

### Changed

- `blocked_by` and `depends_on` in the front matter count as blocking relationships, like a
  "Blocked by" line.
- The header has a Board button next to Open, and every project a Board tab.

- The viewer is one Rust binary instead of a Python app in a Docker container. It listens on
  127.0.0.1, prints its address as the first line, and starts in well under a second; the pages,
  styles and scripts are compiled into it, so nothing else needs installing. `--open` opens the
  address in the browser, `--env-file` reads the settings from another file than `./.env`.
- A wrong setting stops the viewer with a message and exit code 2, a busy port with a message and
  exit code 1. `PORT=0` picks a free port. `LOG_LEVEL` and `REFRESH_SECONDS` are checked instead
  of falling back silently.
- `TZ` left empty means the system's time zone, and a date shown as "on Mon D" is in that zone.
- A file whose front matter repeats a key is shown as broken front matter instead of keeping the
  last value. A title or label such as `y` or `n` is written quoted, as `yes` and `no` already
  were, so a YAML 1.1 reader does not take it for a boolean.
- Writing to an issue file that is a symbolic link changes the file it points at and keeps the
  link.
- An issue file the viewer cannot read is left out of the list instead of failing the page.
- Labels sort case-insensitively, with the exact spelling as the tie-break.
- A search phrase may contain `'` and `\`.
- A link or image the sanitizer refuses keeps its text, and a `#N` reference in text the Markdown
  parser splits (next to `_`, for example) is still linked.
- Static files are sent with an ETag and, when the URL carries the version, cached for good.
- A request body over the limit is answered with 413, and a URL with a trailing slash redirects
  to the one without it.

### Removed

- The GitHub Actions workflow. The viewer only runs on this machine, so `make check`, run locally
  before a merge, is the gate.
- The Docker image, `compose.yaml` and the generated mounts; the viewer reads the directories in
  `PROJECTS` and `OPEN_ROOT` directly.

### Fixed

- `make check` passes on Docker Desktop: the browser tests no longer find `/data` empty when they
  run right after the smoke tests, because the test data directory is emptied between runs
  instead of being removed.
