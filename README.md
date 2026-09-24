# Issue viewer

A local web app that shows a project's issue files the way GitHub shows issues, and edits them
in place. Each project keeps its issues as Markdown files, `tasks/NNN-slug.md`. The viewer lists,
filters, renders and changes those files and nothing else. There is no database, no login and no
users: the files are the source of truth, and they stay readable and editable by hand.

## Quick start

Only Docker is needed on the host.

```bash
cp .env.example .env
```

Put the issue directories you want to see in `PROJECTS` in `.env`, then:

```bash
make up
```

The viewer runs on <http://localhost:8090> (`PORT` in `.env`). `make down` stops it, `make logs`
follows its log, `make help` lists every target.

`PROJECTS` is a comma-separated list of `name=/absolute/path/on/the/host`. Only those
directories are mounted into the container. An entry ending in `:ro` is mounted read-only and
shown without any write control:

```dotenv
PROJECTS=notes=/Users/you/dev/notes/tasks,archive=/Users/you/dev/old-project/tasks:ro
```

Run `make up` again after changing the list: it regenerates `compose.override.yaml`, the file
that holds the mounts.

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

## Configuration

| Variable | Default | Meaning |
|---|---|---|
| `PROJECTS` | – | `name=/absolute/path[:ro],…`, the directories to show |
| `PORT` | `8090` | port on 127.0.0.1 |
| `ALLOWED_HOSTS` | – | extra host names to answer to; `localhost` and `127.0.0.1` always work |
| `TZ` | `UTC` | time zone for comment timestamps and dates |
| `REFRESH_SECONDS` | `3` | how often open pages check for changes; `0` turns it off |
| `LOG_LEVEL` | `INFO` | log level; the log goes to stdout |

## Security

There is no login, so the viewer relies on where requests come from:

- The port is published on 127.0.0.1 only.
- The Host header must be `localhost`, `127.0.0.1` or a name in `ALLOWED_HOSTS`, which stops DNS
  rebinding.
- A write must come from the viewer's own page (Origin and Sec-Fetch-Site) and carry the token
  the page embeds, which stops another site from posting a form to it.
- Only the configured directories are mounted; `:ro` ones are read-only for Docker as well. No
  request can name a path: an issue is addressed by project and number.
- The container runs as a non-root user on a read-only root filesystem, with every capability
  dropped.
- Pages load nothing from outside the viewer: no CDN, no web fonts, no external images. The
  Content Security Policy enforces it.

## Development

Everything runs in Docker.

| Target | What it does |
|---|---|
| `make test` | unit and web tests (pytest) |
| `make lint` | ruff check and format check |
| `make format` | fix and format with ruff |
| `make smoke` | the production image on synthetic issues, checked over HTTP |
| `make e2e` | the same, driven through Chromium with Playwright |
| `make check` | all of the above, as CI runs it |
| `make openapi` | regenerate `openapi.json`, the published schema of the routes |
| `make lock` | refresh `uv.lock` after a dependency change |

Test data is synthetic: `tests/support/sample.py` writes a made-up project. CI runs `make check`
on pull requests, on version tags and on demand, never on a push to `main`. See `CONTRIBUTING.md`
for the conventions.
