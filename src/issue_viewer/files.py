"""Reading and writing issue files on disk.

Other programs edit the same files while the viewer runs, so a write goes to a temporary file
next to the target and replaces it in one rename: a reader sees the old file or the new one,
never half of it. The temporary name starts with a dot, so it never looks like an issue.
"""

from __future__ import annotations

import contextlib
import hashlib
import os
import secrets
from dataclasses import dataclass
from pathlib import Path


def digest(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()[:16]


@dataclass(frozen=True)
class FileText:
    text: str  # newlines normalised to \n
    newline: str  # the newline the file uses on disk
    version: str  # digest of text, what a form sends back to prove it saw this version
    readable: bool = True  # False when the bytes were not UTF-8 and had to be replaced


def read(path: Path) -> FileText:
    raw = path.read_bytes()
    readable = True
    try:
        text = raw.decode("utf-8")
    except UnicodeDecodeError:
        text = raw.decode("utf-8", errors="replace")
        readable = False
    text = text.removeprefix("﻿")
    newline = "\r\n" if "\r\n" in text else "\n"
    text = text.replace("\r\n", "\n")
    return FileText(text, newline, digest(text), readable)


def _encode(text: str, newline: str) -> bytes:
    if newline != "\n":
        text = text.replace("\n", newline)
    return text.encode("utf-8")


def write_atomic(path: Path, text: str, newline: str = "\n") -> None:
    temporary = path.with_name(f".{path.name}.{secrets.token_hex(4)}.tmp")
    try:
        mode = path.stat().st_mode & 0o7777
    except FileNotFoundError:
        mode = None
    try:
        with open(temporary, "xb") as handle:
            handle.write(_encode(text, newline))
            handle.flush()
            os.fsync(handle.fileno())
        if mode is not None:
            os.chmod(temporary, mode)
        os.replace(temporary, path)
    finally:
        with contextlib.suppress(FileNotFoundError):
            temporary.unlink()


def create_exclusive(path: Path, text: str) -> None:
    """Write a new file, failing if one with that name appeared in the meantime."""
    with open(path, "xb") as handle:
        handle.write(_encode(text, "\n"))
        handle.flush()
        os.fsync(handle.fileno())
