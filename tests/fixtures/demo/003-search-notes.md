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
