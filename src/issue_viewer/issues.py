"""Issues as the viewer shows them, built from one file each."""

from __future__ import annotations

import re
import unicodedata
from dataclasses import dataclass
from datetime import UTC, datetime
from functools import cached_property
from pathlib import Path

from .files import FileText
from .issuefile import Comment, IssueFile, lines, parse, task_counts

FILENAME = re.compile(r"^(\d+)(?:-[^/\\]*)?\.md$")
COMPLETED = frozenset({"done", "closed", "complete", "completed", "resolved", "fixed", "merged"})
NOT_PLANNED = frozenset(
    {
        "not-planned",
        "not planned",
        "not_planned",
        "wontfix",
        "won't fix",
        "wont-fix",
        "cancelled",
        "canceled",
        "duplicate",
        "invalid",
        "rejected",
        "obsolete",
        "abandoned",
    }
)
# What the viewer writes. `done` is what the task files already use for a finished issue.
STATE_OPEN = "open"
STATE_DONE = "done"
STATE_NOT_PLANNED = "not-planned"

RELATION = re.compile(r"\*\*(Depends on|Blocked by|Blocks|Blocking):\*\*", re.IGNORECASE)
REFERENCE = re.compile(r"(?<![\w&/#])#(\d+)\b")


def classify(state: str) -> tuple[bool, str | None]:
    """(is_open, reason) for a state value; anything not known to be closed counts as open."""
    value = state.strip().lower()
    if value in COMPLETED:
        return False, "completed"
    if value in NOT_PLANNED:
        return False, "not_planned"
    return True, None


def normalise_labels(value: object) -> tuple[str, ...]:
    if value is None:
        items: list[object] = []
    elif isinstance(value, str):
        items = value.split(",")
    elif isinstance(value, list | tuple | set):
        items = list(value)
    else:
        items = [value]
    labels = (str(item).strip() for item in items if item is not None)
    return tuple(dict.fromkeys(label for label in labels if label))


def relationships(description: str) -> tuple[tuple[int, ...], tuple[int, ...]]:
    """(depends on, blocks) from the first `**Depends on:** … · **Blocks:** …` line."""
    for line in lines(description):
        if line.fenced:
            continue
        labels = list(RELATION.finditer(line.text))
        if not labels:
            continue
        depends: list[int] = []
        blocks: list[int] = []
        for index, label in enumerate(labels):
            end = labels[index + 1].start() if index + 1 < len(labels) else len(line.text)
            numbers = [int(number) for number in REFERENCE.findall(line.text[label.end() : end])]
            kind = label.group(1).lower()
            (depends if kind in ("depends on", "blocked by") else blocks).extend(numbers)
        return tuple(dict.fromkeys(depends)), tuple(dict.fromkeys(blocks))
    return (), ()


def slugify(title: str, limit: int = 60) -> str:
    text = title.replace("ł", "l").replace("Ł", "L").replace("ß", "ss")
    text = unicodedata.normalize("NFKD", text).encode("ascii", "ignore").decode("ascii")
    text = re.sub(r"[^a-z0-9]+", "-", text.lower()).strip("-")
    if len(text) > limit:
        text = text[:limit].rsplit("-", 1)[0] or text[:limit]
    return text or "issue"


def _title_from_filename(filename: str) -> str:
    stem = re.sub(r"^\d+-?", "", filename.removesuffix(".md")).replace("-", " ").strip()
    return stem[:1].upper() + stem[1:] if stem else filename


@dataclass(frozen=True)
class Issue:
    number: int
    filename: str
    path: Path
    title: str
    labels: tuple[str, ...]
    state: str  # the raw value from the front matter, lower case
    is_open: bool
    reason: str | None  # "completed" or "not_planned" when closed
    file: IssueFile
    newline: str
    version: str
    readable: bool
    updated: datetime
    depends_on: tuple[int, ...]
    blocks: tuple[int, ...]
    tasks_done: int
    tasks_total: int
    problems: tuple[str, ...]

    @property
    def description(self) -> str:
        return self.file.description_text

    @property
    def comments(self) -> tuple[Comment, ...]:
        return self.file.comments

    def comment_text(self, comment: Comment) -> str:
        return self.file.comment_text(comment)

    @property
    def editable(self) -> bool:
        return self.readable and self.file.front.error is None

    @property
    def state_is_custom(self) -> bool:
        """True when the raw state says more than open or closed, e.g. `in-progress`."""
        return self.state not in (STATE_OPEN, STATE_DONE)

    @cached_property
    def search_text(self) -> str:
        return f"#{self.number} {self.title}\n{self.file.text}".casefold()


def build(path: Path, number: int, content: FileText, modified: float) -> Issue:
    parsed = parse(content.text)
    data = parsed.front.data
    problems = []
    if parsed.front.error:
        problems.append(f"The front matter could not be read: {parsed.front.error}.")
    if not content.readable:
        problems.append("The file is not valid UTF-8, so it is shown with replacement characters.")
    raw_title = data.get("title")
    title = str(raw_title).strip() if raw_title is not None else ""
    title = title or parsed.heading_title or _title_from_filename(path.name)
    state = str(data.get("state") or STATE_OPEN).strip().lower()
    is_open, reason = classify(state)
    depends, blocks = relationships(parsed.description_text)
    done, total = task_counts(parsed.description_text)
    return Issue(
        number=number,
        filename=path.name,
        path=path,
        title=title,
        labels=normalise_labels(data.get("labels")),
        state=state,
        is_open=is_open,
        reason=reason,
        file=parsed,
        newline=content.newline,
        version=content.version,
        readable=content.readable,
        updated=datetime.fromtimestamp(modified, UTC),
        depends_on=depends,
        blocks=blocks,
        tasks_done=done,
        tasks_total=total,
        problems=tuple(problems),
    )
