"""Smoke tests: the production image, running, serves pages built from files on disk.

They run in the e2e container against the e2e-app service (see compose.yaml), which reads the
synthetic issues this module writes into the shared volume.
"""

import json
import os
import urllib.request
from pathlib import Path

import pytest
from tests.support.sample import write_archive, write_demo

BASE = os.environ.get("BASE_URL", "http://localhost:8080")
DATA = Path(os.environ.get("DATA_DIR", "/data"))


@pytest.fixture(scope="module", autouse=True)
def sample_issues():
    write_demo(DATA / "demo", shared=True)
    write_archive(DATA / "archive", shared=True)


def get(path):
    with urllib.request.urlopen(BASE + path, timeout=10) as response:
        return response.status, response.read().decode("utf-8"), response.headers


def test_the_viewer_answers_its_health_check():
    status, body, _ = get("/healthz")
    assert status == 200
    assert json.loads(body) == {"status": "ok"}


def test_the_start_page_lists_both_projects():
    _, body, headers = get("/")
    assert 'href="/demo/issues"' in body
    assert 'href="/archive/issues"' in body
    assert "script-src 'self'" in headers["Content-Security-Policy"]


def test_the_issue_list_and_an_issue_page_come_from_the_files():
    _, listing, _ = get("/demo/issues")
    assert "Search notes by title and body" in listing
    _, page, _ = get("/demo/issues/3")
    assert "Measured it: 3 ms" in page
    assert "task-list-item-checkbox" in page


def test_the_stylesheet_the_script_and_the_icon_are_served():
    for path, kind in (
        ("/static/app.css", "text/css"),
        ("/static/app.js", "javascript"),
        ("/static/favicon.svg", "svg"),
    ):
        status, _, headers = get(path)
        assert status == 200
        assert kind in headers["Content-Type"]


def test_the_read_only_project_offers_no_writes():
    _, page, _ = get("/archive/issues/2")
    assert "js-comment-form" not in page
    assert "Read-only" in page


def test_the_open_page_lists_the_folders_under_the_open_root():
    _, body, _ = get("/-/open")
    assert "Go to demo" in body
    assert "Go to archive" in body
