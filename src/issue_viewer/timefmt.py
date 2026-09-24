"""Dates the way GitHub writes them: "3 hours ago", "yesterday", "on Sep 3"."""

from __future__ import annotations

from datetime import datetime, tzinfo


def relative(moment: datetime, now: datetime) -> str:
    seconds = (now - moment).total_seconds()
    if seconds < 45:
        return "just now"
    minutes = seconds / 60
    if minutes < 1.5:
        return "1 minute ago"
    if minutes < 45:
        return f"{round(minutes)} minutes ago"
    hours = minutes / 60
    if hours < 1.5:
        return "1 hour ago"
    if hours < 22:
        return f"{round(hours)} hours ago"
    days = hours / 24
    if days < 1.5:
        return "yesterday"
    if days < 26:
        return f"{round(days)} days ago"
    year = f", {moment.year}" if moment.year != now.year else ""
    return f"on {moment:%b} {moment.day}{year}"


def absolute(moment: datetime, zone: tzinfo) -> str:
    local = moment.astimezone(zone)
    return f"{local:%b} {local.day}, {local.year}, {local:%H:%M} {local.tzname() or ''}".strip()


def local(naive: datetime, zone: tzinfo) -> datetime:
    """A timestamp written without a zone, read as local time in `zone`."""
    return naive.replace(tzinfo=zone)
