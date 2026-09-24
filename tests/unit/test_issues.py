from pathlib import Path

from issue_viewer.files import FileText
from issue_viewer.issues import build, classify, normalise_labels, relationships, slugify


def issue_from(text: str, name: str = "007-some-thing.md"):
    return build(Path("/issues") / name, 7, FileText(text, "\n", "v1"), 1_700_000_000)


def test_open_is_open_and_the_rest_are_the_two_kinds_of_closed():
    assert classify("open") == (True, None)
    assert classify("Done") == (False, "completed")
    assert classify("closed") == (False, "completed")
    assert classify("wontfix") == (False, "not_planned")
    assert classify("not-planned") == (False, "not_planned")
    assert classify("in-progress") == (True, None)


def test_labels_can_be_a_list_a_comma_separated_string_or_missing():
    assert normalise_labels(["bug", " ui ", "", None, "bug"]) == ("bug", "ui")
    assert normalise_labels("bug, ui") == ("bug", "ui")
    assert normalise_labels(None) == ()
    assert normalise_labels(2026) == ("2026",)


def test_relationships_come_from_the_depends_on_and_blocks_line():
    assert relationships("Intro\n\n**Depends on:** #2, #3 · **Blocks:** #9\n") == ((2, 3), (9,))
    assert relationships("**Depends on:** – · **Blocks:** –") == ((), ())
    assert relationships("**Blocked by:** #4") == ((4,), ())
    assert relationships("```\n**Depends on:** #1\n```\n") == ((), ())


def test_a_slug_is_short_ascii_kebab_case():
    assert slugify("Zażółć gęślą jaźń: łódź!") == "zazolc-gesla-jazn-lodz"
    assert slugify("   ") == "issue"
    long = slugify("word " * 40)
    assert len(long) <= 60
    assert not long.endswith("-")


def test_an_issue_takes_title_labels_and_state_from_the_front_matter():
    issue = issue_from(
        '---\ntitle: "Export"\nlabels: [a, b]\nstate: done\n---\n\n# #7 Export\n\n'
        "**Depends on:** #1 · **Blocks:** –\n\n- [x] one\n- [ ] two\n"
    )
    assert (issue.title, issue.labels, issue.state) == ("Export", ("a", "b"), "done")
    assert not issue.is_open
    assert issue.reason == "completed"
    assert issue.depends_on == (1,)
    assert (issue.tasks_done, issue.tasks_total) == (1, 2)
    assert issue.editable
    assert not issue.state_is_custom


def test_a_title_falls_back_to_the_heading_and_then_to_the_file_name():
    assert issue_from("# Plain heading\n\nText\n").title == "Plain heading"
    assert issue_from("Just text\n", "007-some-thing.md").title == "Some thing"


def test_a_missing_state_means_open():
    issue = issue_from("---\ntitle: x\n---\n")
    assert issue.state == "open"
    assert issue.is_open


def test_an_unreadable_front_matter_is_a_problem_and_blocks_editing():
    issue = issue_from("---\ntitle: [broken\n---\n\n# #7 Broken\n")
    assert issue.problems
    assert not issue.editable
    assert issue.title == "Broken"
