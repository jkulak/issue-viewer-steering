import json
from pathlib import Path

from tests.web.conftest import issue_path

from issue_viewer.openapi import schema

ROOT = Path(__file__).resolve().parents[2]


def contents(directory):
    return {path.name: path.read_bytes() for path in sorted(directory.iterdir())}


def test_a_write_without_the_token_or_with_a_wrong_one_is_refused(client, dirs):
    before = contents(dirs["demo"])
    assert client.post("/demo/issues/4/comments", data={"action": "close"}).status_code == 403
    response = client.post("/demo/issues/4/comments", data={"action": "close", "_csrf": "guess"})
    assert response.status_code == 403
    assert "Reload" in response.text
    assert contents(dirs["demo"]) == before


def test_a_write_from_another_origin_is_refused_even_with_the_token(client, token, dirs):
    before = contents(dirs["demo"])
    for origin in ("https://attacker.example", "null", "http://localhost:9999"):
        response = client.post(
            "/demo/issues/4/comments",
            data={"_csrf": token, "action": "close"},
            headers={"Origin": origin},
        )
        assert response.status_code == 403, origin
    response = client.post(
        "/demo/issues/4/comments",
        data={"_csrf": token, "action": "close"},
        headers={"Sec-Fetch-Site": "cross-site"},
    )
    assert response.status_code == 403
    assert contents(dirs["demo"]) == before


def test_a_write_from_the_viewers_own_page_goes_through(client, token, dirs):
    response = client.post(
        "/demo/issues/4/comments",
        data={"_csrf": token, "action": "close"},
        headers={"Origin": "http://localhost", "Sec-Fetch-Site": "same-origin"},
        follow_redirects=False,
    )
    assert response.status_code == 303
    assert "state: done" in issue_path(dirs["demo"], 4).read_text()


def test_a_host_name_the_viewer_does_not_answer_to_is_refused(client):
    assert (
        client.get("/demo/issues", headers={"Host": "rebound.attacker.example"}).status_code == 400
    )
    assert client.get("/demo/issues", headers={"Host": "127.0.0.1:8080"}).status_code == 200


def test_every_write_to_a_read_only_project_is_refused(client, token, dirs):
    before = contents(dirs["archive"])
    writes = [
        ("/archive/issues", {"title": "x"}),
        ("/archive/issues/2/title", {"title": "x"}),
        ("/archive/issues/2/description", {"text": "x"}),
        ("/archive/issues/2/labels", {"labels": "x"}),
        ("/archive/issues/2/comments", {"text": "x", "action": "close"}),
        ("/archive/issues/2/comments/0/delete", {}),
    ]
    for url, fields in writes:
        response = client.post(url, data={"_csrf": token, **fields}, follow_redirects=False)
        assert response.status_code == 403, url
    response = client.post(
        "/archive/issues/2/tasks",
        data={"part": "description", "line": "0", "checked": "1"},
        headers={"X-CSRF-Token": token, "Accept": "application/json"},
    )
    assert response.status_code == 403
    assert contents(dirs["archive"]) == before


def test_no_address_reaches_a_file_outside_the_configured_directories(client):
    for url in (
        "/../etc/passwd",
        "/%2e%2e/%2e%2e/etc/passwd",
        "/demo/issues/..%2F..%2Fetc%2Fpasswd",
        "/demo/issues/-1",
        "/static/../app.py",
        "/static/%2e%2e/app.py",
    ):
        response = client.get(url)
        assert response.status_code == 404, url
        assert "root:" not in response.text


def test_the_published_openapi_schema_is_up_to_date():
    published = json.loads((ROOT / "openapi.json").read_text())
    assert published == schema(), "run `make openapi` and commit openapi.json"
