# Issue viewer

A local web app that shows a project's issue files the way GitHub shows issues, and edits them
in place. Each project keeps its issues as Markdown files, `tasks/NNN-slug.md`. The viewer lists,
filters, renders and changes those files and nothing else. There is no database, no login and no
users: the files are the source of truth, and they stay readable and editable by hand.

## Quick start

The viewer is one binary that serves the pages on 127.0.0.1 and prints the address. Building it
needs `rustup`; the toolchain version comes from `rust-toolchain.toml`.

```bash
make env
```

`make env` copies `.env.example` to `.env` unless `.env` is already there. Put the issue
directories you want to see in `PROJECTS` in `.env`, then:

```bash
make ui
```

`make ui` builds `target/release/issue-viewer`, starts it and opens it in the browser; `make tui`
is the same. `make run` starts it without the browser. The first line it prints is the address,
<http://127.0.0.1:8090> by default (`PORT` in `.env`). Ctrl + C stops it. `make how` lists the
steps, `make help` every target.

The binary carries its pages, styles and scripts, so it can be copied anywhere and started on its
own. It reads `.env` from the directory it starts in, or the file `--env-file` names; a variable
set in the environment wins over the file:

```bash
PORT=8091 target/release/issue-viewer --env-file /Users/you/issue-viewer.env --open
```

A setting it cannot use stops it with a message and exit code 2; a port that is already taken
stops it with exit code 1. `PORT=0` lets the system pick a free port.

`PROJECTS` is a comma-separated list of `name=/absolute/path`. The viewer shows those directories
and nothing else. An entry ending in `:ro` is read-only: the viewer never writes to it and shows
it without any write control:

```dotenv
PROJECTS=notes=/Users/you/dev/notes/tasks,archive=/Users/you/dev/old-project/tasks:ro
```

Restart the viewer after changing the list.

### Opening a directory that is not in the list

Set `OPEN_ROOT` to a directory tree, for example the folder that holds your repositories, and
every page gets an **Open** button:

```dotenv
OPEN_ROOT=/Users/you/dev:ro
```

Open browses the folders under that root, shows how many issue files each one holds and points at
the `tasks/` or `issues/` folder inside a repository. Opening a folder adds it to the project list
until the viewer restarts; to keep it, add it to `PROJECTS`. Its name comes from its path, without
a trailing `tasks` or `issues`: `notes-app/tasks` opens as `notes-app`. A folder that is already a
project opens that project. With `:ro`, every folder opened from the root is read-only; a
directory in `PROJECTS` keeps its own mode either way.

## The files it reads and writes

An issue is one file named `NNN-slug.md`. The number is the issue's identity and never changes;
the slug is only there for people.

```markdown
---
title: "Search notes by title and body"
labels: [enhancement, ui]
state: open
---

# #3 Search notes by title and body

**Depends on:** #2 · **Blocks:** –

## Acceptance criteria

- [x] Typing filters the list
- [ ] Matches are highlighted

## Comments

### 2026-09-21 16:40

Measured it: 3 ms for 2000 notes.
```

- **Front matter.** `title`, `labels` and `state`. A file without front matter still shows: the
  first `#` heading is its title and it counts as open.
- **State.** `open` is open. `done`, `closed`, `completed`, `resolved` and `fixed` mean closed as
  completed. `not-planned`, `wontfix`, `duplicate`, `invalid` and `cancelled` mean closed as not
  planned. Any other value counts as open and is shown as written, for example `in-progress`.
  Closing writes `state: done`, closing as not planned writes `state: not-planned`, reopening
  writes `state: open`.
- **`# #N Title` heading.** Hidden on the page, where the title is shown anyway. Editing the
  title changes the front matter and this heading.
- **`**Depends on:** … · **Blocks:** …`.** Shown in the sidebar as *Blocked by* and *Blocking*.
  *Blocking* also lists every issue that depends on this one. A list row says *Blocked by #N*
  while that issue is open.
- **Task lists.** Shown as checkboxes you can tick; a tick changes that one character in the
  file. The list shows the progress of the description's task list.
- **Comments.** They live at the end of the file, under `## Comments`, one `### YYYY-MM-DD HH:MM`
  heading each, in the time zone from `TZ`. Nothing may follow the comments section.
- **`README.md`.** When the directory has one with an index table (first column `#`), the viewer
  updates the row of the issue it changed and appends a row for an issue it created. Nothing
  else in the README changes.
- **`TEMPLATE.md`.** When present, a new issue starts from it; otherwise from a built-in
  template with Context, Scope, Acceptance criteria and a Definition of Done.

Every write goes through a temporary file and a rename, so another program never reads half a
file. A write changes only the part it is about: the rest of the file stays the same, byte for
byte, including quoting in the front matter and Windows line endings. A description edit, comment
edit or checkbox tick based on a copy of the file that has since changed on disk is refused, and
the text you typed stays in the editor. Open pages check for changes every few seconds
(`REFRESH_SECONDS`) and reload, unless you are typing, in which case they say so instead.

## Using it

The list works like GitHub's: open and closed tabs, a label menu, a sort menu, 25 issues a page.
The search box understands:

| Query | Finds |
|---|---|
| `is:open`, `is:closed` | open or closed issues |
| `state:in-progress` | a raw `state` value |
| `reason:completed`, `reason:not-planned` | how a closed issue was closed |
| `label:ui` | issues with that label; repeat for all of several, `label:ui,bug` for either |
| `-label:backlog` | issues without that label |
| `no:label` | issues with no labels |
| `sort:updated-desc` | also `created-asc`, `comments-desc`, `updated-asc`, … |
| any other words | text in the title or the file; `"quoted words"` as a phrase |

On an issue you can edit the title, edit the description, tick checkboxes, set labels (type a new
name in the label menu to create one), comment, close, reopen, and edit or delete a comment. The
editor has a Preview tab; Ctrl + Enter submits. `/` jumps to the search box and `c` opens a new
issue.

Markdown is rendered like on GitHub: tables, task lists, strikethrough and fenced code. Raw HTML
is escaped, except a few tags without attributes (`details`, `summary`, `kbd`, `sub`, `sup`, …).
HTML comments are hidden. `#N` and links to `NNN-slug.md` open that issue.

### The board

**Board** in the header shows every project's issues as a kanban board; the Projects menu narrows
it to one or several, and a project's own Board tab shows just that project. It is meant for
following several projects, and several people or agents working on them, at once: which issue
is being worked on where, and which ones wait for you.

| Column | Labels that put an issue there | Waits for you |
|---|---|---|
| Backlog | `backlog`, `later`, `icebox`, `someday`, or no status at all | yes, a decision whether to do it |
| Selected for development | `selected`, `ready`, `todo`, `approved`, `next` | |
| In progress | `in-progress`, `doing`, `wip`, `started` | |
| Testing | `testing`, `in-testing`, `qa` | |
| Review | `review`, `in-review`, `needs-review`, `awaiting-review` | yes, a review or a merge |
| Blocked | `blocked`, `on-hold`, `waiting`, `needs-decision` | yes, a decision or an action |
| Done | `done`, or any closed issue | |

Labels are compared without regard to case, spaces or underscores (`In Progress`, `in_progress`).
A custom `state` such as `in-review` counts like a label. A closed issue is always in Done; when an
open issue has several status labels, the column further right wins, so a forgotten `backlog`
next to `blocked` does not hide it. Other labels stay on the card, and `blocked_by` /
`depends_on` in the front matter, or a "Blocked by" line, show which issues it waits for. Done
shows the ten most recently changed; "Show N more" shows the rest.

When an issue directory is inside a git repository, the board also reads its branches, read-only.
A branch whose name starts with an issue number (`007-normalise-owner-field`) is shown on that
issue's card with its worktree, the commits it is ahead of `main`, the uncommitted changes in its
worktree and when it last changed. Where several issues share the number, the one whose file name
shares the most words with the branch name gets it. An issue with no status label goes to In
progress, marked "from git", while its branch is checked out in a worktree or has commits ahead
of `main` from the last 14 days. A worktree on a branch no issue claims is listed under "Work
without an issue".

Drag a card to another column, or use its ⋯ menu, to move it. Moving replaces the issue's status
labels with the column's label, spelled the way the project already spells it, and keeps the rest;
moving to Done closes the issue and moving out of Done reopens it. A card whose file changed after
the board was loaded is not moved, and read-only projects cannot be moved at all. The search box
takes the same queries as the issue list.

### GitHub on the board

With `GITHUB_REPOS` set, the board also shows the issues and pull requests of GitHub
repositories, next to the issue files. The viewer holds no token: it runs the GitHub CLI, `gh`,
which has to be installed and signed in (`gh auth login`) for the user the viewer runs as, and
only reads.

```
GITHUB_REPOS=acme-notes/notes-api=/Users/you/dev/notes-api,acme-notes/*
```

- `owner/name` is one repository, `owner/*` every repository of that user or organisation that
  is neither archived nor a fork (up to 100), and `=/absolute/path` names the repository's local
  checkout, whose numbered branches then count as work like an issue file's.
- Issues go to columns the same way: their labels name the column, a closed issue is in Done.
  An open pull request that closes an issue (`Closes #11`) puts it in Review, or in In progress
  while it is a draft or has changes requested, marked "from PR", unless a label already puts it
  further right. The card lists those pull requests with their review and checks.
- A pull request that closes none of the issues on the board is a card of its own.
- Cards show the repository, labels, assignees, open blockers and comments, and link to GitHub
  in a new tab. They cannot be dragged or moved: change them on GitHub.
- The Projects menu lists the repositories, and `?project=owner/name` shows one of them.

The board never waits for GitHub. While a board is open, the viewer reads every repository again
in the background at most every `GITHUB_REFRESH_SECONDS` (one GraphQL query each, four at a time),
and the board reloads when the answer changed. The first board after a start says it is reading
GitHub. A repository that cannot be read is named with gh's message, and its cards stay as they
were last read. Each query reads the 100 most recently updated open issues, the 20 most recently
closed and 50 open pull requests; a repository with more says so.

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `PROJECTS` | – | `name=/absolute/path[:ro],…`, the directories to show |
| `OPEN_ROOT` | – | `/absolute/path[:ro]`, the tree **Open** browses; empty turns Open off |
| `PORT` | `8090` | port on 127.0.0.1; `0` lets the system pick one |
| `ALLOWED_HOSTS` | – | extra host names to answer to; `localhost` and `127.0.0.1` always work |
| `TZ` | the system's | time zone for comment timestamps and dates, for example `Europe/Warsaw` |
| `REFRESH_SECONDS` | `3` | how often open pages check for changes; `0` turns it off |
| `LOG_LEVEL` | `INFO` | `DEBUG`, `INFO`, `WARNING` or `ERROR`; the log goes to stdout |
| `GITHUB_REPOS` | – | `owner/name[=/absolute/checkout]` or `owner/*`, comma-separated: GitHub repositories for the board, read with `gh` |
| `GITHUB_REFRESH_SECONDS` | `120` | how often the board reads GitHub again while it is open; at least `30` |

## Security

There is no login, so the viewer relies on where requests come from:

- It listens on 127.0.0.1 only, never on another interface.
- The Host header must be `localhost`, `127.0.0.1` or a name in `ALLOWED_HOSTS`, which stops DNS
  rebinding.
- A write must come from the viewer's own page (Origin and Sec-Fetch-Site) and carry the token
  the page embeds, which stops another site from posting a form to it. The token is new every
  time the viewer starts.
- It reads and writes only the directories in `PROJECTS` and under `OPEN_ROOT`, and never writes
  to a `:ro` one. An issue is addressed by project and number, never by path. Open is the one
  place a request names a folder, always relative to `OPEN_ROOT`: hidden folders and `..` are
  refused, a symbolic link may not lead out of the root, and opening a folder is a write like any
  other, with the same origin and token checks.
- A request body larger than 4 MB is refused.
- GitHub is read through `gh` with the sign-in it already has; the viewer never sees a token,
  only reads, and runs `gh` without a terminal, so it cannot prompt. Every `gh` and `git` call
  has a time limit.
- Pages load nothing from outside the viewer: no CDN, no web fonts, no external images. The
  Content Security Policy enforces it.

## Development

The code is Rust (axum, minijinja); the pages are plain HTML, CSS and JavaScript with no build
step. The smoke and browser tests are Python and run in a Playwright image in Docker, against the
release binary running on the host.

| Target | What it does |
|---|---|
| `make build` | the release binary, `target/release/issue-viewer` |
| `make test` | unit and web tests (`cargo test`) |
| `make lint` | rustfmt check and clippy with warnings as errors |
| `make format` | format with rustfmt |
| `make smoke` | the release binary on synthetic issues, checked over HTTP |
| `make e2e` | the same, driven through Chromium with Playwright |
| `make check` | all of the above, what must pass before a merge |
| `make openapi` | regenerate `openapi.json`, the published schema of the routes |

Test data is synthetic: the made-up project in `tests/fixtures`, which the Rust tests and the
Python tests both copy. There is no CI: the viewer only runs on this machine, so `make check`, run
here before a merge, is the gate. See `CONTRIBUTING.md` for the conventions.
