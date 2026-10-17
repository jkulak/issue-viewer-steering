"""Small SVG icons, drawn for this app on a 16×16 grid, in the current text colour."""

from __future__ import annotations

import math

from markupsafe import Markup

_STROKE = 'fill="none" stroke="currentColor" stroke-width="1.5"'
_THIN = 'fill="none" stroke="currentColor" stroke-width="1.3"'
_BOLD = 'fill="none" stroke="currentColor" stroke-width="1.75"'
_ROUND = 'stroke-linecap="round" stroke-linejoin="round"'


def _gear() -> str:
    points = []
    for tooth in range(8):
        centre = math.radians(tooth * 45)
        for offset, radius in ((-14, 5.3), (-8, 7.2), (8, 7.2), (14, 5.3)):
            angle = centre + math.radians(offset)
            points.append(f"{8 + radius * math.cos(angle):.2f},{8 + radius * math.sin(angle):.2f}")
    return (
        f'<path d="M{" L".join(points)} Z" {_THIN} stroke-linejoin="round"/>'
        f'<circle cx="8" cy="8" r="2.1" {_THIN}/>'
    )


_RING = f'<circle cx="8" cy="8" r="6.25" {_STROKE}/>'

SHAPES = {
    "issue-opened": f'{_RING}<circle cx="8" cy="8" r="1.6" fill="currentColor"/>',
    "issue-closed": f'{_RING}<path d="M5.4 8.3 7.2 10.1 10.7 6.4" {_STROKE} {_ROUND}/>',
    "issue-not-planned": f'{_RING}<path d="M3.9 12.1 12.1 3.9" {_STROKE} {_ROUND}/>',
    "blocked": f'{_RING}<path d="M4.9 8h6.2" {_STROKE} {_ROUND}/>',
    "comment": (
        '<path d="M2.75 2.5h10.5c.69 0 1.25.56 1.25 1.25v6.5c0 .69-.56 1.25-1.25 1.25H8.6'
        'L5.5 14v-2.5H2.75c-.69 0-1.25-.56-1.25-1.25v-6.5c0-.69.56-1.25 1.25-1.25Z" '
        f'{_STROKE} stroke-linejoin="round"/>'
    ),
    "tag": (
        '<path d="M1.75 2.75v4.5c0 .27.11.52.29.71l6.25 6.25a1 1 0 0 0 1.42 0l4.5-4.5a1 1 0 0 0 '
        '0-1.42L7.96 2.04a1 1 0 0 0-.71-.29h-4.5a1 1 0 0 0-1 1Z" '
        f'{_STROKE} stroke-linejoin="round"/>'
        '<circle cx="5" cy="5" r="1.1" fill="currentColor"/>'
    ),
    "search": (
        f'<circle cx="7" cy="7" r="4.75" {_STROKE}/>'
        f'<path d="m10.5 10.5 3.75 3.75" {_STROKE} {_ROUND}/>'
    ),
    "gear": _gear(),
    "triangle-down": (
        '<path d="M4.4 6.2h7.2a.4.4 0 0 1 .3.66L8.3 10.6a.4.4 0 0 1-.6 0'
        'L4.1 6.86a.4.4 0 0 1 .3-.66Z" fill="currentColor"/>'
    ),
    "check": f'<path d="m3 8.5 3 3 7-7" {_BOLD} {_ROUND}/>',
    "x": f'<path d="m4 4 8 8M12 4l-8 8" {_STROKE} {_ROUND}/>',
    "kebab": (
        '<circle cx="3" cy="8" r="1.3" fill="currentColor"/>'
        '<circle cx="8" cy="8" r="1.3" fill="currentColor"/>'
        '<circle cx="13" cy="8" r="1.3" fill="currentColor"/>'
    ),
    "repo": (
        '<path d="M3.25 13V3c0-.69.56-1.25 1.25-1.25h8.25v9.5H4.5c-.69 0-1.25.56-1.25 1.25'
        f'S3.81 14.25 4.5 14.25h8.25" {_STROKE} {_ROUND}/>'
        f'<path d="M6 4.75h4" {_STROKE} {_ROUND}/>'
    ),
    "lock": (
        f'<rect x="3" y="7" width="10" height="7.25" rx="1.25" {_STROKE}/>'
        f'<path d="M5.25 7V5a2.75 2.75 0 0 1 5.5 0v2" {_STROKE}/>'
    ),
    "folder": (
        '<path d="M1.75 3.5c0-.41.34-.75.75-.75h3.4l1.5 1.75h6.1c.41 0 .75.34.75.75v7.25c0 .41-.34'
        f'.75-.75.75H2.5a.75.75 0 0 1-.75-.75Z" {_STROKE} stroke-linejoin="round"/>'
    ),
    "file": (
        '<path d="M3.75 1.75h5.5l3 3v9.25a.25.25 0 0 1-.25.25h-8.25a.25.25 0 0 1-.25-.25'
        f'V2a.25.25 0 0 1 .25-.25Z" {_STROKE} stroke-linejoin="round"/>'
        f'<path d="M9 1.75V5h3.25" {_STROKE} stroke-linejoin="round"/>'
    ),
    "copy": (
        f'<rect x="5.25" y="5.25" width="8.5" height="8.5" rx="1.25" {_STROKE}/>'
        '<path d="M10.75 5.25V3.5c0-.69-.56-1.25-1.25-1.25H3.5c-.69 0-1.25.56-1.25 1.25v6c0 .69.56 '
        f'1.25 1.25 1.25h1.75" {_STROKE}/>'
    ),
    "pencil": (
        '<path d="M10.9 2.6a1.5 1.5 0 0 1 2.1 0l.4.4a1.5 1.5 0 0 1 0 2.1L6 12.5l-3.25.75.75-3.25Z" '
        f'{_STROKE} stroke-linejoin="round"/><path d="m9.75 3.75 2.5 2.5" {_STROKE}/>'
    ),
    "trash": (
        f'<path d="M2.75 4.25h10.5M6.25 4.25V2.75h3.5v1.5" {_STROKE} {_ROUND}/>'
        f'<path d="M4 4.25l.7 9a1 1 0 0 0 1 .95h4.6a1 1 0 0 0 1-.95l.7-9" {_STROKE} {_ROUND}/>'
    ),
    "link": (
        '<path d="M6.9 9.1a2.6 2.6 0 0 0 3.7 0l2.2-2.2a2.6 2.6 0 0 0-3.7-3.7l-.8.8" '
        f"{_STROKE} {_ROUND}/>"
        '<path d="M9.1 6.9a2.6 2.6 0 0 0-3.7 0L3.2 9.1a2.6 2.6 0 0 0 3.7 3.7l.8-.8" '
        f"{_STROKE} {_ROUND}/>"
    ),
    "markdown": (
        '<rect x="1" y="3.25" width="14" height="9.5" rx="1.5" fill="none" stroke="currentColor" '
        'stroke-width="1.25"/><path d="M3.5 10.5v-5l2 2.25 2-2.25v5" fill="none" '
        'stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/>'
        '<path d="M11 5.5v5m-1.75-1.75L11 10.5l1.75-1.75" fill="none" stroke="currentColor" '
        f'stroke-width="1.25" {_ROUND}/>'
    ),
    "alert": (
        '<path d="M7.13 2.5a1 1 0 0 1 1.74 0l5.5 9.75a1 1 0 0 1-.87 1.5H2.5a1 1 0 0 1-.87-1.5Z" '
        f'{_STROKE} stroke-linejoin="round"/>'
        f'<path d="M8 6.25v3" {_STROKE} {_ROUND}/>'
        '<circle cx="8" cy="11.4" r=".9" fill="currentColor"/>'
    ),
    "chevron-left": f'<path d="M9.75 3.5 5.25 8l4.5 4.5" {_STROKE} {_ROUND}/>',
    "chevron-right": f'<path d="m6.25 3.5 4.5 4.5-4.5 4.5" {_STROKE} {_ROUND}/>',
    "tasklist": (
        '<path d="m1.75 3.75 1.25 1.25 2.25-2.5M1.75 10.25l1.25 1.25 2.25-2.5" '
        f"{_STROKE} {_ROUND}/>"
        f'<path d="M8 4.25h6.25M8 10.75h6.25" {_STROKE} {_ROUND}/>'
    ),
}

LOGO = (
    '<svg class="icon logo" viewBox="0 0 32 32" width="{size}" height="{size}" aria-hidden="true">'
    '<rect class="logo-tile" width="32" height="32" rx="8"/>'
    '<circle cx="16" cy="16" r="8.5" fill="none" stroke="#fff" stroke-width="2.75"/>'
    '<circle cx="16" cy="16" r="2.75" fill="#fff"/></svg>'
)


def icon(name: str, size: int = 16, css_class: str = "") -> Markup:
    if name == "logo":
        return Markup(LOGO.format(size=size))
    classes = f"icon icon-{name} {css_class}".strip()
    return Markup(
        f'<svg class="{classes}" viewBox="0 0 16 16" width="{size}" height="{size}" '
        f'aria-hidden="true" focusable="false">{SHAPES[name]}</svg>'
    )


def progress(done: int, total: int) -> Markup:
    """A ring that fills up as the tasks get done, like the one in GitHub's issue list."""
    circumference = 2 * math.pi * 6
    filled = circumference * (done / total if total else 0)
    state = "complete" if total and done == total else "partial"
    return Markup(
        f'<svg class="icon progress-ring progress-{state}" viewBox="0 0 16 16" '
        'width="16" height="16" '
        'aria-hidden="true"><circle class="progress-track" cx="8" cy="8" r="6" fill="none" '
        'stroke-width="2"/><circle class="progress-value" cx="8" cy="8" r="6" fill="none" '
        f'stroke-width="2" stroke-dasharray="{filled:.2f} {circumference:.2f}" '
        'transform="rotate(-90 8 8)"/></svg>'
    )
