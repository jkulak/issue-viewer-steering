"""Front matter: the YAML block between two `---` lines at the top of an issue file.

PyYAML reads it. Writing never re-serialises the block: an edit replaces the lines of one key and
leaves every other line, comment and quoting style as it was, so a hand-edited file changes only
where the viewer meant to change it.
"""

from __future__ import annotations

import json
import re
from dataclasses import dataclass, field

import yaml

OPENING = re.compile(r"^---[ \t]*$")
CLOSING = re.compile(r"^(?:---|\.\.\.)[ \t]*$")
KEY_LINE = re.compile(r"^([A-Za-z_][\w-]*)[ \t]*:(?:[ \t]|$)")


@dataclass(frozen=True)
class FrontMatter:
    present: bool = False
    raw_start: int = 0  # first character of the YAML text, after the opening line
    raw_end: int = 0  # first character of the closing line
    body_start: int = 0  # first character after the closing line
    data: dict = field(default_factory=dict)
    error: str | None = None

    def raw(self, text: str) -> str:
        return text[self.raw_start : self.raw_end]


def split(text: str) -> FrontMatter:
    first_end = text.find("\n")
    if first_end == -1 or not OPENING.match(text[:first_end]):
        return FrontMatter()
    position = first_end + 1
    while position < len(text):
        line_end = text.find("\n", position)
        line = text[position:] if line_end == -1 else text[position:line_end]
        if CLOSING.match(line):
            data, error = load(text[first_end + 1 : position])
            body_start = len(text) if line_end == -1 else line_end + 1
            return FrontMatter(True, first_end + 1, position, body_start, data, error)
        if line_end == -1:
            break
        position = line_end + 1
    # An opening line that is never closed is a thematic break, not front matter.
    return FrontMatter()


def load(raw: str) -> tuple[dict, str | None]:
    try:
        data = yaml.safe_load(raw) if raw.strip() else {}
    except yaml.YAMLError as error:
        return {}, describe(error)
    if data is None:
        return {}, None
    if not isinstance(data, dict):
        return {}, "it is not a list of key: value lines"
    return data, None


def describe(error: yaml.YAMLError) -> str:
    problem = getattr(error, "problem", None) or str(error).splitlines()[0]
    mark = getattr(error, "problem_mark", None)
    # The YAML starts on the second line of the file.
    return f"{problem} (line {mark.line + 2})" if mark is not None else problem


def _key_of(line: str) -> str | None:
    match = KEY_LINE.match(line)
    return match.group(1) if match else None


def _continues(line: str) -> bool:
    """True for a line that belongs to the value of the key above it."""
    return bool(line.strip()) and (line[0] in " \t" or line.startswith("-"))


def _key_lines(lines: list[str], key: str) -> tuple[int, int] | None:
    start = next((i for i, line in enumerate(lines) if _key_of(line) == key), None)
    if start is None:
        return None
    end = start + 1
    while end < len(lines):
        if _continues(lines[end]):
            end += 1
            continue
        if not lines[end].strip():
            # A blank line is part of the value only when the value goes on after it.
            after = end
            while after < len(lines) and not lines[after].strip():
                after += 1
            if after < len(lines) and _continues(lines[after]):
                end = after + 1
                continue
        break
    return start, end


def set_key(raw: str, key: str, rendered: str | None) -> str:
    """Put `key: rendered` in place of the lines of `key`; None removes it; new keys go last."""
    lines = raw.splitlines(keepends=True)
    found = _key_lines(lines, key)
    replacement = [] if rendered is None else [f"{key}: {rendered}\n"]
    if found is None:
        if rendered is None:
            return raw
        if lines and not lines[-1].endswith("\n"):
            lines[-1] += "\n"
        return "".join(lines + replacement)
    start, end = found
    return "".join(lines[:start] + replacement + lines[end:])


def style_of(raw: str, key: str) -> str:
    """How the value of `key` is quoted now: 'double', 'single' or 'plain'."""
    for line in raw.splitlines():
        if _key_of(line) == key:
            value = line.split(":", 1)[1].lstrip()
            if value.startswith('"'):
                return "double"
            if value.startswith("'"):
                return "single"
            return "plain"
    return "plain"


def _plain_ok(value: str) -> bool:
    if not value or value != value.strip() or "\n" in value:
        return False
    try:
        return yaml.safe_load(f"k: {value}") == {"k": value}
    except yaml.YAMLError:
        return False


def render_scalar(value: str, style: str = "plain") -> str:
    if style == "single" and "\n" not in value:
        return "'" + value.replace("'", "''") + "'"
    if style == "plain" and _plain_ok(value):
        return value
    # A JSON string is a valid YAML double-quoted scalar.
    return json.dumps(value, ensure_ascii=False)


def _flow_plain_ok(item: str) -> bool:
    if not item or item != item.strip() or any(char in item for char in ",[]{}\"'\n"):
        return False
    try:
        return yaml.safe_load(f"[{item}]") == [item]
    except yaml.YAMLError:
        return False


def render_list(items: list[str] | tuple[str, ...]) -> str:
    parts = [
        item if _flow_plain_ok(item) else json.dumps(item, ensure_ascii=False) for item in items
    ]
    return "[" + ", ".join(parts) + "]"
