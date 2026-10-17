use crate::support::{JSON, Viewer, capture, issue_path, read};

#[test]
fn creating_an_issue_writes_the_next_file_and_a_readme_row() {
    let viewer = Viewer::new();
    let fields = [
        ("title", "Export notes"),
        ("description", "Body"),
        ("labels", "data"),
        ("labels", "needs design"),
    ];
    let reply = viewer.post("/demo/issues", &fields);
    assert_eq!(reply.status, 303);
    assert_eq!(reply.location(), "/demo/issues/41");
    assert_eq!(
        read(&viewer.demo.join("041-export-notes.md")),
        "---\ntitle: \"Export notes\"\nlabels: [data, needs design]\nstate: open\n---\n\n# #41 Export notes\n\nBody\n"
    );
    assert!(read(&viewer.demo.join("README.md")).contains(
        "| 41 | [Export notes](041-export-notes.md) | data, needs design | open | – |\n\nOrder"
    ));
}

#[test]
fn an_issue_needs_a_title_and_keeps_what_was_typed() {
    let viewer = Viewer::new();
    let reply = viewer.post(
        "/demo/issues",
        &[
            ("title", "   "),
            ("description", "Kept text"),
            ("labels", "bug"),
        ],
    );
    assert_eq!(reply.status, 422);
    assert!(reply.text.contains("A title is required."));
    assert!(reply.text.contains("Kept text"));
    assert!(std::fs::read_dir(&viewer.demo).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("041-")
    }));
}

#[test]
fn editing_the_title_changes_the_front_matter_the_heading_and_the_readme_row() {
    let viewer = Viewer::new();
    assert_eq!(
        viewer
            .post(
                "/demo/issues/4/title",
                &[("title", "Saving  without a title crashes")]
            )
            .status,
        303
    );
    let text = read(&issue_path(&viewer.demo, 4));
    assert!(text.contains("title: Saving without a title crashes\n"));
    assert!(text.contains("# #4 Saving without a title crashes\n"));
    assert!(read(&viewer.demo.join("README.md")).contains(
        "| 4 | [Saving without a title crashes](004-crash-on-empty-title.md) | bug | open | – |"
    ));
}

#[test]
fn closing_as_completed_or_not_planned_and_reopening() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 4);
    assert_eq!(
        viewer
            .post("/demo/issues/4/comments", &[("action", "close")])
            .status,
        303
    );
    assert!(read(&path).contains("state: done\n"));
    viewer.post("/demo/issues/4/comments", &[("action", "reopen")]);
    assert!(read(&path).contains("state: open\n"));
    viewer.post(
        "/demo/issues/4/comments",
        &[("action", "close-not-planned")],
    );
    let text = read(&path);
    assert!(text.contains("state: not-planned\n"));
    assert!(!text.contains("## Comments"));
    assert!(
        viewer
            .get("/demo/issues/4")
            .text
            .contains("State--not-planned")
    );
    assert!(read(&viewer.demo.join("README.md")).contains(
        "| 4 | [Crash on empty title](004-crash-on-empty-title.md) | bug | not-planned | – |"
    ));
}

#[test]
fn commenting_appends_a_dated_comment_to_the_file() {
    let viewer = Viewer::new();
    let reply = viewer.post(
        "/demo/issues/4/comments",
        &[("text", "Reproduced on the laptop."), ("action", "comment")],
    );
    assert_eq!(reply.location(), "/demo/issues/4#comment-0");
    let pattern = regex::Regex::new(
        r"3\. Press save\n\n## Comments\n\n### \d{4}-\d\d-\d\d \d\d:\d\d\n\nReproduced on the laptop\.\n$",
    )
    .unwrap();
    assert!(pattern.is_match(&read(&issue_path(&viewer.demo, 4))));
}

#[test]
fn an_empty_comment_changes_nothing() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 4);
    let before = read(&path);
    let reply = viewer.post(
        "/demo/issues/4/comments",
        &[("text", "   "), ("action", "comment")],
    );
    assert_eq!(reply.status, 422);
    assert!(reply.text.contains("write a comment first"));
    assert_eq!(read(&path), before);
}

#[test]
fn close_with_comment_does_both() {
    let viewer = Viewer::new();
    viewer.post(
        "/demo/issues/4/comments",
        &[("text", "Fixed by the new save flow."), ("action", "close")],
    );
    let text = read(&issue_path(&viewer.demo, 4));
    assert!(text.contains("state: done\n"));
    assert!(text.ends_with("Fixed by the new save flow.\n"));
}

#[test]
fn editing_and_then_deleting_a_comment() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 3);
    let version = viewer.version_of(3);
    let reply = viewer.post(
        "/demo/issues/3/comments/0",
        &[("text", "Highlighting first."), ("version", &version)],
    );
    assert_eq!(reply.location(), "/demo/issues/3#comment-0");
    let text = read(&path);
    assert!(text.contains("Highlighting first."));
    assert!(!text.contains("Highlighting can wait"));
    let version = viewer.version_of(3);
    viewer.post("/demo/issues/3/comments/1/delete", &[("version", &version)]);
    let text = read(&path);
    assert!(!text.contains("Measured it"));
    assert!(text.ends_with("### 2026-09-20 10:15\n\nHighlighting first.\n"));
}

#[test]
fn editing_the_description_keeps_the_heading_and_the_comments() {
    let viewer = Viewer::new();
    let version = viewer.version_of(3);
    let reply = viewer.post(
        "/demo/issues/3/description",
        &[
            ("text", "Short now.\r\n\r\n- [ ] one"),
            ("version", &version),
        ],
    );
    assert_eq!(reply.status, 303);
    let text = read(&issue_path(&viewer.demo, 3));
    assert!(text.contains(
        "# #3 Search notes by title and body\n\nShort now.\n\n- [ ] one\n\n## Comments\n"
    ));
    assert!(text.ends_with("- [ ] Try it with 20000 notes\n"));
    assert!(
        read(&viewer.demo.join("README.md"))
            .contains("| 3 | [Search notes](003-search-notes.md) | enhancement, ui | open | – |")
    );
}

#[test]
fn a_description_edit_on_an_older_version_is_refused_and_the_text_kept() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 4);
    let version = viewer.version_of(4);
    std::fs::write(&path, read(&path) + "\nChanged elsewhere.\n").unwrap();
    let reply = viewer.post(
        "/demo/issues/4/description",
        &[("text", "My new description"), ("version", &version)],
    );
    assert_eq!(reply.status, 409);
    assert!(reply.text.contains("changed on disk"));
    assert!(reply.text.contains("My new description"));
    let text = read(&path);
    assert!(text.contains("Changed elsewhere."));
    assert!(!text.contains("My new description"));
}

#[test]
fn labels_are_set_new_ones_are_kept_and_existing_ones_matched_ignoring_case() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 7);
    viewer.post(
        "/demo/issues/7/labels",
        &[
            ("labels", "UI"),
            ("labels", "needs design"),
            ("labels", " "),
        ],
    );
    assert!(read(&path).contains("labels: [ui, needs design]\n"));
    viewer.post("/demo/issues/7/labels", &[]);
    assert!(read(&path).contains("labels: []\n"));
}

#[test]
fn a_label_with_a_comma_is_refused() {
    let viewer = Viewer::new();
    let reply = viewer.post("/demo/issues/7/labels", &[("labels", "a,b")]);
    assert_eq!(reply.status, 422);
    assert!(reply.text.contains("cannot contain a comma"));
}

#[test]
fn ticking_checkboxes_in_the_description_and_in_a_comment() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/issues/3").text;
    let version = capture(r#"data-version="([0-9a-f]+)""#, &page);
    let line = capture(
        r#"data-part="description" data-line="(\d+)"> Matches are highlighted"#,
        &page,
    );
    let token = viewer.token();
    let headers = [JSON, ("x-csrf-token", token.as_str())];
    let fields = [
        ("part", "description"),
        ("line", line.as_str()),
        ("checked", "1"),
        ("version", version.as_str()),
    ];
    let reply = viewer.post_raw("/demo/issues/3/tasks", &fields, &headers);
    assert_eq!(reply.status, 200);
    let body = reply.json();
    assert_eq!(
        (body["done"].as_u64(), body["total"].as_u64()),
        (Some(2), Some(3))
    );
    assert_ne!(body["version"].as_str(), Some(version.as_str()));
    assert!(read(&issue_path(&viewer.demo, 3)).contains("- [x] Matches are highlighted"));

    assert_eq!(
        viewer
            .post_raw("/demo/issues/3/tasks", &fields, &headers)
            .status,
        409
    );

    let comment_line = capture(r#"data-part="1" data-line="(\d+)"> Try it"#, &page);
    let new_version = body["version"].as_str().unwrap();
    let fields = [
        ("part", "1"),
        ("line", comment_line.as_str()),
        ("checked", "1"),
        ("version", new_version),
    ];
    assert_eq!(
        viewer
            .post_raw("/demo/issues/3/tasks", &fields, &headers)
            .status,
        200
    );
    assert!(read(&issue_path(&viewer.demo, 3)).contains("- [x] Try it with 20000 notes"));
}

#[test]
fn the_preview_renders_markdown_with_issue_links() {
    let viewer = Viewer::new();
    let token = viewer.token();
    let reply = viewer.post_raw(
        "/demo/preview",
        &[("text", "**bold** and #3")],
        &[JSON, ("x-csrf-token", &token)],
    );
    let html = reply.json()["html"].as_str().unwrap().to_string();
    assert!(html.contains("<strong>bold</strong>"));
    assert!(html.contains(r#"href="/demo/issues/3""#));
}

#[test]
fn a_write_keeps_crlf_line_endings_and_the_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 4);
    std::fs::write(&path, read(&path).replace('\n', "\r\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    assert_eq!(
        viewer
            .post("/demo/issues/4/comments", &[("action", "close")])
            .status,
        303
    );
    let text = read(&path);
    assert!(text.contains("state: done\r\n"));
    assert!(!text.replace("\r\n", "").contains('\n'));
    assert_eq!(
        std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
}
