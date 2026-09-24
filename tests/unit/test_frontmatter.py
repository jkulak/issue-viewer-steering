from issue_viewer import frontmatter


def test_front_matter_is_read_from_between_the_two_dashed_lines():
    text = "---\ntitle: Hello\nlabels: [a, b]\nstate: open\n---\n\nBody\n"
    front = frontmatter.split(text)
    assert front.present
    assert front.data == {"title": "Hello", "labels": ["a", "b"], "state": "open"}
    assert text[front.body_start :] == "\nBody\n"
    assert front.raw(text) == "title: Hello\nlabels: [a, b]\nstate: open\n"


def test_a_file_without_front_matter_has_no_data_and_no_error():
    front = frontmatter.split("# Title\n\nBody\n")
    assert not front.present
    assert front.data == {}
    assert front.error is None
    assert front.body_start == 0


def test_an_opening_line_without_a_closing_line_is_a_thematic_break():
    assert not frontmatter.split("---\ntitle: never closed\n").present


def test_broken_yaml_is_reported_with_the_line_it_is_on():
    front = frontmatter.split("---\ntitle: ok\nlabels: [bug\n---\n")
    assert front.present
    assert front.data == {}
    assert "line" in front.error


def test_setting_a_key_replaces_only_its_line():
    raw = "title: Hello  # keep this comment\nlabels: [a]\nstate: open\nowner: someone\n"
    assert frontmatter.set_key(raw, "state", "done") == (
        "title: Hello  # keep this comment\nlabels: [a]\nstate: done\nowner: someone\n"
    )


def test_setting_a_key_replaces_a_block_list_with_all_its_items():
    raw = "title: Hello\nlabels:\n  - a\n\n  - b\nstate: open\n"
    assert frontmatter.set_key(raw, "labels", "[c]") == "title: Hello\nlabels: [c]\nstate: open\n"


def test_setting_a_key_replaces_a_list_written_at_the_same_indent():
    raw = "labels:\n- a\n- b\nstate: open\n"
    assert frontmatter.set_key(raw, "labels", "[]") == "labels: []\nstate: open\n"


def test_a_missing_key_is_added_at_the_end():
    assert frontmatter.set_key("title: Hello", "state", "open") == "title: Hello\nstate: open\n"


def test_render_scalar_keeps_a_plain_title_plain_and_quotes_what_yaml_would_misread():
    assert frontmatter.render_scalar("Store notes in SQLite") == "Store notes in SQLite"
    assert frontmatter.render_scalar("Search: by title") == '"Search: by title"'
    assert frontmatter.render_scalar("yes") == '"yes"'
    assert frontmatter.render_scalar("2026") == '"2026"'
    assert frontmatter.render_scalar("# not a comment") == '"# not a comment"'


def test_render_scalar_keeps_the_quote_style_the_file_already_uses():
    assert frontmatter.render_scalar("Plain words", "double") == '"Plain words"'
    assert frontmatter.render_scalar("It's here", "single") == "'It''s here'"
    assert frontmatter.style_of('title: "x"\nstate: open\n', "title") == "double"
    assert frontmatter.style_of('title: "x"\nstate: open\n', "state") == "plain"


def test_rendered_values_read_back_as_the_same_value():
    for value in ["a: b", 'say "hi"', "back\\slash", "zażółć gęślą jaźń", "- dash", "[x]"]:
        for style in ("plain", "double", "single"):
            rendered = frontmatter.render_scalar(value, style)
            assert frontmatter.load(f"title: {rendered}\n") == ({"title": value}, None)


def test_render_list_quotes_only_the_items_that_need_it():
    assert frontmatter.render_list(["bug", "good first issue"]) == "[bug, good first issue]"
    assert frontmatter.render_list(["a,b", "yes", "ok"]) == '["a,b", "yes", ok]'
    assert frontmatter.render_list([]) == "[]"
    labels = ["x: y", "[z]", "true", "wiki-api"]
    assert frontmatter.load(f"labels: {frontmatter.render_list(labels)}\n") == (
        {"labels": labels},
        None,
    )
