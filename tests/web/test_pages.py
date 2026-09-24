import re

from tests.web.conftest import issue_path, row_numbers


def test_the_start_page_lists_every_project_with_its_counts(client):
    page = client.get("/")
    assert page.status_code == 200
    assert 'href="/demo/issues"' in page.text
    assert 'href="/archive/issues"' in page.text
    assert "27 open" in page.text
    assert "13 closed" in page.text
    assert "Read-only" in page.text


def test_a_project_address_leads_to_its_issues(client):
    response = client.get("/demo", follow_redirects=False)
    assert response.headers["location"] == "/demo/issues"


def test_the_issue_list_shows_25_open_issues_newest_first(client):
    page = client.get("/demo/issues")
    numbers = row_numbers(page.text)
    assert len(numbers) == 25
    assert numbers[:4] == [40, 38, 37, 35]
    assert 'Open <span class="Counter">27</span>' in page.text
    assert 'Closed <span class="Counter">13</span>' in page.text
    second = client.get("/demo/issues", params={"q": "is:issue is:open", "page": 2})
    assert row_numbers(second.text) == [4, 3]


def test_the_list_filters_by_label_and_by_text(client):
    assert row_numbers(client.get("/demo/issues", params={"q": "is:open label:backlog"}).text) == [
        8,
        5,
    ]
    assert row_numbers(client.get("/demo/issues", params={"q": "sqlite"}).text) == [6, 3, 2]
    empty = client.get("/demo/issues", params={"q": "nothing-matches-this"})
    assert "No results" in empty.text


def test_a_row_shows_labels_task_progress_comments_and_what_blocks_it(client):
    page = client.get("/demo/issues", params={"q": "is:open label:ui"}).text
    row = page[page.index('id="issue-5"') :][:3000]
    assert "Blocked by" in row and 'href="/demo/issues/3"' in row
    row = page[page.index('id="issue-3"') :][:3000]
    assert "1 of 3 tasks" in row
    assert 'aria-label="2 comments"' in row


def test_the_issue_page_shows_the_description_comments_and_relationships(client):
    page = client.get("/demo/issues/3").text
    assert '<bdi class="js-issue-title">Search notes by title and body</bdi>' in page
    assert '<span class="issue-number">#3</span>' in page
    assert "State--open" in page
    assert page.count('class="timeline-item" id="comment-') == 2
    assert "<table>" in page
    assert "Measured it: 3 ms" in page
    assert "Blocked by" in page and "Blocking" in page
    assert 'href="/demo/issues/2"' in page
    assert "1 of 3 tasks" in page
    assert "<h1>" not in page  # the `# #3 Title` line is the page title, not part of the body


def rendered_description(page):
    """The description as rendered, without the raw Markdown kept in the (hidden) editor."""
    start = page.index('js-comment-body"')
    return page[start : page.index('<form class="timeline-comment-edit', start)]


def test_markup_in_an_issue_is_rendered_safely(client):
    page = client.get("/demo/issues/8").text
    body = rendered_description(page)
    assert "whoever picks this up" not in body
    assert "<script>" not in page
    assert "&lt;script&gt;" in body
    assert "<summary>Why not a server</summary>" in body
    assert 'href="/demo/issues/4"' in body
    assert "/demo/issues/99" not in page
    assert "A code block, not a comment." in body
    assert 'id="comment-' not in page


def test_a_closed_issue_says_how_it_was_closed(client):
    assert "State--done" in client.get("/demo/issues/1").text
    page = client.get("/demo/issues/6").text
    assert "State--not-planned" in page
    assert "as not planned" in page


def test_a_custom_state_is_shown_as_it_is_written(client):
    page = client.get("/demo/issues/10").text
    assert "State--open" in page
    assert '<span class="Label">in-progress</span>' in page


def test_files_without_front_matter_or_with_broken_front_matter_still_show(client):
    plain = client.get("/demo/issues/11").text
    assert "Plain file without front matter" in plain
    broken = client.get("/demo/issues/12")
    assert broken.status_code == 200
    assert "The front matter could not be read" in broken.text
    assert "js-title-edit" not in broken.text
    assert "js-comment-form" not in broken.text


def test_an_unknown_issue_or_project_is_a_404_page(client):
    missing = client.get("/demo/issues/999")
    assert missing.status_code == 404
    assert "There is no issue #999 in demo." in missing.text
    assert client.get("/nope/issues").status_code == 404
    assert client.get("/nope/-/version", headers={"accept": "application/json"}).json() == {
        "error": "There is no project called “nope”."
    }


def test_the_labels_page_counts_open_and_closed_issues_per_label(client):
    page = client.get("/demo/labels").text
    assert re.search(r">backlog</a>.*?2 open.*?0 closed", page, re.S)
    assert re.search(r">enhancement</a>.*?3 open.*?3 closed", page, re.S)


def test_the_new_issue_form_starts_from_the_template(client, dirs):
    page = client.get("/demo/issues/new").text
    assert "## Acceptance criteria" in page
    (dirs["demo"] / "TEMPLATE.md").write_text("## What we saw\n")
    assert "## What we saw" in client.get("/demo/issues/new").text


def test_a_read_only_project_shows_no_write_controls(client):
    listing = client.get("/archive/issues").text
    assert "js-new-issue" not in listing
    page = client.get("/archive/issues/2").text
    for control in ("js-comment-form", "js-title-edit", "js-label-picker", "js-edit-open"):
        assert control not in page
    assert "task-list-item-checkbox" in page and "disabled" in page
    assert "This project is read-only" in page
    assert client.get("/archive/issues/new").status_code == 403


def test_the_version_changes_when_a_file_changes_on_disk(client, dirs):
    project_before = client.get("/demo/-/version").json()["version"]
    issue_before = client.get("/demo/-/version", params={"issue": 4}).json()["version"]
    path = issue_path(dirs["demo"], 4)
    path.write_text(path.read_text() + "\nMore.\n")
    assert client.get("/demo/-/version").json()["version"] != project_before
    assert client.get("/demo/-/version", params={"issue": 4}).json()["version"] != issue_before
    assert client.get("/demo/-/version", params={"issue": 999}).json() == {"version": None}


def test_pages_forbid_scripts_and_frames_from_anywhere_else(client):
    page = client.get("/demo/issues")
    policy = page.headers["content-security-policy"]
    assert "script-src 'self'" in policy
    assert "frame-ancestors 'none'" in policy
    assert page.headers["cache-control"] == "no-store"
    assert page.headers["x-content-type-options"] == "nosniff"


def test_pages_load_nothing_from_outside_the_viewer(client):
    for url in ("/", "/demo/issues", "/demo/issues/8", "/demo/issues/new", "/demo/labels"):
        html = client.get(url).text
        for address in re.findall(r'<(?:script|link|img)\b[^>]*?\b(?:src|href)="([^"]+)"', html):
            assert address.startswith("/"), (url, address)


def test_the_health_check_answers(client):
    assert client.get("/healthz").json() == {"status": "ok"}
