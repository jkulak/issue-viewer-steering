"""Walking the open root: which folders a person may see, and what is in them.

Open is the only way a request names a directory, so everything it is given is a path relative to
the root and is checked here before anything touches the disk under it.
"""

from __future__ import annotations

import os
import re
from collections.abc import Container
from dataclasses import dataclass
from pathlib import Path, PurePosixPath

from .issues import FILENAME
from .settings import PROJECT_NAME, RESERVED_NAMES

ISSUE_FOLDERS = ("tasks", "issues")
_UNSAFE = re.compile(r"[^A-Za-z0-9._-]+")


class BrowseError(LookupError):
    """The path is not a directory Open may show."""


@dataclass(frozen=True)
class Folder:
    name: str
    relative: str
    issues: int  # issue files directly inside it
    issue_folders: tuple[str, ...]  # `tasks/` or `issues/` folders inside it


def resolve(root: Path, relative: str) -> Path:
    """The directory `relative` names under `root`, or BrowseError.

    Hidden folders and `..` are refused outright; a symbolic link is followed only as far as the
    root, so it cannot lead anywhere the root does not contain.
    """
    parts = [part for part in relative.split("/") if part not in ("", ".")]
    if any(part.startswith(".") for part in parts):
        raise BrowseError("hidden folders and “..” cannot be opened")
    try:
        base = root.resolve()
        path = base.joinpath(*parts).resolve()
        inside = path == base or base in path.parents
        if not inside or not path.is_dir():
            raise BrowseError(f"there is no folder “{relative}” to open")
    except (OSError, ValueError):
        raise BrowseError(f"there is no folder “{relative}” to open") from None
    return path


def relative_to(root: Path, path: Path) -> str:
    """The path as Open addresses it: relative to the root, with `/`, empty for the root."""
    relative = path.relative_to(root.resolve()).as_posix()
    return "" if relative == "." else relative


def count_issues(directory: Path) -> int:
    try:
        with os.scandir(directory) as entries:
            return sum(1 for entry in entries if FILENAME.match(entry.name) and entry.is_file())
    except OSError:
        return 0


def folders(root: Path, directory: Path) -> list[Folder]:
    """The visible folders directly inside `directory`, by name."""
    found = []
    try:
        with os.scandir(directory) as entries:
            names = [
                entry.name
                for entry in entries
                if not entry.name.startswith(".") and entry.is_dir(follow_symlinks=False)
            ]
    except OSError:
        return []
    base = relative_to(root, directory)
    for name in sorted(names, key=str.casefold):
        path = directory / name
        found.append(
            Folder(
                name=name,
                relative=f"{base}/{name}" if base else name,
                issues=count_issues(path),
                issue_folders=tuple(
                    folder
                    for folder in ISSUE_FOLDERS
                    if (path / folder).is_dir() and not (path / folder).is_symlink()
                ),
            )
        )
    return found


def project_name(root: Path, relative: str, taken: Container[str]) -> str:
    """A project name for an opened folder, made from its path and free in `taken`.

    `notes-app/tasks` becomes `notes-app`: the folder that holds the issues says less than the one
    around it. A name longer than a project name may be keeps its end, the most specific part.
    """
    parts = list(PurePosixPath(relative).parts) or [root.name]
    if len(parts) > 1 and parts[-1] in ISSUE_FOLDERS:
        parts.pop()
    slugs = [_UNSAFE.sub("-", part).strip("-.") for part in parts]
    name = ".".join(slug for slug in slugs if slug)[-64:].lstrip("-._") or "folder"
    if name in RESERVED_NAMES or not PROJECT_NAME.match(name):
        name = "folder"
    candidate, number = name, 1
    while candidate in taken:
        number += 1
        suffix = f"-{number}"
        candidate = name[: 64 - len(suffix)] + suffix
    return candidate
