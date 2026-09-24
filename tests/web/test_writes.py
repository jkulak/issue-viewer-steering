import re

from tests.web.conftest import issue_path, version_of

JSON = {"Accept": "application/json"}


def test_creating_an_issue_writes_the_next_file_and_a_readme_row(post, dirs):
    response = post(
        "/demo/issues", title="Export notes", description="Body", labels=["data", "needs design"]
    )
    assert response.status_code == 303
    assert response.headers["location"] == "/demo/issues/41"
    text = (dirs["demo"] / "041-export-notes.md").read_text()
    assert text == (
        '---\ntitle: "Export notes"\nlabels: [data, needs design]\nstate: open\n---\n\n'
        "# #41 Export notes\n\nBody\n"
    )
    readme = (dirs["demo"] / "README.md").read_text()
    assert (
        "| 41 | [Export notes](041-export-notes.md) | data, needs design | open | – |\n\nOrder"
        in readme
    )


def test_an_issue_needs_a_title_and_keeps_what_was_typed(post, dirs):
    response = post("/demo/issues", title="   ", description="Kept text", labels=["bug"])
    assert response.status_code == 422
    assert "A title is required." in response.text
    assert "Kept text" in response.text
    assert not list(dirs["demo"].glob("041-*"))


def test_editing_the_title_changes_the_front_matter_the_heading_and_the_readme_row(post, dirs):
    response = post("/demo/issues/4/title", title="Saving  without a title crashes")
    assert response.status_code == 303
    text = issue_path(dirs["demo"], 4).read_text()
    assert "title: Saving without a title crashes\n" in text
    assert "# #4 Saving without a title crashes\n" in text
    readme = (dirs["demo"] / "README.md").read_text()
    assert (
        "| 4 | [Saving without a title crashes](004-crash-on-empty-title.md) | bug | open | – |"
        in readme
    )


def test_closing_as_completed_or_not_planned_and_reopening(post, client, dirs):
    path = issue_path(dirs["demo"], 4)
    assert post("/demo/issues/4/comments", action="close").status_code == 303
    assert "state: done\n" in path.read_text()
    post("/demo/issues/4/comments", action="reopen")
    assert "state: open\n" in path.read_text()
    post("/demo/issues/4/comments", action="close-not-planned")
    assert "state: not-planned\n" in path.read_text()
    assert "## Comments" not in path.read_text()
    assert "State--not-planned" in client.get("/demo/issues/4").text
    assert (
        "| 4 | [Crash on empty title](004-crash-on-empty-title.md) | bug | not-planned | – |"
        in (dirs["demo"] / "README.md").read_text()
    )


def test_commenting_appends_a_dated_comment_to_the_file(post, dirs):
    response = post("/demo/issues/4/comments", text="Reproduced on the laptop.", action="comment")
    assert response.headers["location"] == "/demo/issues/4#comment-0"
    text = issue_path(dirs["demo"], 4).read_text()
    assert re.search(
        r"3\. Press save\n\n## Comments\n\n### \d{4}-\d\d-\d\d \d\d:\d\d\n\nReproduced on the laptop\.\n$",
        text,
    )


def test_an_empty_comment_changes_nothing(post, dirs):
    path = issue_path(dirs["demo"], 4)
    before = path.read_text()
    response = post("/demo/issues/4/comments", text="   ", action="comment")
    assert response.status_code == 422
    assert "write a comment first" in response.text
    assert path.read_text() == before


def test_close_with_comment_does_both(post, dirs):
    post("/demo/issues/4/comments", text="Fixed by the new save flow.", action="close")
    text = issue_path(dirs["demo"], 4).read_text()
    assert "state: done\n" in text
    assert text.endswith("Fixed by the new save flow.\n")


def test_editing_and_then_deleting_a_comment(post, client, dirs):
    path = issue_path(dirs["demo"], 3)
    response = post(
        "/demo/issues/3/comments/0", text="Highlighting first.", version=version_of(client, 3)
    )
    assert response.headers["location"] == "/demo/issues/3#comment-0"
    assert "Highlighting first." in path.read_text()
    assert "Highlighting can wait" not in path.read_text()
    post("/demo/issues/3/comments/1/delete", version=version_of(client, 3))
    text = path.read_text()
    assert "Measured it" not in text
    assert text.endswith("### 2026-09-20 10:15\n\nHighlighting first.\n")


def test_editing_the_description_keeps_the_heading_and_the_comments(post, client, dirs):
    path = issue_path(dirs["demo"], 3)
    response = post(
        "/demo/issues/3/description",
        text="Short now.\r\n\r\n- [ ] one",
        version=version_of(client, 3),
    )
    assert response.status_code == 303
    text = path.read_text()
    assert "# #3 Search notes by title and body\n\nShort now.\n\n- [ ] one\n\n## Comments\n" in text
    assert text.endswith("- [ ] Try it with 20000 notes\n")
    readme = (dirs["demo"] / "README.md").read_text()
    assert "| 3 | [Search notes](003-search-notes.md) | enhancement, ui | open | – |" in readme


def test_a_description_edit_on_an_older_version_is_refused_and_the_text_kept(post, client, dirs):
    path = issue_path(dirs["demo"], 4)
    version = version_of(client, 4)
    path.write_text(path.read_text() + "\nChanged elsewhere.\n")
    response = post("/demo/issues/4/description", text="My new description", version=version)
    assert response.status_code == 409
    assert "changed on disk" in response.text
    assert "My new description" in response.text
    assert "Changed elsewhere." in path.read_text()
    assert "My new description" not in path.read_text()


def test_labels_are_set_new_ones_are_kept_and_existing_ones_matched_ignoring_case(
    post, client, dirs, token
):
    path = issue_path(dirs["demo"], 7)
    post("/demo/issues/7/labels", labels=["UI", "needs design", " "])
    assert "labels: [ui, needs design]\n" in path.read_text()
    client.post("/demo/issues/7/labels", data={"_csrf": token}, follow_redirects=False)
    assert "labels: []\n" in path.read_text()


def test_a_label_with_a_comma_is_refused(post):
    response = post("/demo/issues/7/labels", labels=["a,b"])
    assert response.status_code == 422
    assert "cannot contain a comma" in response.text


def test_ticking_checkboxes_in_the_description_and_in_a_comment(client, token, dirs):
    page = client.get("/demo/issues/3").text
    version = re.search(r'data-version="([0-9a-f]+)"', page).group(1)
    line = re.search(
        r'data-part="description" data-line="(\d+)"> Matches are highlighted', page
    ).group(1)
    headers = {**JSON, "X-CSRF-Token": token}
    fields = {"part": "description", "line": line, "checked": "1", "version": version}
    response = client.post("/demo/issues/3/tasks", data=fields, headers=headers)
    assert response.status_code == 200
    body = response.json()
    assert (body["done"], body["total"]) == (2, 3)
    assert body["version"] != version
    assert "- [x] Matches are highlighted" in issue_path(dirs["demo"], 3).read_text()

    stale = client.post("/demo/issues/3/tasks", data=fields, headers=headers)
    assert stale.status_code == 409

    comment_line = re.search(r'data-part="1" data-line="(\d+)"> Try it', page).group(1)
    fields = {"part": "1", "line": comment_line, "checked": "1", "version": body["version"]}
    assert client.post("/demo/issues/3/tasks", data=fields, headers=headers).status_code == 200
    assert "- [x] Try it with 20000 notes" in issue_path(dirs["demo"], 3).read_text()


def test_the_preview_renders_markdown_with_issue_links(client, token):
    response = client.post(
        "/demo/preview", data={"text": "**bold** and #3"}, headers={**JSON, "X-CSRF-Token": token}
    )
    html = response.json()["html"]
    assert "<strong>bold</strong>" in html
    assert 'href="/demo/issues/3"' in html
