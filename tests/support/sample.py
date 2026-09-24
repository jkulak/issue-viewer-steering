"""Synthetic issue directories for the tests: a made-up notes app, nothing from a real project.

Standard library only, so the browser tests can use it in the Playwright image too.
"""

from __future__ import annotations

from pathlib import Path

DEMO = {
    "001-set-up-the-project-skeleton.md": """\
---
title: "Set up the project skeleton"
labels: [enhancement, infra]
state: done
---

# #1 Set up the project skeleton

**Depends on:** – · **Blocks:** #2

## Context

The notes app needs a package, a container and a Makefile before anything else.

## Acceptance criteria

- [x] `make up` starts the app
- [x] `make test` runs the tests
""",
    "002-store-notes-in-sqlite.md": """\
---
title: Store notes in SQLite
labels: [enhancement, data]
state: done
---

# #2 Store notes in SQLite

**Depends on:** #1 · **Blocks:** #3

Notes live in one SQLite file next to the app.

- [x] A note survives a restart
""",
    "003-search-notes.md": """\
---
title: "Search notes by title and body"
labels: [enhancement, ui]
state: open
---

# #3 Search notes by title and body

**Depends on:** #2 · **Blocks:** –

## Context

With more than a few dozen notes, scrolling stops working. The notes live where
[the storage issue](002-store-notes-in-sqlite.md) put them.

## Acceptance criteria

- [x] Typing filters the list as you type
- [ ] Matches are highlighted
- [ ] An empty search shows every note

| Field | Searched |
|---|---|
| title | yes |
| body | yes |

## Comments

### 2026-09-20 10:15

Highlighting can wait until the list is fast.

### 2026-09-21 16:40

Measured it: 3 ms for 2000 notes, so highlighting is fine.

- [ ] Try it with 20000 notes
""",
    "004-crash-on-empty-title.md": """\
---
title: The app crashes when a note title is empty
labels: [bug]
state: open
---

# #4 The app crashes when a note title is empty

**Depends on:** – · **Blocks:** –

Saving a note without a title raises `ValueError` and the window closes.

1. Create a note
2. Clear the title
3. Press save
""",
    "005-dark-theme.md": """\
---
title: "Dark theme"
labels: [enhancement, ui, backlog]
state: open
---

# #5 Dark theme

**Depends on:** #3 · **Blocks:** –

Follow the system setting.
""",
    "006-export-notes-to-markdown.md": """\
---
title: "Export notes to Markdown"
labels: [enhancement]
state: wontfix
---

# #6 Export notes to Markdown

Copying the SQLite file is enough for now.
""",
    "007-keyboard-shortcuts.md": """\
---
title: "Keyboard shortcuts"
labels: []
state: open
---

# #7 Keyboard shortcuts

- `n` for a new note
- `/` to search
""",
    "008-sync-between-devices.md": """\
---
title: "Sync between devices"
labels: [enhancement, backlog]
state: open
---

# #8 Sync between devices

**Depends on:** #2, #3 · **Blocks:** –

<!-- whoever picks this up: start with the conflict rules -->

Relates to #4 and #99. ~~Dropbox~~ is out, see <https://example.com/sync>.

<details>
<summary>Why not a server</summary>

Nobody wants to run one.
</details>

<script>alert("never runs")</script>

```markdown
## Comments

### 2026-01-01 00:00

A code block, not a comment.
```

- [ ] Pick a sync format
""",
    "009-typo-on-the-settings-page.md": """\
---
title: Typo on the settings page
labels: [bug, ui]
state: done
---

# #9 Typo on the settings page

"Preferances" should read "Preferences".
""",
    "010-document-the-configuration.md": """\
---
title: "Document the configuration"
labels: [documentation]
state: in-progress
---

# #10 Document the configuration

Every setting, with its default.
""",
    "011-plain-file.md": """\
# Plain file without front matter

Nothing but a heading and a line.
""",
    "012-broken-front-matter.md": """\
---
title: [this is not closed
labels: [bug
---

# #12 Broken front matter
""",
}
FILLERS = range(13, 41)

DEMO_README = """\
# Issues

One file per issue, numbered in order. `state` and `labels` live in the front matter.

| # | Title | Labels | State | Depends on |
|---|---|---|---|---|
| 1 | [Set up the project skeleton](001-set-up-the-project-skeleton.md) | enhancement, infra | done | – |
| 2 | [Store notes in SQLite](002-store-notes-in-sqlite.md) | enhancement, data | done | #1 |
| 3 | [Search notes](003-search-notes.md) | enhancement, ui | open | #2 |
| 4 | [Crash on empty title](004-crash-on-empty-title.md) | bug | open | – |

Order of work: 1 → 2 → 3.
"""

ARCHIVE = {
    "001-an-old-idea.md": """\
---
title: "An old idea"
labels: [idea]
state: done
---

# #1 An old idea

- [x] Written down
""",
    "002-another-old-idea.md": """\
---
title: "Another old idea"
labels: [idea]
state: open
---

# #2 Another old idea

- [ ] Think it through
""",
}


def filler(number: int) -> str:
    label = ("bug", "ui", "data", "documentation")[number % 4]
    state = "done" if number % 3 == 0 else "open"
    return (
        f'---\ntitle: "Filler issue {number}"\nlabels: [{label}]\nstate: {state}\n---\n\n'
        f"# #{number} Filler issue {number}\n\nPadding, so the list has more than one page.\n"
    )


def write_project(
    directory: Path, issues: dict[str, str], readme: str | None, shared: bool = False
) -> Path:
    """Write the files, removing whatever else is in the directory.

    `shared` makes everything writable by any user, for a directory the app container
    (a different user) writes into during the browser tests.
    """
    directory.mkdir(parents=True, exist_ok=True)
    for old in directory.iterdir():
        if old.is_file():
            old.unlink()
    files = dict(issues)
    if readme is not None:
        files["README.md"] = readme
    for name, text in files.items():
        path = directory / name
        path.write_text(text, encoding="utf-8")
        if shared:
            path.chmod(0o666)
    if shared:
        directory.chmod(0o777)
    return directory


def demo_issues() -> dict[str, str]:
    return {**DEMO, **{f"{n:03d}-filler-issue-{n}.md": filler(n) for n in FILLERS}}


def write_demo(directory: Path, shared: bool = False) -> Path:
    return write_project(directory, demo_issues(), DEMO_README, shared)


def write_archive(directory: Path, shared: bool = False) -> Path:
    return write_project(directory, ARCHIVE, None, shared)
