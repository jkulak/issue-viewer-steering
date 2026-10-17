"""Browser tests: the production image, used through Chromium the way a person would use it.

Every test starts from a fresh copy of the synthetic issues in the shared volume, and checks the
file on disk as well as the page, because the file is what the viewer is for.
"""

import os
import re
from pathlib import Path

import pytest
from playwright.sync_api import expect, sync_playwright
from tests.support.sample import write_archive, write_demo

BASE = os.environ.get("BASE_URL", "http://localhost:8080")
DATA = Path(os.environ.get("DATA_DIR", "/data"))


@pytest.fixture(scope="session")
def browser():
    with sync_playwright() as playwright:
        browser = playwright.chromium.launch()
        yield browser
        browser.close()


@pytest.fixture
def page(browser):
    write_demo(DATA / "demo", shared=True)
    write_archive(DATA / "archive", shared=True)
    context = browser.new_context(base_url=BASE)
    page = context.new_page()
    page.set_default_timeout(5000)
    yield page
    context.close()


def issue_file(number):
    return next((DATA / "demo").glob(f"{number:03d}-*.md"))


def test_filtering_the_list_by_a_label_from_the_menu(page):
    page.goto("/demo/issues")
    expect(page.locator(".issue-row")).to_have_count(25)
    page.locator(".header-filters summary", has_text="Labels").click()
    page.locator(".header-filters .menu-item", has_text="backlog").click()
    expect(page.locator(".js-search-input")).to_have_value("is:issue is:open label:backlog")
    expect(page.locator(".issue-row")).to_have_count(2)


def test_the_slash_key_jumps_to_the_search_box(page):
    page.goto("/demo/issues")
    page.keyboard.press("/")
    expect(page.locator(".js-search-input")).to_be_focused()


def test_ticking_a_checkbox_writes_that_line_of_the_file(page):
    page.goto("/demo/issues/3")
    box = page.locator("#description li", has_text="Matches are highlighted").locator("input")
    box.check()
    expect(page.locator(".issue-meta .js-task-summary")).to_have_text("2 of 3 tasks")
    assert "- [x] Matches are highlighted" in issue_file(3).read_text()


def test_commenting_and_then_closing_the_issue(page):
    page.goto("/demo/issues/4")
    page.locator(".js-comment-form textarea").fill("Reproduced it on the laptop.")
    expect(page.locator(".js-close-label")).to_have_text("Close with comment")
    page.locator(".js-comment-button").click()
    expect(page.locator("#comment-0 .timeline-comment-body")).to_contain_text(
        "Reproduced it on the laptop."
    )
    expect(page.locator(".js-comment-button")).to_be_disabled()
    page.locator(".js-close-button").click()
    expect(page.locator(".issue-meta .State")).to_have_text("Closed")
    text = issue_file(4).read_text()
    assert "state: done" in text
    assert "Reproduced it on the laptop." in text


def test_the_preview_tab_shows_the_rendered_comment(page):
    page.goto("/demo/issues/4")
    page.locator(".js-comment-form textarea").fill("**Bold** and a link to #3")
    page.locator(".js-comment-form .js-editor-tab[data-tab=preview]").click()
    preview = page.locator(".js-comment-form .js-editor-preview")
    expect(preview.locator("strong")).to_have_text("Bold")
    expect(preview.locator("a.issue-link")).to_have_attribute("href", "/demo/issues/3")


def test_editing_the_title_in_place(page):
    page.goto("/demo/issues/5")
    page.locator(".js-title-edit").click()
    page.locator(".issue-title-input").fill("Dark theme that follows the system")
    page.get_by_role("button", name="Save").click()
    expect(page.locator(".js-issue-title")).to_have_text("Dark theme that follows the system")
    text = issue_file(5).read_text()
    assert 'title: "Dark theme that follows the system"' in text
    assert "# #5 Dark theme that follows the system" in text


def test_editing_the_description(page):
    page.goto("/demo/issues/7")
    page.locator("#description summary.btn-octicon").click()
    page.locator("#description .js-edit-open").click()
    page.locator("#edit-description textarea").fill(
        "Shortcuts for everything.\n\n- [ ] `n` for a new note"
    )
    page.get_by_role("button", name="Update description").click()
    expect(page.locator("#description .timeline-comment-body")).to_contain_text(
        "Shortcuts for everything."
    )
    assert "Shortcuts for everything." in issue_file(7).read_text()


def test_labels_picked_in_the_sidebar_apply_when_the_menu_closes(page):
    page.goto("/demo/issues/7")
    page.locator(".label-picker summary").click()
    page.locator(".label-option", has_text="ui").click()
    page.keyboard.press("Escape")
    expect(page.locator(".sidebar-labels .IssueLabel")).to_have_text(["ui"])
    assert "labels: [ui]" in issue_file(7).read_text()


def test_creating_an_issue_with_a_new_label(page):
    page.goto("/demo/issues/new")
    page.locator("#issue-title").fill("Export notes as CSV")
    page.locator(".label-picker summary").click()
    page.locator(".js-label-filter").fill("needs design")
    page.keyboard.press("Enter")
    page.locator(".js-label-filter").fill("data")
    page.keyboard.press("Enter")
    page.keyboard.press("Escape")
    expect(page.locator(".js-chosen-labels .IssueLabel")).to_have_count(2)
    page.get_by_role("button", name="Create").click()
    expect(page.locator(".issue-number")).to_have_text("#41")
    expect(page.locator(".js-issue-title")).to_have_text("Export notes as CSV")
    labels = re.search(r"^labels: \[(.*)\]$", issue_file(41).read_text(), re.M).group(1)
    assert sorted(labels.split(", ")) == ["data", "needs design"]


def test_deleting_a_comment_asks_first(page):
    page.goto("/demo/issues/3")
    page.once("dialog", lambda dialog: dialog.accept())
    page.locator("#comment-1 summary.btn-octicon").click()
    page.locator("#comment-1 .menu-item-danger").click()
    expect(page.locator("#comment-1")).to_have_count(0)
    assert "Measured it" not in issue_file(3).read_text()


def test_a_change_on_disk_shows_up_without_a_reload(page):
    page.goto("/demo/issues/7")
    path = issue_file(7)
    path.write_text(
        path.read_text().replace('"Keyboard shortcuts"', '"Keyboard shortcuts everywhere"')
    )
    expect(page.locator(".js-issue-title")).to_have_text(
        "Keyboard shortcuts everywhere", timeout=8000
    )


def test_a_change_on_disk_keeps_the_text_being_typed(page):
    page.goto("/demo/issues/7")
    page.locator(".js-comment-form textarea").fill("Half a thought")
    path = issue_file(7)
    path.write_text(path.read_text() + "\nMore from elsewhere.\n")
    expect(page.locator(".live-banner")).to_be_visible(timeout=8000)
    expect(page.locator(".js-comment-form textarea")).to_have_value("Half a thought")


def test_a_read_only_project_offers_nothing_to_change(page):
    page.goto("/archive/issues/2")
    expect(page.locator(".js-comment-form")).to_have_count(0)
    expect(page.locator(".js-title-edit")).to_have_count(0)
    expect(page.locator(".task-list-item-checkbox")).to_be_disabled()


def test_opening_a_folder_from_the_header(page):
    write_archive(DATA / "notes-app" / "tasks", shared=True)
    page.goto("/")
    page.locator(".app-header-open").click()
    page.get_by_role("link", name="notes-app", exact=True).click()
    page.get_by_role("link", name="Up one folder").click()
    page.get_by_role("link", name="tasks/", exact=True).click()
    expect(page.locator(".Box-title")).to_have_text("2 issue files in this folder")
    page.get_by_role("button", name="Open this folder").click()
    expect(page).to_have_url(re.compile(r"/notes-app/issues$"))
    expect(page.get_by_role("link", name="Another old idea")).to_be_visible()
    page.goto("/")
    expect(page.locator('a[href="/notes-app/issues"]').first).to_be_visible()
