import pytest

from issue_viewer.browse import (
    BrowseError,
    count_issues,
    folders,
    project_name,
    relative_to,
    resolve,
)
from issue_viewer.settings import PROJECT_NAME


@pytest.fixture
def root(tmp_path):
    """A made-up tree: two apps with issue folders, a hidden folder, a file and a way out."""
    base = tmp_path / "root"
    (base / "notes-app" / "tasks").mkdir(parents=True)
    (base / "notes-app" / "tasks" / "001-first.md").write_text("# #1 First\n")
    (base / "notes-app" / "tasks" / "002-second.md").write_text("# #2 Second\n")
    (base / "notes-app" / "tasks" / "README.md").write_text("# Issues\n")
    (base / "notes-app" / "docs" / "plan" / "issues").mkdir(parents=True)
    (base / "Archive 2025").mkdir()
    (base / ".git").mkdir()
    (base / "readme.txt").write_text("not a folder\n")
    (tmp_path / "outside").mkdir()
    (base / "escape").symlink_to(tmp_path / "outside")
    return base


def test_a_path_under_the_root_resolves_to_that_folder(root):
    assert resolve(root, "notes-app/tasks") == (root / "notes-app" / "tasks").resolve()
    assert resolve(root, "") == root.resolve()
    assert resolve(root, "/notes-app/./tasks/") == (root / "notes-app" / "tasks").resolve()


@pytest.mark.parametrize(
    "path",
    [
        "..",
        "notes-app/../..",
        ".git",
        "notes-app/.hidden",
        "escape",
        "readme.txt",
        "missing",
        "a\0b",
    ],
)
def test_hidden_folders_ways_out_files_and_missing_folders_cannot_be_opened(root, path):
    with pytest.raises(BrowseError):
        resolve(root, path)


def test_the_listing_shows_visible_folders_with_their_issue_counts(root):
    found = folders(root, resolve(root, ""))
    assert [(f.name, f.relative, f.issues, f.issue_folders) for f in found] == [
        ("Archive 2025", "Archive 2025", 0, ()),
        ("notes-app", "notes-app", 0, ("tasks",)),
    ]
    inner = folders(root, resolve(root, "notes-app"))
    assert [(f.relative, f.issues) for f in inner] == [
        ("notes-app/docs", 0),
        ("notes-app/tasks", 2),
    ]


def test_only_numbered_markdown_files_count_as_issues(root):
    assert count_issues(root / "notes-app" / "tasks") == 2
    assert count_issues(root / "missing") == 0


def test_the_relative_path_of_the_root_is_empty(root):
    assert relative_to(root, resolve(root, "")) == ""
    assert relative_to(root, resolve(root, "notes-app/tasks")) == "notes-app/tasks"


@pytest.mark.parametrize(
    ("relative", "name"),
    [
        ("notes-app/tasks", "notes-app"),
        ("notes-app/docs/plan/issues", "notes-app.docs.plan"),
        ("tasks", "tasks"),
        ("Archive 2025/to do!", "Archive-2025.to-do"),
        ("", "root"),
    ],
)
def test_a_name_comes_from_the_path_without_the_folder_that_holds_the_issues(root, relative, name):
    assert project_name(root, relative, set()) == name


def test_a_name_that_is_taken_gets_the_next_number(root):
    assert project_name(root, "notes-app/tasks", {"notes-app"}) == "notes-app-2"
    assert project_name(root, "notes-app/tasks", {"notes-app", "notes-app-2"}) == "notes-app-3"


def test_a_long_path_keeps_the_end_of_its_name_and_stays_a_valid_name(root):
    name = project_name(root, "/".join(["a-very-long-folder-name"] * 6) + "/tasks", set())
    assert len(name) <= 64
    assert name.endswith("a-very-long-folder-name")
    assert PROJECT_NAME.match(name)


def test_a_name_that_would_shadow_a_route_is_not_used(root):
    assert project_name(root, "static/tasks", set()) == "folder"
