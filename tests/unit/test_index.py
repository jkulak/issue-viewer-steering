from issue_viewer import index

README = """\
# Issues

Intro text stays.

| # | Title | Labels | State | Depends on |
|---|---|---|---|---|
| 1 | [Skeleton](001-skeleton.md) | enhancement, infra | done | – |
| 2 | [Short title by hand](002-storage.md) | data | open | #1 |

Order of work: 1 → 2.
"""


def test_updating_a_row_changes_only_the_given_cells_of_that_row():
    result = index.updated_row(README, 2, state="done")
    assert result == README.replace(
        "| 2 | [Short title by hand](002-storage.md) | data | open | #1 |",
        "| 2 | [Short title by hand](002-storage.md) | data | done | #1 |",
    )


def test_updating_the_title_keeps_the_link_target():
    result = index.updated_row(README, 2, title="Store | notes [fast]", filename="ignored.md")
    assert "| 2 | [Store \\| notes \\[fast\\]](002-storage.md) | data | open | #1 |" in result


def test_labels_and_dependencies_are_written_like_the_rest_of_the_table():
    result = index.updated_row(README, 1, labels=[], depends_on=[2, 3])
    assert "| 1 | [Skeleton](001-skeleton.md) | – | done | #2, #3 |" in result


def test_nothing_to_change_gives_none():
    assert index.updated_row(README, 2, state="open") is None
    assert index.updated_row(README, 9, state="done") is None


def test_a_new_issue_is_appended_after_the_last_row():
    result = index.appended_row(README, 3, "Search", "003-search.md", ["ui"], "open", [2])
    assert result == README.replace(
        "| 2 | [Short title by hand](002-storage.md) | data | open | #1 |\n",
        "| 2 | [Short title by hand](002-storage.md) | data | open | #1 |\n"
        "| 3 | [Search](003-search.md) | ui | open | #2 |\n",
    )


def test_appending_an_issue_that_already_has_a_row_updates_it_instead():
    result = index.appended_row(README, 2, "Storage", "002-storage.md", ["data"], "done", [1])
    assert result.count("| 2 |") == 1
    assert "| 2 | [Storage](002-storage.md) | data | done | #1 |" in result


def test_a_readme_without_an_index_table_is_left_alone():
    text = "# Notes\n\n| a | b |\n|---|---|\n| 1 | 2 |\n"
    assert index.updated_row(text, 1, state="done") is None
    assert index.appended_row(text, 3, "x", "003-x.md", [], "open", []) is None


def test_rewrite_writes_the_change_and_ignores_a_missing_readme(tmp_path):
    index.rewrite(tmp_path, lambda text: text + "never")
    assert not (tmp_path / "README.md").exists()
    (tmp_path / "README.md").write_text(README)
    index.rewrite(tmp_path, lambda text: index.updated_row(text, 2, state="done"))
    assert "| data | done | #1 |" in (tmp_path / "README.md").read_text()
