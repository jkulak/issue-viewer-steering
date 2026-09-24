"""Keeping the index table in the issues directory's README.md in step with the issues.

The viewer rewrites only the row of the issue it changed and appends a row for an issue it created.
Everything else stays as it was: hand-shortened titles, the other rows, the text around the table.
A README without an index table is left alone.
"""

from __future__ import annotations

import logging
import re
from collections.abc import Iterable
from pathlib import Path

from . import files

log = logging.getLogger(__name__)

README = "README.md"
EMPTY = "–"
CELL_SPLIT = re.compile(r"(?<!\\)\|")
SEPARATOR_CELL = re.compile(r"^:?-+:?$")
LINK = re.compile(r"^\[(.*)\]\(([^)]*)\)$")
DEPENDS_COLUMNS = ("depends on", "depends", "blocked by")


def _cells(line: str) -> list[str]:
    parts = CELL_SPLIT.split(line.strip())
    if parts and not parts[0].strip():
        parts = parts[1:]
    if parts and not parts[-1].strip():
        parts = parts[:-1]
    return [part.strip() for part in parts]


def _is_row(line: str) -> bool:
    return line.lstrip().startswith("|")


def _find_table(lines: list[str]) -> tuple[list[str], int, int] | None:
    """(column names, first row, end) of the first table whose first column is `#`."""
    for index in range(len(lines) - 1):
        if not _is_row(lines[index]) or not _is_row(lines[index + 1]):
            continue
        header = _cells(lines[index])
        separator = _cells(lines[index + 1])
        if not header or header[0] != "#" or not all(SEPARATOR_CELL.match(c) for c in separator):
            continue
        end = index + 2
        while end < len(lines) and _is_row(lines[end]):
            end += 1
        return [name.strip().lower() for name in header], index + 2, end
    return None


def _row_number(cells: list[str]) -> int | None:
    match = re.search(r"\d+", cells[0]) if cells else None
    return int(match.group()) if match else None


def _escape(text: str) -> str:
    return text.replace("\\", "\\\\").replace("|", "\\|").replace("[", "\\[").replace("]", "\\]")


def _refs(numbers: Iterable[int]) -> str:
    return ", ".join(f"#{number}" for number in numbers) or EMPTY


def _join(cells: list[str]) -> str:
    return "| " + " | ".join(cells) + " |"


def _column(columns: list[str], *names: str) -> int | None:
    return next((i for i, column in enumerate(columns) if column in names), None)


def updated_row(
    readme: str,
    number: int,
    *,
    title: str | None = None,
    filename: str | None = None,
    labels: Iterable[str] | None = None,
    state: str | None = None,
    depends_on: Iterable[int] | None = None,
) -> str | None:
    """The README with the row of `number` updated, or None when there is nothing to change."""
    lines = readme.split("\n")
    table = _find_table(lines)
    if table is None:
        return None
    columns, first, end = table
    for index in range(first, end):
        cells = _cells(lines[index])
        if _row_number(cells) != number:
            continue
        cells += [""] * (len(columns) - len(cells))
        position = _column(columns, "title")
        if title is not None and position is not None:
            link = LINK.match(cells[position])
            target = link.group(2) if link else filename
            cells[position] = f"[{_escape(title)}]({target})" if target else _escape(title)
        position = _column(columns, "labels")
        if labels is not None and position is not None:
            cells[position] = ", ".join(labels) or EMPTY
        position = _column(columns, "state")
        if state is not None and position is not None:
            cells[position] = state
        position = _column(columns, *DEPENDS_COLUMNS)
        if depends_on is not None and position is not None:
            cells[position] = _refs(depends_on)
        row = _join(cells)
        if row == lines[index]:
            return None
        lines[index] = row
        return "\n".join(lines)
    return None


def appended_row(
    readme: str,
    number: int,
    title: str,
    filename: str,
    labels: Iterable[str],
    state: str,
    depends_on: Iterable[int],
) -> str | None:
    """The README with a row for a new issue at the end of the index table."""
    lines = readme.split("\n")
    table = _find_table(lines)
    if table is None:
        return None
    columns, first, end = table
    labels, depends_on = list(labels), list(depends_on)
    if any(_row_number(_cells(lines[index])) == number for index in range(first, end)):
        return updated_row(
            readme,
            number,
            title=title,
            filename=filename,
            labels=labels,
            state=state,
            depends_on=depends_on,
        )
    values = {
        "#": str(number),
        "title": f"[{_escape(title)}]({filename})",
        "labels": ", ".join(labels) or EMPTY,
        "state": state,
    }
    cells = [
        _refs(depends_on) if column in DEPENDS_COLUMNS else values.get(column, EMPTY)
        for column in columns
    ]
    lines.insert(end, _join(cells))
    return "\n".join(lines)


def rewrite(directory: Path, change) -> None:
    """Apply `change(readme text) -> new text | None` to the README, if there is one.

    The README belongs to the person keeping the notes, so a failure here is logged and never
    undoes the change to the issue itself.
    """
    path = directory / README
    try:
        for _ in range(3):
            if not path.is_file():
                return
            current = files.read(path)
            if not current.readable:
                return
            new = change(current.text)
            if new is None or new == current.text:
                return
            if files.read(path).version != current.version:
                continue  # someone else wrote it meanwhile; start again from their version
            files.write_atomic(path, new, current.newline)
            return
    except OSError:
        log.warning("could not update %s", path, exc_info=True)
