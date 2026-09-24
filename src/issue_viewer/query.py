"""The search box: the parts of GitHub's issue query syntax that make sense for files.

`is:open`, `is:closed`, `state:<raw state>`, `reason:completed|not-planned`, `label:a` (repeated
for AND, `label:a,b` for OR), `-label:a`, `no:label`, `sort:<key>`, and free text that has to
appear in the title or the file. Quotes keep a value with spaces together: `label:"good first"`.
"""

from __future__ import annotations

import shlex
from dataclasses import dataclass, field

from .issues import Issue

DEFAULT = "is:issue is:open"
SORTS = (
    ("created-desc", "Newest"),
    ("created-asc", "Oldest"),
    ("comments-desc", "Most commented"),
    ("comments-asc", "Least commented"),
    ("updated-desc", "Recently updated"),
    ("updated-asc", "Least recently updated"),
)
SORT_KEYS = dict(SORTS)
DEFAULT_SORT = "created-desc"
REASONS = {"completed": "completed", "not-planned": "not_planned", "not_planned": "not_planned"}


@dataclass
class Query:
    state: str | None = None  # "open", "closed" or None for both
    raw_states: list[str] = field(default_factory=list)
    reason: str | None = None
    labels: list[list[str]] = field(default_factory=list)  # every group must match one of its names
    excluded_labels: list[str] = field(default_factory=list)
    no_label: bool = False
    words: list[str] = field(default_factory=list)
    sort: str = DEFAULT_SORT


def tokenize(text: str) -> list[str]:
    try:
        return shlex.split(text)
    except ValueError:  # an unbalanced quote
        return text.split()


def parse(text: str) -> Query:
    query = Query()
    for token in tokenize(text):
        negated = token.startswith("-") and ":" in token
        key, separator, value = (token[1:] if negated else token).partition(":")
        key, lowered = key.lower(), value.lower()
        if separator and value:
            if key == "is" and lowered in ("open", "closed"):
                flipped = {"open": "closed", "closed": "open"}[lowered]
                query.state = flipped if negated else lowered
                continue
            if key == "is" and lowered in ("issue", "issues"):
                continue
            if key == "state":
                if lowered in ("open", "closed"):
                    query.state = lowered
                else:
                    query.raw_states.append(lowered)
                continue
            if key == "reason" and lowered in REASONS:
                query.reason = REASONS[lowered]
                continue
            if key == "label":
                names = [name.strip() for name in value.split(",") if name.strip()]
                if negated:
                    query.excluded_labels.extend(names)
                else:
                    query.labels.append(names)
                continue
            if key == "no" and lowered == "label":
                query.no_label = True
                continue
            if key == "sort" and lowered in SORT_KEYS:
                query.sort = lowered
                continue
        query.words.append(token.casefold())
    return query


def matches(query: Query, issue: Issue, *, ignore_state: bool = False) -> bool:
    if not ignore_state and not state_matches(query, issue):
        return False
    if query.raw_states and issue.state not in query.raw_states:
        return False
    if query.reason and issue.reason != query.reason:
        return False
    names = {label.casefold() for label in issue.labels}
    if any(not any(name.casefold() in names for name in group) for group in query.labels):
        return False
    if any(name.casefold() in names for name in query.excluded_labels):
        return False
    if query.no_label and issue.labels:
        return False
    return all(word in issue.search_text for word in query.words)


def state_matches(query: Query, issue: Issue) -> bool:
    return query.state is None or (query.state == "open") == issue.is_open


def ordered(issues: list[Issue], key: str) -> list[Issue]:
    if key == "created-asc":
        return sorted(issues, key=lambda issue: issue.number)
    if key == "updated-desc":
        return sorted(issues, key=lambda issue: (issue.updated, issue.number), reverse=True)
    if key == "updated-asc":
        return sorted(issues, key=lambda issue: (issue.updated, issue.number))
    if key == "comments-desc":
        return sorted(issues, key=lambda issue: (-len(issue.comments), -issue.number))
    if key == "comments-asc":
        return sorted(issues, key=lambda issue: (len(issue.comments), -issue.number))
    return sorted(issues, key=lambda issue: issue.number, reverse=True)


# Editing a query string for the links on the list page.


def _quote(token: str) -> str:
    if not any(char.isspace() for char in token) and '"' not in token:
        return token
    key, separator, value = token.partition(":")
    if separator and key and not any(char.isspace() for char in key):
        return f'{key}:"{value}"'
    return f'"{token}"'


def _join(tokens: list[str]) -> str:
    return " ".join(_quote(token) for token in tokens)


def _is_state(token: str) -> bool:
    return token.lower().lstrip("-") in ("is:open", "is:closed", "state:open", "state:closed")


def with_state(text: str, state: str) -> str:
    tokens = [token for token in tokenize(text) if not _is_state(token)]
    position = next((i + 1 for i, token in enumerate(tokens) if token.lower() == "is:issue"), 0)
    tokens.insert(position, f"is:{state}")
    return _join(tokens)


def label_token(label: str) -> str:
    return _quote(f"label:{label}")


def with_label(text: str, label: str) -> str:
    """Add `label:<label>`, or take it out when it is already there."""
    tokens = tokenize(text)
    wanted = f"label:{label}".casefold()
    kept = [token for token in tokens if token.casefold() != wanted]
    if len(kept) == len(tokens):
        kept = [token for token in kept if token.lower() != "no:label"]
        kept.append(f"label:{label}")
    return _join(kept)


def with_no_label(text: str) -> str:
    tokens = [token for token in tokenize(text) if not token.lower().startswith("label:")]
    if "no:label" not in (token.lower() for token in tokens):
        tokens.append("no:label")
    return _join(tokens)


def with_sort(text: str, key: str) -> str:
    tokens = [token for token in tokenize(text) if not token.lower().startswith("sort:")]
    if key != DEFAULT_SORT:
        tokens.append(f"sort:{key}")
    return _join(tokens)


def has_label(query: Query, label: str) -> bool:
    return any(
        len(group) == 1 and group[0].casefold() == label.casefold() for group in query.labels
    )
