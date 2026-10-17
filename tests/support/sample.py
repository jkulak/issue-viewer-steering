"""Synthetic issue directories for the smoke and browser tests: a made-up notes app, nothing from
a real project. The files live in tests/fixtures, which the Rust tests copy too.

Standard library only, so it runs in the Playwright image as it is.
"""

from __future__ import annotations

import shutil
from pathlib import Path

FIXTURES = Path(__file__).resolve().parents[1] / "fixtures"


def write_project(directory: Path, fixture: str, shared: bool = False) -> Path:
    """Copy one fixture directory over `directory`, removing whatever else is in it.

    `shared` makes everything writable by any user, for a directory the viewer (possibly
    another user than the tests) writes into.
    """
    directory.mkdir(parents=True, exist_ok=True)
    for old in directory.iterdir():
        if old.is_file():
            old.unlink()
    for source in sorted((FIXTURES / fixture).iterdir()):
        path = directory / source.name
        shutil.copyfile(source, path)
        if shared:
            path.chmod(0o666)
    if shared:
        directory.chmod(0o777)
    return directory


def write_demo(directory: Path, shared: bool = False) -> Path:
    return write_project(directory, "demo", shared)


def write_archive(directory: Path, shared: bool = False) -> Path:
    return write_project(directory, "archive", shared)
