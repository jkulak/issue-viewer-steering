from issue_viewer.render import Context, Renderer

render = Renderer().render
ISSUES = {2: ("Store notes", False), 4: ("Crash on save", True)}


def test_task_list_items_become_checkboxes_that_know_their_line():
    html = render("Intro\n\n- [x] done\n- [ ] todo\n", Context("demo", {}, "description", True))
    assert '<ul class="contains-task-list">' in html
    assert (
        '<li class="task-list-item"><input type="checkbox" class="task-list-item-checkbox" checked data-part="description" data-line="2"> done</li>'
        in html
    )
    assert 'data-line="3"> todo' in html


def test_checkboxes_are_disabled_when_the_page_cannot_write():
    html = render("- [ ] todo\n", Context("demo", {}, "description", False))
    assert "disabled" in html
    assert "data-line" not in html


def test_brackets_that_are_not_a_task_stay_text():
    html = render("- [link] text\n- [ ]\n", Context("demo"))
    assert "[link] text" in html


def test_issue_numbers_link_to_issues_that_exist_and_only_those():
    html = render(
        "See #2, #4 and #99. Also [#2](https://example.com/x#2).", Context("demo", ISSUES)
    )
    assert '<a href="/demo/issues/2" class="issue-link" title="Store notes (closed)">#2</a>' in html
    assert '<a href="/demo/issues/4" class="issue-link" title="Crash on save (open)">#4</a>' in html
    assert "#99" in html and "/issues/99" not in html
    assert html.count("/demo/issues/2") == 1


def test_references_inside_code_are_not_linked():
    html = render("`#2` and\n\n```\n#4\n```\n", Context("demo", ISSUES))
    assert "issue-link" not in html


def test_links_to_issue_files_open_the_issue_page():
    html = render(
        "[storage](002-store-notes.md) and [readme](../README.md)", Context("demo", ISSUES)
    )
    assert 'href="/demo/issues/2"' in html
    assert 'href="../README.md"' in html


def test_raw_html_is_escaped_but_harmless_tags_survive():
    html = render(
        "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n"
        "<details>\n<summary>More</summary>\n\nHidden\n</details>\n\nPress <kbd>Ctrl</kbd>",
        Context("demo"),
    )
    assert "<script>" not in html
    assert "&lt;script&gt;" in html
    assert "<img" not in html
    assert "<details>" in html and "<summary>More</summary>" in html
    assert "<kbd>Ctrl</kbd>" in html


def test_tags_with_attributes_are_escaped_even_when_the_tag_is_allowed():
    html = render('<details open onclick="x()">text</details>', Context("demo"))
    assert "<details open" not in html
    assert "&lt;details open" in html


def test_html_comments_are_hidden():
    html = render("Before\n\n<!-- a note to self -->\n\nAfter <!-- inline --> end", Context("demo"))
    assert "note to self" not in html
    assert "inline" not in html
    assert "After  end" in html


def test_script_links_are_not_rendered_as_links():
    html = render("[click](javascript:alert(1)) [data](data:text/html,x)", Context("demo"))
    assert 'href="javascript' not in html
    assert 'href="data:text' not in html


def test_external_links_do_not_leak_the_page():
    html = render("<https://example.com/a>", Context("demo"))
    assert 'rel="nofollow noopener noreferrer"' in html


def test_tables_and_strikethrough_render():
    html = render("| a | b |\n|---|---|\n| 1 | 2 |\n\n~~gone~~", Context("demo"))
    assert "<table>" in html and "<td>2</td>" in html
    assert "<s>gone</s>" in html
