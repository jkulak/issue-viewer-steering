import pytest

from issue_viewer.issuefile import (
    EditError,
    add_comment,
    delete_comment,
    edit_comment,
    new_issue_text,
    parse,
    set_description,
    set_front,
    set_task,
    set_title,
    task_counts,
)

ISSUE = """\
---
title: "Search notes"
labels: [enhancement, ui]
state: open
---

# #3 Search notes

**Depends on:** #2 · **Blocks:** –

- [x] Typing filters the list
- [ ] Matches are highlighted

## Comments

### 2026-09-20 10:15

First comment.

### 2026-09-21 16:40 · by hand

Second comment.

- [ ] A task in a comment
"""


def test_parse_finds_the_heading_the_description_and_the_comments():
    parsed = parse(ISSUE)
    assert parsed.heading_number == 3
    assert parsed.heading_title == "Search notes"
    assert parsed.description_text.startswith("**Depends on:** #2")
    assert parsed.description_text.endswith("- [ ] Matches are highlighted")
    assert [c.stamp for c in parsed.comments] == ["2026-09-20 10:15", "2026-09-21 16:40"]
    assert parsed.comments[1].note == "· by hand"
    assert parsed.comment_text(parsed.comments[0]) == "First comment."
    assert parsed.comments[0].when.hour == 10


def test_a_first_heading_that_is_not_the_title_stays_in_the_description():
    parsed = parse('---\ntitle: "Real title"\n---\n\n# Background\n\nText\n')
    assert parsed.heading is None
    assert parsed.description_text == "# Background\n\nText"


def test_a_heading_that_repeats_the_title_without_a_number_is_the_title_heading():
    parsed = parse('---\ntitle: "Real title"\n---\n\n# Real  title\n\nText\n')
    assert parsed.heading is not None
    assert parsed.heading_number is None
    assert parsed.description_text == "Text"


def test_a_comments_heading_followed_by_prose_belongs_to_the_description():
    parsed = parse("---\ntitle: x\n---\n\n## Comments\n\nHow comments should work.\n")
    assert parsed.comments_heading is None
    assert parsed.description_text == "## Comments\n\nHow comments should work."


def test_a_comments_section_inside_a_code_block_is_not_the_comments_section():
    text = "---\ntitle: x\n---\n\n```\n## Comments\n\n### 2026-01-01 00:00\n\nno\n```\n"
    parsed = parse(text)
    assert parsed.comments_heading is None
    assert parsed.comments == ()


def test_setting_the_title_changes_the_front_matter_and_the_heading_and_nothing_else():
    result = set_title(parse(ISSUE), "Search notes quickly")
    assert result == ISSUE.replace(
        'title: "Search notes"', 'title: "Search notes quickly"'
    ).replace("# #3 Search notes\n", "# #3 Search notes quickly\n")


def test_setting_the_state_changes_one_line():
    result = set_front(parse(ISSUE), "state", "done")
    assert result == ISSUE.replace("state: open", "state: done")


def test_setting_the_description_keeps_the_front_matter_the_heading_and_the_comments():
    result = set_description(parse(ISSUE), "\r\nNew text.\r\n\r\n- [ ] one\r\n\n\n")
    assert result.startswith(ISSUE[: ISSUE.index("**Depends")])
    assert "New text.\n\n- [ ] one\n\n## Comments\n" in result
    assert result.endswith(ISSUE[ISSUE.index("## Comments") :])
    assert parse(result).description_text == "New text.\n\n- [ ] one"


def test_an_empty_description_leaves_one_blank_line_before_the_comments():
    result = set_description(parse(ISSUE), "   ")
    assert "# #3 Search notes\n\n## Comments\n" in result
    assert parse(result).description_text == ""


def test_a_description_that_would_start_the_comments_section_is_refused():
    with pytest.raises(EditError):
        set_description(parse("---\ntitle: x\n---\n"), "## Comments\n\n### 2026-01-01 00:00\n\nx")


def test_adding_the_first_comment_creates_the_comments_section():
    text = "---\ntitle: x\n---\n\n# #1 x\n\nBody\n\n\n"
    result = add_comment(parse(text), "2026-09-24 14:05", "Looks good.\r\n")
    assert (
        result
        == "---\ntitle: x\n---\n\n# #1 x\n\nBody\n\n## Comments\n\n### 2026-09-24 14:05\n\nLooks good.\n"
    )


def test_adding_a_comment_appends_it_after_the_last_one():
    result = add_comment(parse(ISSUE), "2026-09-24 14:05", "Third.")
    assert result == ISSUE + "\n### 2026-09-24 14:05\n\nThird.\n"
    assert len(parse(result).comments) == 3


def test_a_line_in_a_comment_that_looks_like_a_comment_heading_is_escaped():
    result = add_comment(parse(ISSUE), "2026-09-24 14:05", "Log:\n### 2026-09-01 10:00\nstarted")
    parsed = parse(result)
    assert len(parsed.comments) == 3
    assert parsed.comment_text(parsed.comments[2]) == "Log:\n\\### 2026-09-01 10:00\nstarted"


def test_an_empty_comment_is_refused():
    with pytest.raises(EditError):
        add_comment(parse(ISSUE), "2026-09-24 14:05", " \n ")


def test_editing_a_comment_replaces_only_its_text():
    result = edit_comment(parse(ISSUE), 0, "Changed.")
    assert result == ISSUE.replace("First comment.", "Changed.")


def test_editing_the_last_comment_keeps_the_file_ending_in_one_newline():
    result = edit_comment(parse(ISSUE), 1, "Changed.")
    assert result.endswith("### 2026-09-21 16:40 · by hand\n\nChanged.\n")


def test_deleting_a_comment_keeps_the_others():
    result = delete_comment(parse(ISSUE), 0)
    parsed = parse(result)
    assert [c.stamp for c in parsed.comments] == ["2026-09-21 16:40"]
    assert result.startswith(ISSUE[: ISSUE.index("### 2026-09-20")])


def test_deleting_the_last_comment_trims_the_blank_lines_it_leaves():
    result = delete_comment(parse(ISSUE), 1)
    assert result.endswith("### 2026-09-20 10:15\n\nFirst comment.\n")


def test_deleting_the_only_comment_removes_the_section():
    once = delete_comment(parse(ISSUE), 1)
    result = delete_comment(parse(once), 0)
    assert result.endswith("- [ ] Matches are highlighted\n")
    assert "## Comments" not in result


def test_ticking_a_checkbox_changes_one_character():
    parsed = parse(ISSUE)
    lines = parsed.description_text.split("\n")
    line = lines.index("- [ ] Matches are highlighted")
    result = set_task(parsed, "description", line, True)
    assert result == ISSUE.replace("- [ ] Matches", "- [x] Matches")
    assert set_task(parse(result), "description", line, False) == ISSUE


def test_ticking_a_checkbox_in_a_comment():
    parsed = parse(ISSUE)
    result = set_task(parsed, "1", 2, True)
    assert result == ISSUE.replace("- [ ] A task in a comment", "- [x] A task in a comment")


def test_a_checkbox_that_is_no_longer_on_its_line_is_refused():
    with pytest.raises(EditError):
        set_task(parse(ISSUE), "description", 0, True)
    with pytest.raises(EditError):
        set_task(parse(ISSUE), "7", 0, True)


def test_ticking_a_box_that_is_already_ticked_changes_nothing():
    assert set_task(parse(ISSUE), "description", 2, True) == ISSUE


def test_task_counts_skip_code_blocks():
    text = "- [x] one\n- [ ] two\n\n```\n- [ ] not a task\n```\n  * [X] three\n1. [ ] four\n"
    assert task_counts(text) == (2, 4)


def test_editing_a_file_without_front_matter_adds_one():
    result = set_front(parse("# Plain\n\nText\n"), "state", "done")
    assert result == "---\nstate: done\n---\n\n# Plain\n\nText\n"


def test_front_matter_that_cannot_be_read_is_not_edited():
    with pytest.raises(EditError):
        set_front(parse("---\ntitle: [broken\n---\n"), "state", "done")


def test_a_new_issue_has_front_matter_a_numbered_heading_and_the_body():
    text = new_issue_text(52, "Export: CSV", ["bug", "needs design"], "\nBody\n\n")
    assert text == (
        '---\ntitle: "Export: CSV"\nlabels: [bug, needs design]\nstate: open\n---\n\n'
        "# #52 Export: CSV\n\nBody\n"
    )
    parsed = parse(text)
    assert parsed.front.data == {
        "title": "Export: CSV",
        "labels": ["bug", "needs design"],
        "state": "open",
    }
    assert parsed.description_text == "Body"
