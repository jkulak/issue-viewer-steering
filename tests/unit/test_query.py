from dataclasses import replace
from datetime import UTC, datetime
from pathlib import Path

from issue_viewer import query
from issue_viewer.files import FileText
from issue_viewer.issues import build


def make(number, title, labels, state="open", updated=0, body="", comments=0):
    comment_text = "".join(f"\n### 2026-09-2{i} 10:00\n\nc{i}\n" for i in range(comments))
    section = f"\n## Comments\n{comment_text}" if comments else ""
    text = f'---\ntitle: "{title}"\nlabels: {labels}\nstate: {state}\n---\n\n{body}\n{section}'
    issue = build(Path(f"/x/{number:03d}-x.md"), number, FileText(text, "\n", "v"), 0)
    return replace(issue, updated=datetime.fromtimestamp(updated, UTC))


ISSUES = [
    make(1, "Skeleton", "[infra]", "done", updated=50),
    make(
        2, "Search notes", "[ui, enhancement]", updated=10, body="Matches highlighted", comments=2
    ),
    make(3, "Crash on save", "[bug]", updated=30, comments=1),
    make(4, "Dark theme", "[ui, good first]", updated=20),
    make(5, "Old idea", "[]", "wontfix", updated=40),
]


def numbers(text, ignore_state=False):
    parsed = query.parse(text)
    found = [i for i in ISSUES if query.matches(parsed, i, ignore_state=ignore_state)]
    return [issue.number for issue in query.ordered(found, parsed.sort)]


def test_the_default_query_shows_open_issues_newest_first():
    assert numbers(query.DEFAULT) == [4, 3, 2]
    assert numbers("is:closed") == [5, 1]
    assert numbers("") == [5, 4, 3, 2, 1]


def test_repeated_labels_must_all_match_and_a_comma_means_either():
    assert numbers("label:ui") == [4, 2]
    assert numbers("label:ui label:enhancement") == [2]
    assert numbers("label:bug,infra") == [3, 1]
    assert numbers('label:"good first"') == [4]
    assert numbers("label:UI") == [4, 2]


def test_a_label_can_be_excluded_and_no_label_finds_the_bare_ones():
    assert numbers("is:open -label:ui") == [3]
    assert numbers("no:label") == [5]


def test_free_text_matches_the_title_and_the_body():
    assert numbers("search") == [2]
    assert numbers("highlighted") == [2]
    assert numbers('"crash on"') == [3]
    assert numbers("#3") == [3]


def test_raw_states_and_reasons():
    assert numbers("state:wontfix") == [5]
    assert numbers("reason:not-planned") == [5]
    assert numbers("reason:completed") == [1]


def test_the_sort_orders():
    assert numbers("sort:created-asc") == [1, 2, 3, 4, 5]
    assert numbers("sort:updated-desc") == [1, 5, 3, 4, 2]
    assert numbers("sort:updated-asc") == [2, 4, 3, 5, 1]
    assert numbers("sort:comments-desc") == [2, 3, 5, 4, 1]
    assert numbers("sort:comments-asc") == [5, 4, 1, 3, 2]


def test_switching_the_state_replaces_the_state_qualifier():
    assert query.with_state("is:issue is:open label:ui", "closed") == "is:issue is:closed label:ui"
    assert query.with_state("label:ui", "open") == "is:open label:ui"


def test_adding_a_label_twice_takes_it_out_again_and_quotes_spaces():
    once = query.with_label("is:open", "good first")
    assert once == 'is:open label:"good first"'
    assert query.with_label(once, "good first") == "is:open"
    assert query.with_label("is:open no:label", "bug") == "is:open label:bug"
    assert query.with_no_label('is:open label:"good first"') == "is:open no:label"


def test_the_default_sort_is_left_out_of_the_query():
    assert query.with_sort("is:open", "updated-desc") == "is:open sort:updated-desc"
    assert query.with_sort("is:open sort:updated-desc", "created-desc") == "is:open"


def test_an_unbalanced_quote_does_not_break_the_search():
    assert query.parse('label:"oops').labels == [['"oops']]
