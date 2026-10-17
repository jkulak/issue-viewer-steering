use crate::support::{JSON, Viewer, issue_path, row_numbers};

#[test]
fn the_start_page_lists_every_project_with_its_counts() {
    let viewer = Viewer::new();
    let page = viewer.get("/");
    assert_eq!(page.status, 200);
    assert!(page.text.contains(r#"href="/demo/issues""#));
    assert!(page.text.contains(r#"href="/archive/issues""#));
    assert!(page.text.contains("27 open"));
    assert!(page.text.contains("13 closed"));
    assert!(page.text.contains("Read-only"));
}

#[test]
fn a_project_address_leads_to_its_issues() {
    let viewer = Viewer::new();
    assert_eq!(viewer.get("/demo").location(), "/demo/issues");
}

#[test]
fn an_address_with_a_trailing_slash_leads_to_the_one_without() {
    let viewer = Viewer::new();
    let reply = viewer.get("/demo/issues/?q=is%3Aclosed");
    assert_eq!(reply.status, 307);
    assert_eq!(reply.location(), "/demo/issues?q=is%3Aclosed");
}

#[test]
fn the_issue_list_shows_25_open_issues_newest_first() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/issues").text;
    let numbers = row_numbers(&page);
    assert_eq!(numbers.len(), 25);
    assert_eq!(numbers[..4], [40, 38, 37, 35]);
    assert!(page.contains(r#"Open <span class="Counter">27</span>"#));
    assert!(page.contains(r#"Closed <span class="Counter">13</span>"#));
    let second = viewer.get_query("/demo/issues", &[("q", "is:issue is:open"), ("page", "2")]);
    assert_eq!(row_numbers(&second.text), [4, 3]);
}

#[test]
fn the_list_filters_by_label_and_by_text() {
    let viewer = Viewer::new();
    let rows = |q: &str| row_numbers(&viewer.get_query("/demo/issues", &[("q", q)]).text);
    assert_eq!(rows("is:open label:backlog"), [8, 5]);
    assert_eq!(rows("sqlite"), [6, 3, 2]);
    assert!(
        viewer
            .get_query("/demo/issues", &[("q", "nothing-matches-this")])
            .text
            .contains("No results")
    );
}

#[test]
fn a_page_number_that_is_not_a_number_is_a_bad_request() {
    let viewer = Viewer::new();
    assert_eq!(viewer.get("/demo/issues?page=two").status, 400);
}

fn row(page: &str, number: u64) -> &str {
    let start = page.find(&format!(r#"id="issue-{number}""#)).unwrap();
    let rest = &page[start..];
    &rest[..rest.len().min(3000)]
}

#[test]
fn a_row_shows_labels_task_progress_comments_and_what_blocks_it() {
    let viewer = Viewer::new();
    let page = viewer
        .get_query("/demo/issues", &[("q", "is:open label:ui")])
        .text;
    let five = row(&page, 5);
    assert!(five.contains("Blocked by") && five.contains(r#"href="/demo/issues/3""#));
    let three = row(&page, 3);
    assert!(three.contains("1 of 3 tasks"));
    assert!(three.contains(r#"aria-label="2 comments""#));
}

#[test]
fn the_issue_page_shows_the_description_comments_and_relationships() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/issues/3").text;
    assert!(page.contains(r#"<bdi class="js-issue-title">Search notes by title and body</bdi>"#));
    assert!(page.contains(r##"<span class="issue-number">#3</span>"##));
    assert!(page.contains("State--open"));
    assert_eq!(
        page.matches(r#"class="timeline-item" id="comment-"#)
            .count(),
        2
    );
    assert!(page.contains("<table>"));
    assert!(page.contains("Measured it: 3 ms"));
    assert!(page.contains("Blocked by") && page.contains("Blocking"));
    assert!(page.contains(r#"href="/demo/issues/2""#));
    assert!(page.contains("1 of 3 tasks"));
    // The `# #3 Title` line is the page title, not part of the body.
    assert!(!page.contains("<h1>"));
}

/// The description as rendered, without the raw Markdown kept in the (hidden) editor.
fn rendered_description(page: &str) -> &str {
    let start = page.find(r#"js-comment-body""#).unwrap();
    let end = start
        + page[start..]
            .find(r#"<form class="timeline-comment-edit"#)
            .unwrap();
    &page[start..end]
}

#[test]
fn markup_in_an_issue_is_rendered_safely() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/issues/8").text;
    let body = rendered_description(&page);
    assert!(!body.contains("whoever picks this up"));
    assert!(!page.contains("<script>"));
    assert!(body.contains("&lt;script&gt;"));
    assert!(body.contains("<summary>Why not a server</summary>"));
    assert!(body.contains(r#"href="/demo/issues/4""#));
    assert!(!page.contains("/demo/issues/99"));
    assert!(body.contains("A code block, not a comment."));
    assert!(!page.contains(r#"id="comment-"#));
}

#[test]
fn a_closed_issue_says_how_it_was_closed() {
    let viewer = Viewer::new();
    assert!(viewer.get("/demo/issues/1").text.contains("State--done"));
    let page = viewer.get("/demo/issues/6").text;
    assert!(page.contains("State--not-planned"));
    assert!(page.contains("as not planned"));
}

#[test]
fn a_custom_state_is_shown_as_it_is_written() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/issues/10").text;
    assert!(page.contains("State--open"));
    assert!(page.contains(r#"<span class="Label">in-progress</span>"#));
}

#[test]
fn files_without_front_matter_or_with_broken_front_matter_still_show() {
    let viewer = Viewer::new();
    assert!(
        viewer
            .get("/demo/issues/11")
            .text
            .contains("Plain file without front matter")
    );
    let broken = viewer.get("/demo/issues/12");
    assert_eq!(broken.status, 200);
    assert!(broken.text.contains("The front matter could not be read"));
    assert!(!broken.text.contains("js-title-edit"));
    assert!(!broken.text.contains("js-comment-form"));
}

#[test]
fn an_unknown_issue_or_project_is_a_404_page() {
    let viewer = Viewer::new();
    let missing = viewer.get("/demo/issues/999");
    assert_eq!(missing.status, 404);
    assert!(missing.text.contains("There is no issue #999 in demo."));
    assert_eq!(viewer.get("/nope/issues").status, 404);
    let reply = viewer.get_with("/nope/-/version", &[JSON]);
    assert_eq!(
        reply.json(),
        serde_json::json!({ "error": "There is no project called “nope”." })
    );
}

#[test]
fn the_labels_page_counts_open_and_closed_issues_per_label() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/labels").text;
    assert!(
        regex::Regex::new(r"(?s)>backlog</a>.*?2 open.*?0 closed")
            .unwrap()
            .is_match(&page)
    );
    assert!(
        regex::Regex::new(r"(?s)>enhancement</a>.*?3 open.*?3 closed")
            .unwrap()
            .is_match(&page)
    );
}

#[test]
fn the_new_issue_form_starts_from_the_template() {
    let viewer = Viewer::new();
    assert!(
        viewer
            .get("/demo/issues/new")
            .text
            .contains("## Acceptance criteria")
    );
    std::fs::write(viewer.demo.join("TEMPLATE.md"), "## What we saw\n").unwrap();
    assert!(
        viewer
            .get("/demo/issues/new")
            .text
            .contains("## What we saw")
    );
}

#[test]
fn a_read_only_project_shows_no_write_controls() {
    let viewer = Viewer::new();
    assert!(!viewer.get("/archive/issues").text.contains("js-new-issue"));
    let page = viewer.get("/archive/issues/2").text;
    for control in [
        "js-comment-form",
        "js-title-edit",
        "js-label-picker",
        "js-edit-open",
    ] {
        assert!(!page.contains(control), "{control}");
    }
    assert!(page.contains("task-list-item-checkbox") && page.contains("disabled"));
    assert!(page.contains("This project is read-only"));
    assert_eq!(viewer.get("/archive/issues/new").status, 403);
}

#[test]
fn the_version_changes_when_a_file_changes_on_disk() {
    let viewer = Viewer::new();
    let project_version = || viewer.get("/demo/-/version").json()["version"].clone();
    let project_before = project_version();
    let issue_before = viewer.version_of(4);
    let path = issue_path(&viewer.demo, 4);
    let text = std::fs::read_to_string(&path).unwrap();
    std::fs::write(&path, text + "\nMore.\n").unwrap();
    assert_ne!(project_version(), project_before);
    assert_ne!(viewer.version_of(4), issue_before);
    assert_eq!(
        viewer.get("/demo/-/version?issue=999").json(),
        serde_json::json!({ "version": null })
    );
}

#[test]
fn pages_forbid_scripts_and_frames_from_anywhere_else() {
    let viewer = Viewer::new();
    let page = viewer.get("/demo/issues");
    let policy = page.header("content-security-policy");
    assert!(policy.contains("script-src 'self'"));
    assert!(policy.contains("frame-ancestors 'none'"));
    assert_eq!(page.header("cache-control"), "no-store");
    assert_eq!(page.header("x-content-type-options"), "nosniff");
}

#[test]
fn pages_load_nothing_from_outside_the_viewer() {
    let viewer = Viewer::new();
    let pattern =
        regex::Regex::new(r#"<(?:script|link|img)\b[^>]*?\b(?:src|href)="([^"]+)""#).unwrap();
    for url in [
        "/",
        "/demo/issues",
        "/demo/issues/8",
        "/demo/issues/new",
        "/demo/labels",
    ] {
        let html = viewer.get(url).text;
        for address in pattern.captures_iter(&html) {
            assert!(address[1].starts_with('/'), "{url}: {}", &address[1]);
        }
    }
}

#[test]
fn the_health_check_answers() {
    let viewer = Viewer::new();
    assert_eq!(
        viewer.get("/healthz").json(),
        serde_json::json!({ "status": "ok" })
    );
}

#[test]
fn static_files_are_cached_for_good_when_asked_for_by_version_and_revalidated_otherwise() {
    let viewer = Viewer::new();
    let plain = viewer.get("/static/app.css");
    assert_eq!(plain.status, 200);
    assert!(plain.header("content-type").starts_with("text/css"));
    assert_eq!(plain.header("cache-control"), "no-cache");
    let etag = plain.header("etag").to_string();
    assert_eq!(
        viewer
            .get_with("/static/app.css", &[("if-none-match", &etag)])
            .status,
        304
    );
    let page = viewer.get("/").text;
    let versioned = crate::support::capture(r#"href="(/static/app\.css\?v=[0-9a-f]+)""#, &page);
    assert!(
        viewer
            .get(&versioned)
            .header("cache-control")
            .contains("immutable")
    );
    assert_eq!(viewer.get("/static/missing.css").status, 404);
}
