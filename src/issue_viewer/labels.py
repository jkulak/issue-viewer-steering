"""Colours for labels, which in a file are only names.

GitHub's default labels keep their usual colours; any other name gets one from a fixed palette,
chosen by a hash of the name, so a label looks the same on every page and after every restart.
"""

from __future__ import annotations

import hashlib

KNOWN = {
    "bug": "d73a4a",
    "documentation": "0075ca",
    "duplicate": "cfd3d7",
    "enhancement": "a2eeef",
    "good first issue": "7057ff",
    "help wanted": "008672",
    "invalid": "e4e669",
    "question": "d876e3",
    "wontfix": "ffffff",
    "backlog": "ededed",
}
PALETTE = (
    "0e8a16",
    "1d76db",
    "5319e7",
    "b60205",
    "d93f0b",
    "fbca04",
    "006b75",
    "0052cc",
    "c5def5",
    "bfd4f2",
    "d4c5f9",
    "f9d0c4",
    "fef2c0",
    "c2e0c6",
    "bfdadc",
    "e99695",
    "84b6eb",
    "fad8c7",
    "bfe5bf",
    "c9a8ff",
)
AVATARS = ("1f883d", "0969da", "8250df", "bf3989", "cf222e", "bc4c00", "1b7c83", "4d2d8f")


def _pick(name: str, choices: tuple[str, ...]) -> str:
    return choices[int(hashlib.sha256(name.casefold().encode()).hexdigest(), 16) % len(choices)]


def color(name: str) -> str:
    return KNOWN.get(name.casefold()) or _pick(name, PALETTE)


def text_color(background: str) -> str:
    red, green, blue = (int(background[i : i + 2], 16) for i in (0, 2, 4))
    lightness = (red * 0.2126 + green * 0.7152 + blue * 0.0722) / 255
    return "#1f2328" if lightness > 0.453 else "#ffffff"


def style(name: str) -> str:
    background = color(name)
    return f"background-color:#{background};color:{text_color(background)}"


def avatar_style(name: str) -> str:
    return f"background-color:#{_pick(name, AVATARS)}"
