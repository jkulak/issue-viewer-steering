import pytest
from fastapi.testclient import TestClient
from tests.support.sample import ARCHIVE, write_project

from issue_viewer.app import create_app
from issue_viewer.settings import OpenRoot, ProjectConfig, Settings

OPENED_LABEL = 'title="Picked with Open; forgotten when the viewer restarts">Opened</span>'


@pytest.fixture
def root(tmp_path, dirs):
    """The open root: demo and archive from `dirs`, a notes app, and a hidden folder."""
    write_project(tmp_path / "notes-app" / "tasks", ARCHIVE, None)
    write_project(tmp_path / ".hidden" / "tasks", ARCHIVE, None)
    return tmp_path


def viewer(root, dirs, read_only=False):
    settings = Settings(
        projects=(ProjectConfig("demo", dirs["demo"]),), open_root=OpenRoot(root, read_only)
    )
    return TestClient(create_app(settings), base_url="http://localhost")


@pytest.fixture
def browser(root, dirs):
    with viewer(root, dirs) as client:
        yield client


def open_folder(client, path, **headers):
    return client.post(
        "/-/open",
        data={"_csrf": client.app.state.csrf_token, "path": path},
        headers=headers,
        follow_redirects=False,
    )


def project_names(client):
    return [project.name for project in client.app.state.registry]


def test_without_an_open_root_there_is_no_open_button_and_no_open_page(client, post):
    assert 'href="/-/open"' not in client.get("/").text
    assert client.get("/-/open").status_code == 404
    assert post("/-/open", path="demo").status_code == 404


def test_every_page_has_an_open_button_when_an_open_root_is_set(browser):
    for url in ("/", "/demo/issues", "/demo/issues/3", "/demo/labels"):
        assert 'href="/-/open"' in browser.get(url).text, url


def test_the_open_page_lists_the_folders_under_the_root_with_their_issue_files(browser):
    page = browser.get("/-/open").text
    assert 'href="/-/open?path=notes-app"' in page
    assert 'href="/-/open?path=notes-app%2Ftasks">tasks/</a>' in page
    assert "Go to demo" in page
    assert "2 issue files" in page  # the archive folder
    assert ".hidden" not in page
    inner = browser.get("/-/open", params={"path": "notes-app/tasks"}).text
    assert "2 issue files in this folder" in inner
    assert 'href="/-/open?path=notes-app" aria-label="Up one folder"' in inner
    assert 'name="path" value="notes-app/tasks"' in inner


def test_opening_a_folder_adds_it_as_a_project_and_shows_its_issues(browser):
    response = open_folder(browser, "notes-app/tasks")
    assert response.status_code == 303
    assert response.headers["location"] == "/notes-app/issues"
    listing = browser.get("/notes-app/issues").text
    assert "Another old idea" in listing
    assert OPENED_LABEL in listing
    home = browser.get("/").text
    assert 'href="/notes-app/issues"' in home
    assert OPENED_LABEL in home
    assert OPENED_LABEL not in browser.get("/demo/issues").text


def test_opening_a_folder_again_or_one_from_projects_goes_to_the_existing_project(browser):
    assert open_folder(browser, "notes-app/tasks").headers["location"] == "/notes-app/issues"
    assert open_folder(browser, "notes-app/tasks/").headers["location"] == "/notes-app/issues"
    assert open_folder(browser, "demo").headers["location"] == "/demo/issues"
    assert project_names(browser) == ["demo", "notes-app"]
    assert "Go to notes-app" in browser.get("/-/open", params={"path": "notes-app"}).text


def test_an_opened_folder_can_be_changed_when_the_open_root_is_read_write(browser, root):
    open_folder(browser, "notes-app/tasks")
    token = browser.app.state.csrf_token
    response = browser.post(
        "/notes-app/issues/2/comments",
        data={"_csrf": token, "action": "close"},
        follow_redirects=False,
    )
    assert response.status_code == 303
    assert "state: done" in (root / "notes-app" / "tasks" / "002-another-old-idea.md").read_text()


def test_an_opened_folder_is_read_only_when_the_open_root_is(root, dirs):
    path = root / "notes-app" / "tasks" / "002-another-old-idea.md"
    before = path.read_bytes()
    with viewer(root, dirs, read_only=True) as client:
        assert open_folder(client, "notes-app/tasks").status_code == 303
        assert "Read-only" in client.get("/notes-app/issues/2").text
        response = client.post(
            "/notes-app/issues/2/comments",
            data={"_csrf": client.app.state.csrf_token, "action": "close"},
            follow_redirects=False,
        )
        assert response.status_code == 403
    assert path.read_bytes() == before


def test_opening_needs_the_token_and_a_request_from_the_viewers_own_pages(browser):
    assert browser.post("/-/open", data={"path": "notes-app/tasks"}).status_code == 403
    assert (
        open_folder(browser, "notes-app/tasks", Origin="https://attacker.example").status_code
        == 403
    )
    assert (
        open_folder(browser, "notes-app/tasks", **{"Sec-Fetch-Site": "cross-site"}).status_code
        == 403
    )
    assert project_names(browser) == ["demo"]


@pytest.mark.parametrize(
    "path",
    ["..", "../..", "notes-app/../..", ".hidden", ".hidden/tasks", "missing", "demo/README.md"],
)
def test_hidden_folders_and_ways_out_of_the_root_can_be_neither_listed_nor_opened(browser, path):
    assert browser.get("/-/open", params={"path": path}).status_code == 404
    assert open_folder(browser, path).status_code == 404
    assert project_names(browser) == ["demo"]


def test_a_link_to_a_folder_that_is_no_longer_open_says_why(browser):
    response = browser.get("/notes-app/issues")
    assert response.status_code == 404
    assert "forgotten when the viewer restarts" in response.text
    assert 'href="/-/open"' in response.text
