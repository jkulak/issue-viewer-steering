"""The web app: pages, forms, and the checks that keep other sites from using them.

There is no login, so the protection is about where a request comes from: the Host header must be
one the viewer answers to (no DNS rebinding), a write must come from the viewer's own pages
(Origin and Sec-Fetch-Site) and carry the token those pages embed (no cross-site forms).
"""

from __future__ import annotations

import hashlib
import logging
import math
import secrets
from collections.abc import Callable
from datetime import UTC, datetime
from importlib.metadata import PackageNotFoundError
from importlib.metadata import version as package_version
from pathlib import Path
from typing import Annotated
from urllib.parse import urlencode, urlsplit

from fastapi import APIRouter, Depends, FastAPI, Form, Request
from fastapi.exceptions import RequestValidationError
from fastapi.responses import HTMLResponse, JSONResponse, RedirectResponse, Response
from fastapi.staticfiles import StaticFiles
from fastapi.templating import Jinja2Templates
from markupsafe import Markup
from starlette.exceptions import HTTPException
from starlette.middleware.trustedhost import TrustedHostMiddleware

from . import browse, files, frontmatter, labels, timefmt
from . import query as search
from .icons import icon, progress
from .issuefile import (
    STAMP_FORMAT,
    EditError,
    add_comment,
    delete_comment,
    edit_comment,
    parse,
    set_description,
    set_front,
    set_task,
    set_title,
)
from .issues import STATE_DONE, STATE_NOT_PLANNED, STATE_OPEN, Issue, normalise_labels
from .project import ConflictError, NotFoundError, Project, ReadOnlyError, Registry, Snapshot
from .render import Context, Renderer
from .settings import OpenRoot, Settings, load_settings

log = logging.getLogger(__name__)

PACKAGE = Path(__file__).parent
PAGE_SIZE = 25
TITLE_LIMIT = 256
LABEL_LIMIT = 50
TEMPLATE_FILE = "TEMPLATE.md"
CLOSE_ACTIONS = {"close": STATE_DONE, "close-not-planned": STATE_NOT_PLANNED, "reopen": STATE_OPEN}
CSP = "; ".join(
    (
        "default-src 'self'",
        "img-src 'self' data:",
        "style-src 'self' 'unsafe-inline'",  # label colours are style attributes
        "script-src 'self'",
        "object-src 'none'",
        "base-uri 'none'",
        "form-action 'self'",
        "frame-ancestors 'none'",
    )
)
DEFAULT_TEMPLATE = """\
**Depends on:** – · **Blocks:** –

## Context

## Scope

## Acceptance criteria

- [ ]

## Definition of Done

- [ ] Tests pass (unit, smoke, e2e as applicable)
- [ ] Migration has a working `down`
- [ ] Published schema / OpenAPI regenerated and committed
- [ ] CHANGELOG updated
- [ ] No new runtime dependency without an explicit approval
- [ ] Nothing secret or personal in the diff — checked, not assumed
"""

router = APIRouter()
FormText = Annotated[str, Form()]
FormList = Annotated[list[str] | None, Form()]


def _version() -> str:
    try:
        return package_version("issue-viewer")
    except PackageNotFoundError:
        return "dev"


def _asset_version() -> str:
    digest = hashlib.sha256()
    for path in sorted((PACKAGE / "static").iterdir()):
        digest.update(path.read_bytes())
    return digest.hexdigest()[:10]


def _sentence(error: Exception) -> str:
    text = str(error).strip()
    return text[:1].upper() + text[1:] + ("" if text.endswith(".") else ".")


def _base(
    request: Request, project: Project | None = None, snapshot: Snapshot | None = None
) -> dict:
    settings: Settings = request.app.state.settings
    now = datetime.now(UTC)
    return {
        "csrf_token": request.app.state.csrf_token,
        "refresh_seconds": settings.refresh_seconds,
        "app_version": request.app.version,
        "can_open": settings.open_root is not None,
        "project": project,
        "snapshot": snapshot,
        "open_total": snapshot.open_count if snapshot else 0,
        "label_total": len(snapshot.labels) if snapshot else 0,
        "ago": lambda moment: timefmt.relative(moment, now),
        "absolute": lambda moment: timefmt.absolute(moment, settings.timezone),
    }


def _page(request: Request, name: str, context: dict, status_code: int = 200) -> HTMLResponse:
    templates: Jinja2Templates = request.app.state.templates
    return templates.TemplateResponse(request, name, context, status_code=status_code)


def _project(request: Request, name: str) -> Project:
    try:
        return request.app.state.registry.get(name)
    except NotFoundError:
        message = f"There is no project called “{name}”."
        if request.app.state.settings.open_root:
            message += " A directory picked with Open is forgotten when the viewer restarts."
        raise HTTPException(404, message) from None


def _writable(request: Request, name: str) -> Project:
    project = _project(request, name)
    if not project.writable:
        raise HTTPException(403, f"{project.name} is read-only.")
    return project


def _issue(snapshot: Snapshot, project: Project, number: int) -> Issue:
    issue = snapshot.by_number.get(number)
    if issue is None:
        raise HTTPException(404, f"There is no issue #{number} in {project.name}.")
    return issue


def _refs(snapshot: Snapshot) -> dict[int, tuple[str, bool]]:
    return {issue.number: (issue.title, issue.is_open) for issue in snapshot.issues}


def _clean_title(title: str) -> str:
    clean = " ".join(title.split())
    if not clean:
        raise EditError("a title is required")
    if len(clean) > TITLE_LIMIT:
        raise EditError(f"a title can have at most {TITLE_LIMIT} characters")
    return clean


def _clean_labels(raw: list[str], existing: list[str]) -> tuple[str, ...]:
    known = {label.casefold(): label for label in existing}
    chosen = []
    for value in raw:
        name = " ".join(value.split())
        if not name:
            continue
        if "," in name:
            raise EditError(f"a label cannot contain a comma: “{name}”")
        if len(name) > LABEL_LIMIT:
            raise EditError(f"a label can have at most {LABEL_LIMIT} characters")
        chosen.append(known.get(name.casefold(), name))
    return normalise_labels(chosen)


def _page_items(page: int, pages: int) -> list[int | None]:
    """Page numbers around the current one, with None where a gap goes."""
    wanted = sorted(n for n in {1, pages, *range(page - 2, page + 3)} if 1 <= n <= pages)
    items: list[int | None] = []
    previous = 0
    for number in wanted:
        if number - previous > 1:
            items.append(None)
        items.append(number)
        previous = number
    return items


async def same_origin(request: Request) -> None:
    """Refuse a write that did not come from one of the viewer's own pages."""
    site = request.headers.get("sec-fetch-site")
    if site is not None and site not in ("same-origin", "none"):
        raise HTTPException(403, "A page on another site tried to change something here.")
    origin = request.headers.get("origin")
    if origin is not None and (
        origin == "null" or urlsplit(origin).netloc != request.headers.get("host")
    ):
        raise HTTPException(403, "A request from another origin cannot change anything here.")
    token = request.headers.get("x-csrf-token")
    if token is None:
        token = (await request.form()).get("_csrf")
    if not isinstance(token, str) or not secrets.compare_digest(
        token, request.app.state.csrf_token
    ):
        raise HTTPException(
            403, "This page is out of date (the viewer restarted after it was loaded). Reload it."
        )


writes = APIRouter(dependencies=[Depends(same_origin)])


# Pages


@router.get("/healthz", include_in_schema=False)
def healthz() -> dict:
    return {"status": "ok"}


@router.get("/favicon.ico", include_in_schema=False)
def favicon() -> RedirectResponse:
    return RedirectResponse("/static/favicon.svg", status_code=301)


@router.get("/", response_class=HTMLResponse)
def home(request: Request) -> HTMLResponse:
    rows = []
    for project in request.app.state.registry:
        snapshot = project.snapshot()
        latest = max((issue.updated for issue in snapshot.issues), default=None)
        rows.append({"project": project, "snapshot": snapshot, "latest": latest})
    return _page(request, "projects.html", {**_base(request), "rows": rows})


def _open_root(request: Request) -> OpenRoot:
    root = request.app.state.settings.open_root
    if root is None:
        raise HTTPException(404, "Open is off: set OPEN_ROOT in .env and run make up.")
    return root


def _browsed(root: OpenRoot, path: str) -> Path:
    try:
        return browse.resolve(root.path, path)
    except browse.BrowseError as error:
        raise HTTPException(404, _sentence(error)) from None


def _browse_url(relative: str) -> str:
    return f"/-/open?{urlencode({'path': relative})}" if relative else "/-/open"


@router.get("/-/open", response_class=HTMLResponse)
def open_page(request: Request, path: str = "") -> HTMLResponse:
    root = _open_root(request)
    directory = _browsed(root, path)
    relative = browse.relative_to(root.path, directory)
    parts = relative.split("/") if relative else []
    known = {project.path.resolve(): project for project in request.app.state.registry}
    return _page(
        request,
        "open.html",
        {
            **_base(request),
            "root": root,
            "relative": relative,
            "crumbs": [(part, "/".join(parts[: i + 1])) for i, part in enumerate(parts)],
            "parent": "/".join(parts[:-1]) if parts else None,
            "here": known.get(directory),
            "here_issues": browse.count_issues(directory),
            "rows": [
                {"folder": folder, "project": known.get(directory / folder.name)}
                for folder in browse.folders(root.path, directory)
            ],
            "browse_url": _browse_url,
        },
    )


@router.get("/{project_name}", include_in_schema=False)
def project_root(request: Request, project_name: str) -> RedirectResponse:
    project = _project(request, project_name)
    return RedirectResponse(f"/{project.name}/issues", status_code=302)


@router.get("/{project_name}/issues", response_class=HTMLResponse)
def issue_list(
    request: Request, project_name: str, q: str | None = None, page: int = 1
) -> HTMLResponse:
    project = _project(request, project_name)
    signature = project.signature()
    snapshot = project.snapshot()
    text = search.DEFAULT if q is None else q
    parsed = search.parse(text)
    candidates = [i for i in snapshot.issues if search.matches(parsed, i, ignore_state=True)]
    open_count = sum(issue.is_open for issue in candidates)
    shown = search.ordered([i for i in candidates if search.state_matches(parsed, i)], parsed.sort)
    pages = max(1, math.ceil(len(shown) / PAGE_SIZE))
    page = min(max(page, 1), pages)

    def query_url(new_query: str, new_page: int = 1) -> str:
        params: dict[str, str | int] = {"q": new_query}
        if new_page > 1:
            params["page"] = new_page
        return f"/{project.name}/issues?{urlencode(params)}"

    return _page(
        request,
        "issues.html",
        {
            **_base(request, project, snapshot),
            "section": "issues",
            "signature": signature,
            "q": text,
            "default_q": search.DEFAULT,
            "query": parsed,
            "rows": shown[(page - 1) * PAGE_SIZE : page * PAGE_SIZE],
            "open_count": open_count,
            "closed_count": len(candidates) - open_count,
            "page": page,
            "pages": pages,
            "page_items": _page_items(page, pages),
            "query_url": query_url,
            "with_state": lambda state: search.with_state(text, state),
            "with_label": lambda label: search.with_label(text, label),
            "with_no_label": lambda: search.with_no_label(text),
            "with_sort": lambda key: search.with_sort(text, key),
            "has_label": lambda label: search.has_label(parsed, label),
            "label_query": lambda label: f"{search.DEFAULT} {search.label_token(label)}",
            "sorts": search.SORTS,
            "sort_label": search.SORT_KEYS[parsed.sort],
        },
    )


@router.get("/{project_name}/issues/new", response_class=HTMLResponse)
def new_issue_form(request: Request, project_name: str) -> HTMLResponse:
    return _new_issue_page(request, _writable(request, project_name))


def _template(project: Project) -> str:
    path = project.path / TEMPLATE_FILE
    try:
        return files.read(path).text if path.is_file() else DEFAULT_TEMPLATE
    except OSError:
        return DEFAULT_TEMPLATE


def _new_issue_page(
    request: Request,
    project: Project,
    *,
    title: str = "",
    description: str | None = None,
    chosen: list[str] | tuple[str, ...] = (),
    error: str | None = None,
    status: int = 200,
) -> HTMLResponse:
    snapshot = project.snapshot()
    return _page(
        request,
        "new_issue.html",
        {
            **_base(request, project, snapshot),
            "section": "issues",
            "title": title,
            "description": _template(project) if description is None else description,
            "chosen": list(chosen),
            "all_labels": sorted(set(snapshot.labels) | set(chosen), key=str.casefold),
            "error": error,
        },
        status,
    )


@router.get("/{project_name}/issues/{number:int}", response_class=HTMLResponse)
def issue_page(request: Request, project_name: str, number: int) -> HTMLResponse:
    project = _project(request, project_name)
    snapshot = project.snapshot()
    return _issue_page(request, project, snapshot, _issue(snapshot, project, number))


def _issue_page(
    request: Request,
    project: Project,
    snapshot: Snapshot,
    issue: Issue,
    *,
    flash: dict | None = None,
    draft: dict | None = None,
    status: int = 200,
) -> HTMLResponse:
    renderer: Renderer = request.app.state.renderer
    zone = request.app.state.settings.timezone
    refs = _refs(snapshot)
    can_write = project.writable and issue.editable

    def render(text: str, part: str) -> Markup:
        return Markup(renderer.render(text, Context(project.name, refs, part, can_write)))

    comments = [
        {
            "comment": comment,
            "text": issue.comment_text(comment),
            "html": render(issue.comment_text(comment), str(comment.index)),
            "when": timefmt.local(comment.when, zone) if comment.when else None,
        }
        for comment in issue.comments
    ]
    return _page(
        request,
        "issue.html",
        {
            **_base(request, project, snapshot),
            "section": "issues",
            "issue": issue,
            "can_write": can_write,
            "description_html": render(issue.description, "description")
            if issue.description
            else None,
            "comments": comments,
            "blocked_by": [(n, snapshot.by_number.get(n)) for n in issue.depends_on],
            "blocking": [(n, snapshot.by_number.get(n)) for n in snapshot.blocking(issue.number)],
            "label_url": lambda name: (
                f"/{project.name}/issues?"
                + urlencode({"q": f"{search.DEFAULT} {search.label_token(name)}"})
            ),
            "flash": flash,
            "draft": draft or {},
        },
        status,
    )


def _issue_again(
    request: Request, project: Project, number: int, flash: dict, draft: dict | None, status: int
) -> HTMLResponse:
    snapshot = project.snapshot()
    issue = _issue(snapshot, project, number)
    return _issue_page(request, project, snapshot, issue, flash=flash, draft=draft, status=status)


@router.get("/{project_name}/labels", response_class=HTMLResponse)
def label_list(request: Request, project_name: str) -> HTMLResponse:
    project = _project(request, project_name)
    signature = project.signature()
    snapshot = project.snapshot()
    rows = []
    for name in snapshot.labels:
        tagged = [issue for issue in snapshot.issues if name in issue.labels]
        open_count = sum(issue.is_open for issue in tagged)
        rows.append({"name": name, "open": open_count, "closed": len(tagged) - open_count})
    return _page(
        request,
        "labels.html",
        {
            **_base(request, project, snapshot),
            "section": "labels",
            "signature": signature,
            "rows": rows,
            "label_url": lambda name, state: (
                f"/{project.name}/issues?"
                + urlencode({"q": f"is:issue is:{state} {search.label_token(name)}"})
            ),
        },
    )


@router.get("/{project_name}/-/version")
def version(request: Request, project_name: str, issue: int | None = None) -> dict:
    """What open pages poll to notice that a file changed on disk."""
    project = _project(request, project_name)
    if issue is None:
        return {"version": project.signature()}
    try:
        return {"version": project.version_of(issue)}
    except NotFoundError:
        return {"version": None}


# Writes


def _apply(
    request: Request,
    project: Project,
    number: int,
    edit: Callable[[Issue], str],
    *,
    expected_version: str | None = None,
    draft: dict | None = None,
) -> Issue | Response:
    try:
        return project.update(number, edit, expected_version)
    except ReadOnlyError:
        raise HTTPException(403, f"{project.name} is read-only.") from None
    except NotFoundError:
        raise HTTPException(404, f"There is no issue #{number} in {project.name}.") from None
    except ConflictError:
        flash = {
            "kind": "warn",
            "text": "This issue changed on disk after you opened it, so nothing was saved. "
            "Below is the latest version, with your text kept in the editor.",
        }
        return _issue_again(request, project, number, flash, draft, 409)
    except EditError as error:
        flash = {"kind": "error", "text": f"Not saved: {error}."}
        return _issue_again(request, project, number, flash, draft, 422)


def _back(project: Project, number: int, anchor: str = "") -> RedirectResponse:
    return RedirectResponse(f"/{project.name}/issues/{number}{anchor}", status_code=303)


@writes.post("/-/open")
def open_directory(request: Request, path: FormText = "") -> RedirectResponse:
    """Show a directory under the open root as a project, until the viewer restarts."""
    root = _open_root(request)
    directory = _browsed(root, path)
    relative = browse.relative_to(root.path, directory)
    project = request.app.state.registry.open(
        directory, lambda taken: browse.project_name(root.path, relative, taken), root.read_only
    )
    return RedirectResponse(f"/{project.name}/issues", status_code=303)


@writes.post("/{project_name}/issues")
def create_issue(
    request: Request,
    project_name: str,
    title: FormText = "",
    description: FormText = "",
    labels: FormList = None,
) -> Response:
    project = _writable(request, project_name)
    try:
        clean_title = _clean_title(title)
        chosen = _clean_labels(labels or [], project.snapshot().labels)
        issue = project.create(clean_title, list(chosen), description)
    except ReadOnlyError:
        raise HTTPException(403, f"{project.name} is read-only.") from None
    except EditError as error:
        return _new_issue_page(
            request,
            project,
            title=title,
            description=description,
            chosen=[label for label in labels or [] if label.strip()],
            error=_sentence(error),
            status=422,
        )
    return RedirectResponse(f"/{project.name}/issues/{issue.number}", status_code=303)


@writes.post("/{project_name}/issues/{number:int}/title")
def update_title(
    request: Request, project_name: str, number: int, title: FormText = ""
) -> Response:
    project = _project(request, project_name)
    result = _apply(
        request,
        project,
        number,
        lambda issue: set_title(issue.file, _clean_title(title)),
        draft={"target": "title", "text": title},
    )
    return result if isinstance(result, Response) else _back(project, number)


@writes.post("/{project_name}/issues/{number:int}/description")
def update_description(
    request: Request, project_name: str, number: int, text: FormText = "", version: FormText = ""
) -> Response:
    project = _project(request, project_name)
    result = _apply(
        request,
        project,
        number,
        lambda issue: set_description(issue.file, text),
        expected_version=version or None,
        draft={"target": "description", "text": text},
    )
    return result if isinstance(result, Response) else _back(project, number)


@writes.post("/{project_name}/issues/{number:int}/labels")
def update_labels(
    request: Request, project_name: str, number: int, labels: FormList = None
) -> Response:
    project = _project(request, project_name)
    existing = project.snapshot().labels

    def edit(issue: Issue) -> str:
        chosen = _clean_labels(labels or [], existing)
        return set_front(issue.file, "labels", frontmatter.render_list(chosen))

    result = _apply(request, project, number, edit)
    return result if isinstance(result, Response) else _back(project, number)


@writes.post("/{project_name}/issues/{number:int}/comments")
def post_comment(
    request: Request,
    project_name: str,
    number: int,
    text: FormText = "",
    action: FormText = "comment",
) -> Response:
    project = _project(request, project_name)
    stamp = datetime.now(request.app.state.settings.timezone).strftime(STAMP_FORMAT)

    def edit(issue: Issue) -> str:
        new = issue.file.text
        if text.strip():
            new = add_comment(issue.file, stamp, text)
        elif action not in CLOSE_ACTIONS:
            raise EditError("write a comment first")
        if action in CLOSE_ACTIONS:
            parsed = parse(new)
            style = frontmatter.style_of(parsed.front.raw(new), "state")
            new = set_front(
                parsed, "state", frontmatter.render_scalar(CLOSE_ACTIONS[action], style)
            )
        return new

    result = _apply(request, project, number, edit, draft={"target": "new-comment", "text": text})
    if isinstance(result, Response):
        return result
    anchor = f"#comment-{result.comments[-1].index}" if text.strip() and result.comments else ""
    return _back(project, number, anchor)


@writes.post("/{project_name}/issues/{number:int}/comments/{index:int}")
def update_comment(
    request: Request,
    project_name: str,
    number: int,
    index: int,
    text: FormText = "",
    version: FormText = "",
) -> Response:
    project = _project(request, project_name)
    result = _apply(
        request,
        project,
        number,
        lambda issue: edit_comment(issue.file, index, text),
        expected_version=version or None,
        draft={"target": f"comment-{index}", "text": text},
    )
    return result if isinstance(result, Response) else _back(project, number, f"#comment-{index}")


@writes.post("/{project_name}/issues/{number:int}/comments/{index:int}/delete")
def remove_comment(
    request: Request, project_name: str, number: int, index: int, version: FormText = ""
) -> Response:
    project = _project(request, project_name)
    result = _apply(
        request,
        project,
        number,
        lambda issue: delete_comment(issue.file, index),
        expected_version=version or None,
    )
    return result if isinstance(result, Response) else _back(project, number, "#comments")


@writes.post("/{project_name}/issues/{number:int}/tasks")
def toggle_task(
    request: Request,
    project_name: str,
    number: int,
    part: FormText = "",
    line: Annotated[int, Form()] = -1,
    checked: FormText = "",
    version: FormText = "",
) -> JSONResponse:
    project = _project(request, project_name)
    tick = checked in ("1", "true", "on")
    try:
        issue = project.update(
            number, lambda issue: set_task(issue.file, part, line, tick), version or None
        )
    except ReadOnlyError:
        return JSONResponse({"error": f"{project.name} is read-only."}, status_code=403)
    except NotFoundError:
        return JSONResponse({"error": f"There is no issue #{number}."}, status_code=404)
    except ConflictError:
        message = "This issue changed on disk. Reload to see the latest version first."
        return JSONResponse({"error": message}, status_code=409)
    except EditError as error:
        return JSONResponse({"error": _sentence(error)}, status_code=422)
    return JSONResponse(
        {"version": issue.version, "done": issue.tasks_done, "total": issue.tasks_total}
    )


@writes.post("/{project_name}/preview")
def preview(request: Request, project_name: str, text: FormText = "") -> dict:
    project = _project(request, project_name)
    renderer: Renderer = request.app.state.renderer
    return {"html": renderer.render(text, Context(project.name, _refs(project.snapshot())))}


# Errors and the app itself


def _wants_json(request: Request) -> bool:
    return "application/json" in request.headers.get("accept", "")


async def http_error(request: Request, error: HTTPException) -> Response:
    message = error.detail
    if error.status_code == 404 and message == "Not Found":
        message = "There is nothing at this address."
    if _wants_json(request):
        return JSONResponse({"error": message}, status_code=error.status_code)
    context = {**_base(request), "status": error.status_code, "message": message}
    return _page(request, "error.html", context, error.status_code)


async def validation_error(request: Request, error: RequestValidationError) -> Response:
    message = "The request was not understood."
    if _wants_json(request):
        return JSONResponse({"error": message}, status_code=400)
    return _page(request, "error.html", {**_base(request), "status": 400, "message": message}, 400)


async def security_headers(request: Request, call_next) -> Response:
    response = await call_next(request)
    response.headers.setdefault("Content-Security-Policy", CSP)
    response.headers.setdefault("X-Content-Type-Options", "nosniff")
    response.headers.setdefault("Referrer-Policy", "same-origin")
    response.headers.setdefault("X-Frame-Options", "DENY")
    if not request.url.path.startswith("/static/"):
        response.headers.setdefault("Cache-Control", "no-store")
    return response


def create_app(settings: Settings | None = None) -> FastAPI:
    settings = settings or load_settings()
    app = FastAPI(title="Issue viewer", version=_version(), docs_url=None, redoc_url=None)
    app.state.settings = settings
    app.state.registry = Registry(settings.projects)
    app.state.csrf_token = secrets.token_urlsafe(32)
    app.state.renderer = Renderer()

    templates = Jinja2Templates(directory=str(PACKAGE / "templates"))
    templates.env.trim_blocks = True
    templates.env.lstrip_blocks = True
    templates.env.globals.update(
        icon=icon,
        progress=progress,
        label_style=labels.style,
        label_color=labels.color,
        avatar_style=labels.avatar_style,
        asset_version=_asset_version(),
    )
    app.state.templates = templates

    app.add_middleware(TrustedHostMiddleware, allowed_hosts=list(settings.allowed_hosts))
    app.middleware("http")(security_headers)
    app.add_exception_handler(HTTPException, http_error)
    app.add_exception_handler(RequestValidationError, validation_error)
    app.mount("/static", StaticFiles(directory=PACKAGE / "static"), name="static")
    app.include_router(writes)
    app.include_router(router)

    for project in app.state.registry:
        if not project.exists:
            log.warning("project %s: %s is not a directory", project.name, project.path)
    return app
