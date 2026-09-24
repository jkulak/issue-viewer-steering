import os

import pytest
from tests.support.sample import write_demo

from issue_viewer.issuefile import set_description, set_front
from issue_viewer.project import ConflictError, Project, ReadOnlyError
from issue_viewer.settings import ProjectConfig


@pytest.fixture
def demo(tmp_path):
    return Project(ProjectConfig("demo", write_demo(tmp_path / "demo")))


def test_a_snapshot_has_one_issue_per_number_and_hides_later_duplicates(demo):
    (demo.path / "003-z-second-three.md").write_text("---\ntitle: dup\n---\n")
    (demo.path / "notes.md").write_text("not an issue")
    (demo.path / ".003-x.md.1234.tmp").write_text("half written")
    snapshot = demo.snapshot()
    assert [issue.number for issue in snapshot.issues] == list(range(1, 41))
    assert snapshot.by_number[3].filename == "003-search-notes.md"
    assert snapshot.duplicates == ("003-z-second-three.md",)
    assert (snapshot.open_count, snapshot.closed_count) == (27, 13)


def test_a_snapshot_rereads_a_file_that_changed_on_disk(demo):
    assert demo.snapshot().by_number[4].title == "The app crashes when a note title is empty"
    path = demo.path / "004-crash-on-empty-title.md"
    path.write_text(path.read_text().replace("The app crashes", "It crashes"))
    os.utime(path, ns=(1, 1))
    assert demo.snapshot().by_number[4].title == "It crashes when a note title is empty"


def test_blocking_combines_the_blocks_line_with_the_issues_that_depend_on_this_one(demo):
    snapshot = demo.snapshot()
    assert snapshot.blocking(3) == [5, 8]
    assert snapshot.blocking(2) == [3, 8]
    assert [issue.number for issue in snapshot.open_dependencies(snapshot.by_number[5])] == [3]


def test_an_update_keeps_windows_newlines_and_the_file_mode(demo):
    path = demo.path / "004-crash-on-empty-title.md"
    path.write_bytes(path.read_bytes().replace(b"\n", b"\r\n"))
    path.chmod(0o640)
    demo.update(4, lambda issue: set_front(issue.file, "state", "done"))
    data = path.read_bytes()
    assert b"state: done\r\n" in data
    assert b"\n" not in data.replace(b"\r\n", b"")
    assert path.stat().st_mode & 0o777 == 0o640
    assert not [p for p in demo.path.iterdir() if p.name.endswith(".tmp")]


def test_an_update_based_on_an_old_version_is_a_conflict(demo):
    version = demo.fresh(4).version
    path = demo.path / "004-crash-on-empty-title.md"
    path.write_text(path.read_text() + "\nEdited elsewhere.\n")
    with pytest.raises(ConflictError):
        demo.update(4, lambda issue: set_description(issue.file, "mine"), version)
    assert "Edited elsewhere." in path.read_text()


def test_an_update_syncs_the_readme_row(demo):
    demo.update(3, lambda issue: set_front(issue.file, "state", "done"))
    readme = (demo.path / "README.md").read_text()
    assert "| 3 | [Search notes](003-search-notes.md) | enhancement, ui | done | #2 |" in readme
    assert readme.endswith("Order of work: 1 → 2 → 3.\n")


def test_create_takes_the_next_number_and_appends_a_readme_row(demo):
    issue = demo.create("Export notes: CSV", ["data"], "Body text")
    assert issue.number == 41
    assert issue.filename == "041-export-notes-csv.md"
    assert (
        (demo.path / "041-export-notes-csv.md")
        .read_text()
        .startswith(
            '---\ntitle: "Export notes: CSV"\nlabels: [data]\nstate: open\n---\n\n# #41 Export notes: CSV\n'
        )
    )
    readme = (demo.path / "README.md").read_text()
    assert (
        "| 4 | [Crash on empty title](004-crash-on-empty-title.md) | bug | open | – |\n| 41 | [Export notes: CSV](041-export-notes-csv.md) | data | open | – |\n"
        in readme
    )


def test_a_read_only_project_refuses_to_write(tmp_path):
    project = Project(ProjectConfig("demo", write_demo(tmp_path / "demo"), read_only=True))
    with pytest.raises(ReadOnlyError):
        project.update(4, lambda issue: issue.file.text + "x")
    with pytest.raises(ReadOnlyError):
        project.create("x", [], "")


def test_a_missing_directory_is_an_empty_project(tmp_path):
    project = Project(ProjectConfig("gone", tmp_path / "gone"))
    assert not project.exists
    assert not project.writable
    assert project.snapshot().issues == ()
