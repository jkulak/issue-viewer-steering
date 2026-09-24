"""Markdown to HTML the way GitHub renders an issue, safe to put straight into a page.

Raw HTML is escaped, apart from a short list of tags without attributes (details, summary, kbd,
sub, sup, …) that can neither run a script nor load anything; HTML comments disappear, as they do
on GitHub. Task list items become checkboxes that know the source line they came from, so ticking
one in the browser changes exactly that line. `#N` and links to `NNN-slug.md` open that issue.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

from markdown_it import MarkdownIt
from markdown_it.common.utils import escapeHtml
from markdown_it.token import Token

ALLOWED_TAG = re.compile(
    r"^</?(?:b|i|u|s|em|strong|code|kbd|sub|sup|br|details|summary|ins|del|mark|small)\s*/?>$",
    re.IGNORECASE,
)
TAG = re.compile(r"(<[^<>]*>)")
HTML_COMMENT = re.compile(r"<!--.*?-->", re.DOTALL)
TASK_PREFIX = re.compile(r"^\[([ xX])\](?=\s|$)")
ISSUE_REFERENCE = re.compile(r"(?<![\w&/#])#(\d+)\b")
ISSUE_FILE = re.compile(r"^(?:\./)?(\d+)(?:-[^/?#]*)?\.md(?:#.*)?$")


@dataclass(frozen=True)
class Context:
    project: str | None = None
    issues: dict[int, tuple[str, bool]] = field(default_factory=dict)  # number: (title, open)
    part: str | None = None  # "description" or a comment index, for the checkboxes
    interactive: bool = False  # the checkboxes can be ticked


def _render_html(renderer, tokens, index, options, env) -> str:
    content = HTML_COMMENT.sub("", tokens[index].content)
    return "".join(
        part if ALLOWED_TAG.match(part) else escapeHtml(part) for part in TAG.split(content) if part
    )


def _render_checkbox(renderer, tokens, index, options, env) -> str:
    meta = tokens[index].meta
    context: Context = env["context"]
    attributes = ['type="checkbox"', 'class="task-list-item-checkbox"']
    if meta["checked"]:
        attributes.append("checked")
    if context.interactive and context.part is not None and meta["line"] is not None:
        attributes.append(f'data-part="{escapeHtml(context.part)}" data-line="{int(meta["line"])}"')
    else:
        attributes.append("disabled")
    return f"<input {' '.join(attributes)}> "


def _task_lists(state) -> None:
    tokens = state.tokens
    for index in range(2, len(tokens)):
        inline = tokens[index]
        if (
            inline.type != "inline"
            or tokens[index - 1].type != "paragraph_open"
            or tokens[index - 2].type != "list_item_open"
        ):
            continue
        match = TASK_PREFIX.match(inline.content)
        children = inline.children or []
        if not match or not children or children[0].type != "text":
            continue
        if not children[0].content.startswith(match.group(0)):
            continue
        children[0].content = children[0].content[len(match.group(0)) :].lstrip(" \t")
        box = Token("task_checkbox", "input", 0)
        box.meta = {"checked": match.group(1) != " ", "line": inline.map[0] if inline.map else None}
        inline.children = [box, *children]
        item = tokens[index - 2]
        item.attrJoin("class", "task-list-item")
        for back in range(index - 3, -1, -1):
            candidate = tokens[back]
            if (
                candidate.type in ("bullet_list_open", "ordered_list_open")
                and candidate.level == item.level - 1
            ):
                if "contains-task-list" not in str(candidate.attrGet("class") or ""):
                    candidate.attrJoin("class", "contains-task-list")
                break


def _text(content: str, like: Token) -> Token:
    token = Token("text", "", 0)
    token.content = content
    token.level = like.level
    return token


def _references(token: Token, context: Context) -> list[Token]:
    parts: list[Token] = []
    position = 0
    for match in ISSUE_REFERENCE.finditer(token.content):
        number = int(match.group(1))
        if number not in context.issues:
            continue
        if match.start() > position:
            parts.append(_text(token.content[position : match.start()], token))
        title, is_open = context.issues[number]
        opening = Token("link_open", "a", 1)
        opening.attrs = {
            "href": f"/{context.project}/issues/{number}",
            "class": "issue-link",
            "title": f"{title} ({'open' if is_open else 'closed'})",
        }
        parts += [opening, _text(match.group(0), token), Token("link_close", "a", -1)]
        position = match.end()
    if not parts:
        return [token]
    if position < len(token.content):
        parts.append(_text(token.content[position:], token))
    return parts


def _issue_links(state) -> None:
    context: Context = state.env["context"]
    for token in state.tokens:
        if token.type != "inline" or not token.children:
            continue
        children: list[Token] = []
        depth = 0
        for child in token.children:
            if child.type == "link_open":
                depth += 1
                href = str(child.attrGet("href") or "")
                match = ISSUE_FILE.match(href)
                if match and context.project:
                    child.attrSet("href", f"/{context.project}/issues/{int(match.group(1))}")
                elif href.startswith(("http://", "https://", "//")):
                    child.attrSet("rel", "nofollow noopener noreferrer")
            elif child.type == "link_close":
                depth -= 1
            if child.type == "text" and depth == 0 and context.project and "#" in child.content:
                children.extend(_references(child, context))
            else:
                children.append(child)
        token.children = children


class Renderer:
    def __init__(self) -> None:
        md = MarkdownIt("commonmark", {"html": True, "linkify": False, "typographer": False})
        md.enable(["table", "strikethrough"])
        md.core.ruler.push("task_lists", _task_lists)
        md.core.ruler.push("issue_links", _issue_links)
        md.add_render_rule("html_block", _render_html)
        md.add_render_rule("html_inline", _render_html)
        md.add_render_rule("task_checkbox", _render_checkbox)
        self._md = md

    def render(self, text: str, context: Context | None = None) -> str:
        return self._md.render(text, {"context": context or Context()})
