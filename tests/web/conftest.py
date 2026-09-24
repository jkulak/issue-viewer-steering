import re

import pytest
from fastapi.testclient import TestClient
from tests.support.sample import write_archive, write_demo

from issue_viewer.app import create_app
from issue_viewer.settings import ProjectConfig, Settings


@pytest.fixture
def dirs(tmp_path):
    return {"demo": write_demo(tmp_path / "demo"), "archive": write_archive(tmp_path / "archive")}


@pytest.fixture
def app(dirs):
    projects = (
        ProjectConfig("demo", dirs["demo"]),
        ProjectConfig("archive", dirs["archive"], read_only=True),
    )
    return create_app(Settings(projects=projects))


@pytest.fixture
def client(app):
    with TestClient(app, base_url="http://localhost") as client:
        yield client


@pytest.fixture
def token(app):
    return app.state.csrf_token


@pytest.fixture
def post(client, token):
    def send(url, **fields):
        return client.post(url, data={"_csrf": token, **fields}, follow_redirects=False)

    return send


def issue_path(directory, number):
    return next(directory.glob(f"{number:03d}-*.md"))


def version_of(client, number):
    return client.get("/demo/-/version", params={"issue": number}).json()["version"]


def row_numbers(html):
    return [int(n) for n in re.findall(r'id="issue-(\d+)"', html)]
