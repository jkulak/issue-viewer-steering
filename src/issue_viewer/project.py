"""One configured issues directory: reading it, and writing to it safely."""

from __future__ import annotations

import logging
import os
import threading
from collections.abc import Callable
from dataclasses import dataclass, field
from functools import cached_property
from pathlib import Path

from . import files, index
from .issuefile import EditError, new_issue_text, parse
from .issues import FILENAME, Issue, build, slugify
from .settings import ProjectConfig

log = logging.getLogger(__name__)


class ReadOnlyError(PermissionError):
    """The project is read-only, by configuration or because the directory cannot be written."""


class ConflictError(RuntimeError):
    """The file changed on disk since the version the edit was based on."""


class NotFoundError(LookupError):
    pass


@dataclass(frozen=True)
class Snapshot:
    """The issues of a project as they are on disk right now, one per number."""

    issues: tuple[Issue, ...]
    duplicates: tuple[str, ...] = ()  # files hidden because an earlier file has their number
    by_number: dict[int, Issue] = field(init=False)

    def __post_init__(self) -> None:
        object.__setattr__(self, "by_number", {issue.number: issue for issue in self.issues})

    @property
    def open_count(self) -> int:
        return sum(issue.is_open for issue in self.issues)

    @property
    def closed_count(self) -> int:
        return len(self.issues) - self.open_count

    @cached_property
    def labels(self) -> list[str]:
        names = {label for issue in self.issues for label in issue.labels}
        return sorted(names, key=str.casefold)

    def blocking(self, number: int) -> list[int]:
        """Issues this one blocks: its own `Blocks` list plus every issue that depends on it."""
        issue = self.by_number.get(number)
        found = list(issue.blocks) if issue else []
        found += [other.number for other in self.issues if number in other.depends_on]
        return sorted(set(found) - {number})

    def open_dependencies(self, issue: Issue) -> list[Issue]:
        return [
            self.by_number[number]
            for number in issue.depends_on
            if number in self.by_number and self.by_number[number].is_open
        ]


class Project:
    def __init__(self, config: ProjectConfig) -> None:
        self.config = config
        self.name = config.name
        self.path = config.path
        self._cache: dict[str, tuple[int, int, Issue]] = {}
        self._lock = threading.RLock()

    @property
    def exists(self) -> bool:
        return self.path.is_dir()

    @property
    def writable(self) -> bool:
        return not self.config.read_only and self.exists and os.access(self.path, os.W_OK)

    def _files(self) -> list[tuple[str, int, os.stat_result]]:
        found = []
        try:
            with os.scandir(self.path) as entries:
                for entry in entries:
                    match = FILENAME.match(entry.name)
                    if match and entry.is_file():
                        found.append((entry.name, int(match.group(1)), entry.stat()))
        except OSError:
            return []
        return sorted(found, key=lambda item: (item[1], item[0]))

    def signature(self) -> str:
        """Changes whenever an issue file is added, removed or modified."""
        listing = "".join(
            f"{name}\0{stat.st_mtime_ns}\0{stat.st_size}\n" for name, _, stat in self._files()
        )
        return files.digest(listing)

    def _read(self, name: str, number: int) -> Issue:
        path = self.path / name
        stat = path.stat()
        return build(path, number, files.read(path), stat.st_mtime)

    def snapshot(self) -> Snapshot:
        issues: list[Issue] = []
        duplicates: list[str] = []
        seen: set[int] = set()
        with self._lock:
            listed = self._files()
            for name, number, stat in listed:
                cached = self._cache.get(name)
                if cached and cached[:2] == (stat.st_mtime_ns, stat.st_size):
                    issue = cached[2]
                else:
                    try:
                        issue = self._read(name, number)
                    except FileNotFoundError:
                        continue
                    self._cache[name] = (stat.st_mtime_ns, stat.st_size, issue)
                if number in seen:
                    duplicates.append(name)
                else:
                    seen.add(number)
                    issues.append(issue)
            names = {name for name, _, _ in listed}
            for name in set(self._cache) - names:
                del self._cache[name]
        return Snapshot(tuple(issues), tuple(duplicates))

    def _path_of(self, number: int) -> tuple[str, int]:
        for name, found, _ in self._files():
            if found == number:
                return name, found
        raise NotFoundError(f"no issue #{number} in {self.name}")

    def fresh(self, number: int) -> Issue:
        """The issue read from disk now, bypassing the cache."""
        return self._read(*self._path_of(number))

    def version_of(self, number: int) -> str:
        """What a page showing this issue compares against to notice a change on disk."""
        name, _ = self._path_of(number)
        return files.read(self.path / name).version

    def update(
        self,
        number: int,
        edit: Callable[[Issue], str],
        expected_version: str | None = None,
    ) -> Issue:
        """Apply `edit(issue) -> new text` to the file and write it, if the text changed.

        With `expected_version`, the edit is refused when the file is no longer the version the
        user was looking at. Without it, the edit is applied to whatever is on disk now; a file
        that changes between reading and writing is read again and the edit applied again.
        """
        if not self.writable:
            raise ReadOnlyError(f"{self.name} is read-only")
        with self._lock:
            for _ in range(3):
                issue = self.fresh(number)
                if not issue.editable:
                    raise EditError(" ".join(issue.problems) or "this file cannot be edited here")
                if expected_version is not None and issue.version != expected_version:
                    raise ConflictError(f"issue #{number} changed on disk")
                text = edit(issue)
                if text == issue.file.text:
                    return issue
                if parse(text).front.error:
                    raise EditError("the edit would leave the front matter unreadable")
                if files.read(issue.path).version != issue.version:
                    continue
                files.write_atomic(issue.path, text, issue.newline)
                log.info("wrote %s/%s", self.name, issue.filename)
                after = self.fresh(number)
                self.sync_index(issue, after)
                return after
        raise ConflictError(f"issue #{number} keeps changing on disk")

    def create(self, title: str, labels: list[str], description: str) -> Issue:
        if not self.writable:
            raise ReadOnlyError(f"{self.name} is read-only")
        with self._lock:
            for _ in range(5):
                listed = self._files()
                number = max((found for _, found, _ in listed), default=0) + 1
                width = max(
                    (len(FILENAME.match(name).group(1)) for name, _, _ in listed), default=3
                )
                name = f"{number:0{width}d}-{slugify(title)}.md"
                try:
                    files.create_exclusive(
                        self.path / name, new_issue_text(number, title, labels, description)
                    )
                except FileExistsError:
                    continue
                # Another program may have taken the same number with a different name meanwhile.
                if any(found == number and other != name for other, found, _ in self._files()):
                    (self.path / name).unlink()
                    continue
                log.info("created %s/%s", self.name, name)
                break
            else:
                raise EditError("could not find a free issue number")
        issue = self.fresh(number)
        index.rewrite(
            self.path,
            lambda readme: index.appended_row(
                readme,
                issue.number,
                issue.title,
                issue.filename,
                issue.labels,
                issue.state,
                issue.depends_on,
            ),
        )
        return issue

    def sync_index(self, before: Issue, after: Issue) -> None:
        """Bring the README row of an issue in line with what just changed in its file."""
        changes: dict = {}
        if after.title != before.title:
            changes.update(title=after.title, filename=after.filename)
        if after.labels != before.labels:
            changes["labels"] = after.labels
        if after.state != before.state:
            changes["state"] = after.state
        if after.depends_on != before.depends_on:
            changes["depends_on"] = after.depends_on
        if changes:
            index.rewrite(
                self.path, lambda readme: index.updated_row(readme, after.number, **changes)
            )


class Registry:
    def __init__(self, configs: tuple[ProjectConfig, ...]) -> None:
        self.projects = {config.name: Project(config) for config in configs}
        # Open adds projects while other requests list them: it replaces the dict instead of
        # changing it, so a request that is going through the old one is not disturbed.
        self._lock = threading.Lock()

    def get(self, name: str) -> Project:
        try:
            return self.projects[name]
        except KeyError:
            raise NotFoundError(f"no project called {name!r}") from None

    def find(self, directory: Path) -> Project | None:
        """The project that shows this directory, however its path was written."""
        target = directory.resolve()
        return next((project for project in self if project.path.resolve() == target), None)

    def open(
        self, directory: Path, name_for: Callable[[set[str]], str], read_only: bool
    ) -> Project:
        """The project for `directory`, added under a new name if no project shows it yet."""
        with self._lock:
            existing = self.find(directory)
            if existing:
                return existing
            name = name_for(set(self.projects))
            project = Project(ProjectConfig(name, directory, read_only, opened=True))
            self.projects = {**self.projects, name: project}
        log.info("opened %s: %s", name, directory)
        return project

    def __iter__(self):
        return iter(self.projects.values())
