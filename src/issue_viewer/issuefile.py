"""The parts of an issue file, and the edits the viewer makes to them.

An issue file is front matter, an optional `# #N Title` heading, the description, and an optional
`## Comments` section at the end with one `### YYYY-MM-DD HH:MM` heading per comment. Parsing
records where each part starts and ends; an edit replaces one of those spans and nothing else, so
whatever the viewer was not asked to change stays byte for byte.
"""

from __future__ import annotations

import re
from dataclasses import dataclass
from datetime import datetime

from . import frontmatter

HEADING_1 = re.compile(r"^# +(.+?)(?:[ \t]+#+)?[ \t]*$")
NUMBERED = re.compile(r"^#(\d+)\b[ \t]*(.*)$")
COMMENTS_HEADING = re.compile(r"^## +Comments[ \t]*$", re.IGNORECASE)
COMMENT_HEADING = re.compile(
    r"^### +(\d{4}-\d{2}-\d{2}[ T]\d{2}:\d{2}(?::\d{2})?)(?:[ \t]+(.*?))?[ \t]*$"
)
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})(.*)$")
TASK = re.compile(r"^((?:[ \t]*>)*[ \t]*(?:[-*+]|\d{1,9}[.)])[ \t]+)\[([ xX])\](?=[ \t]|$)")
STAMP_FORMAT = "%Y-%m-%d %H:%M"


class EditError(ValueError):
    """The edit cannot be made to the file as it is now."""


@dataclass(frozen=True)
class Span:
    start: int
    end: int

    def of(self, text: str) -> str:
        return text[self.start : self.end]


@dataclass(frozen=True)
class Line:
    start: int
    end: int  # position of the newline, or the end of the text
    text: str
    fenced: bool  # inside a fenced code block, the fence lines included


def lines(text: str, start: int = 0) -> list[Line]:
    found: list[Line] = []
    fence: str | None = None
    position = start
    while position < len(text):
        newline = text.find("\n", position)
        stop = len(text) if newline == -1 else newline
        line = text[position:stop]
        match = FENCE.match(line)
        if fence is None:
            opens = match is not None and not (match.group(1)[0] == "`" and "`" in match.group(2))
            found.append(Line(position, stop, line, opens))
            if opens:
                fence = match.group(1)
        else:
            found.append(Line(position, stop, line, True))
            if (
                match
                and match.group(1)[0] == fence[0]
                and len(match.group(1)) >= len(fence)
                and not match.group(2).strip()
            ):
                fence = None
        position = stop + 1
    return found


def _line_span(text: str, line: Line) -> Span:
    return Span(line.start, min(line.end + 1, len(text)))


def _content(text: str, span: Span) -> Span:
    """The span without its leading blank lines and trailing whitespace."""
    start, end = span.start, span.end
    while start < end:
        newline = text.find("\n", start, end)
        line_end = end if newline == -1 else newline
        if text[start:line_end].strip():
            break
        start = end if newline == -1 else newline + 1
    while end > start and text[end - 1] in " \t\n":
        end -= 1
    return Span(start, end)


@dataclass(frozen=True)
class Comment:
    index: int
    stamp: str  # the timestamp as written in the heading
    note: str  # whatever follows the timestamp on the heading line
    heading: Span  # the heading line, newline included
    body: Span  # everything after the heading, up to the next comment
    content: Span  # the body without surrounding blank lines

    @property
    def when(self) -> datetime | None:
        for layout in (
            "%Y-%m-%d %H:%M",
            "%Y-%m-%d %H:%M:%S",
            "%Y-%m-%dT%H:%M",
            "%Y-%m-%dT%H:%M:%S",
        ):
            try:
                return datetime.strptime(self.stamp, layout)
            except ValueError:
                continue
        return None


@dataclass(frozen=True)
class IssueFile:
    text: str
    front: frontmatter.FrontMatter
    heading: Span | None  # the `# #N Title` line, newline included
    heading_number: int | None  # N when the heading starts with #N
    heading_title: str | None
    description: Span  # between the heading (or the front matter) and the comments
    description_content: Span
    comments_heading: Span | None
    comments: tuple[Comment, ...]

    @property
    def description_text(self) -> str:
        return self.description_content.of(self.text)

    def comment_text(self, comment: Comment) -> str:
        return comment.content.of(self.text)


def _same(left: str, right: str) -> bool:
    return " ".join(left.split()).casefold() == " ".join(right.split()).casefold()


def parse(text: str) -> IssueFile:
    front = frontmatter.split(text)
    title = front.data.get("title")
    body_lines = lines(text, front.body_start)

    heading = heading_number = heading_title = None
    for line in body_lines:
        if not line.text.strip():
            continue
        match = None if line.fenced else HEADING_1.match(line.text)
        if match:
            inner = match.group(1).strip()
            numbered = NUMBERED.match(inner)
            # Without a title in the front matter, the first heading is the only title there is.
            if numbered or title is None or _same(inner, str(title)):
                heading = _line_span(text, line)
                heading_number = int(numbered.group(1)) if numbered else None
                heading_title = numbered.group(2).strip() if numbered else inner
        break

    description_start = heading.end if heading else front.body_start
    rest = [line for line in body_lines if line.start >= description_start]
    comments_heading = None
    for index, line in enumerate(rest):
        if line.fenced or not COMMENTS_HEADING.match(line.text):
            continue
        following = next((later for later in rest[index + 1 :] if later.text.strip()), None)
        if following is None or (not following.fenced and COMMENT_HEADING.match(following.text)):
            comments_heading = _line_span(text, line)
            break

    description = Span(description_start, comments_heading.start if comments_heading else len(text))
    comments: tuple[Comment, ...] = ()
    if comments_heading:
        section = [line for line in rest if line.start >= comments_heading.end]
        comments = _comments(text, section)
    return IssueFile(
        text,
        front,
        heading,
        heading_number,
        heading_title,
        description,
        _content(text, description),
        comments_heading,
        comments,
    )


def _comments(text: str, section: list[Line]) -> tuple[Comment, ...]:
    headings = [line for line in section if not line.fenced and COMMENT_HEADING.match(line.text)]
    found = []
    for index, line in enumerate(headings):
        match = COMMENT_HEADING.match(line.text)
        heading = _line_span(text, line)
        end = headings[index + 1].start if index + 1 < len(headings) else len(text)
        body = Span(heading.end, end)
        found.append(
            Comment(
                index,
                match.group(1),
                (match.group(2) or "").strip(),
                heading,
                body,
                _content(text, body),
            )
        )
    return tuple(found)


def task_counts(text: str) -> tuple[int, int]:
    """(done, total) task list items outside code blocks."""
    done = total = 0
    for line in lines(text):
        match = None if line.fenced else TASK.match(line.text)
        if match:
            total += 1
            done += match.group(2) != " "
    return done, total


def normalise_block(text: str) -> str:
    text = text.replace("\r\n", "\n").replace("\r", "\n")
    return re.sub(r"\A(?:[ \t]*\n)+", "", text).rstrip()


def escape_comment_headings(text: str) -> str:
    """Keep a line that looks like a comment heading from starting a new comment."""
    return "\n".join(
        "\\" + line.text if not line.fenced and COMMENT_HEADING.match(line.text) else line.text
        for line in lines(text)
    )


def replace(text: str, span: Span, new: str) -> str:
    return text[: span.start] + new + text[span.end :]


# Edits. Each takes the parsed file and returns the new text of the whole file.


def set_front(issue_file: IssueFile, key: str, rendered: str) -> str:
    text, front = issue_file.text, issue_file.front
    if front.error:
        raise EditError(f"fix the front matter by hand first: {front.error}")
    if not front.present:
        separator = "" if not text or text.startswith("\n") else "\n"
        return f"---\n{key}: {rendered}\n---\n{separator}{text}"
    raw = frontmatter.set_key(front.raw(text), key, rendered)
    return replace(text, Span(front.raw_start, front.raw_end), raw)


def set_title(issue_file: IssueFile, title: str) -> str:
    text = issue_file.text
    if issue_file.heading is not None:
        number = issue_file.heading_number
        line = f"# #{number} {title}" if number is not None else f"# {title}"
        ending = (
            "\n" if text[issue_file.heading.start : issue_file.heading.end].endswith("\n") else ""
        )
        text = replace(text, issue_file.heading, line + ending)
    reparsed = parse(text)
    style = frontmatter.style_of(reparsed.front.raw(text), "title")
    return set_front(reparsed, "title", frontmatter.render_scalar(title, style))


def set_description(issue_file: IssueFile, description: str) -> str:
    body = normalise_block(description)
    before = issue_file.description.start > 0
    after = issue_file.comments_heading is not None
    if body:
        new = ("\n" if before else "") + body + "\n" + ("\n" if after else "")
    else:
        new = "\n" if before and after else ""
    prefix = issue_file.text[: issue_file.description.start]
    if prefix and not prefix.endswith("\n"):
        new = "\n" + new
    result = replace(issue_file.text, issue_file.description, new)
    if parse(result).description_text != body:
        raise EditError(
            "the description would run into the comments section: a '## Comments' heading "
            "followed by a dated '###' heading is how the comments start, so rename one of them"
        )
    return result


def add_comment(issue_file: IssueFile, stamp: str, text: str) -> str:
    body = escape_comment_headings(normalise_block(text))
    if not body:
        raise EditError("a comment cannot be empty")
    source = issue_file.text.rstrip("\n")
    if issue_file.comments_heading is None:
        source = (source + "\n\n" if source else "") + "## Comments\n"
    else:
        source += "\n"
    return f"{source}\n### {stamp}\n\n{body}\n"


def _comment(issue_file: IssueFile, index: int) -> Comment:
    if not 0 <= index < len(issue_file.comments):
        raise EditError("that comment is no longer in the file")
    return issue_file.comments[index]


def edit_comment(issue_file: IssueFile, index: int, text: str) -> str:
    comment = _comment(issue_file, index)
    body = escape_comment_headings(normalise_block(text))
    if not body:
        raise EditError("a comment cannot be empty; delete it instead")
    last = index == len(issue_file.comments) - 1
    return replace(issue_file.text, comment.body, f"\n{body}\n" + ("" if last else "\n"))


def delete_comment(issue_file: IssueFile, index: int) -> str:
    comment = _comment(issue_file, index)
    if len(issue_file.comments) == 1:
        # The last comment takes the section heading and the blank lines before it along.
        head = issue_file.text[: issue_file.comments_heading.start].rstrip("\n")
        return head + "\n" if head else ""
    text = replace(issue_file.text, Span(comment.heading.start, comment.body.end), "")
    if index == len(issue_file.comments) - 1:
        text = text.rstrip("\n") + "\n"
    return text


def set_task(issue_file: IssueFile, part: str, line_number: int, checked: bool) -> str:
    """Tick or clear the checkbox on a line of the description or of one comment."""
    if part == "description":
        content = issue_file.description_content
    elif part.isdigit():
        content = _comment(issue_file, int(part)).content
    else:
        raise EditError("unknown part of the issue")
    part_lines = content.of(issue_file.text).split("\n")
    match = TASK.match(part_lines[line_number]) if 0 <= line_number < len(part_lines) else None
    if not match:
        raise EditError("that checkbox is no longer where it was")
    if (match.group(2) != " ") == checked:
        return issue_file.text
    offset = content.start + sum(len(line) + 1 for line in part_lines[:line_number])
    position = offset + match.end(1) + 1
    text = issue_file.text
    return text[:position] + ("x" if checked else " ") + text[position + 1 :]


def new_issue_text(number: int, title: str, labels: list[str], description: str) -> str:
    text = (
        "---\n"
        f"title: {frontmatter.render_scalar(title, 'double')}\n"
        f"labels: {frontmatter.render_list(labels)}\n"
        "state: open\n"
        "---\n"
        f"\n# #{number} {title}\n"
    )
    body = normalise_block(description)
    return text + (f"\n{body}\n" if body else "")
