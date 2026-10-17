use crate::support::{Open, Reply, Viewer, read};

const OPENED_LABEL: &str =
    r#"title="Picked with Open; forgotten when the viewer restarts">Opened</span>"#;

fn open_folder(viewer: &Viewer, path: &str, headers: &[(&str, &str)]) -> Reply {
    let token = viewer.token();
    viewer.post_raw(
        "/-/open",
        &[("_csrf", token.as_str()), ("path", path)],
        headers,
    )
}

#[test]
fn without_an_open_root_there_is_no_open_button_and_no_open_page() {
    let viewer = Viewer::new();
    assert!(!viewer.get("/").text.contains(r#"href="/-/open""#));
    assert_eq!(viewer.get("/-/open").status, 404);
    assert_eq!(viewer.post("/-/open", &[("path", "demo")]).status, 404);
}

#[test]
fn every_page_has_an_open_button_when_an_open_root_is_set() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    for url in ["/", "/demo/issues", "/demo/issues/3", "/demo/labels"] {
        assert!(viewer.get(url).text.contains(r#"href="/-/open""#), "{url}");
    }
}

#[test]
fn the_open_page_lists_the_folders_under_the_root_with_their_issue_files() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    let page = viewer.get("/-/open").text;
    assert!(page.contains(r#"href="/-/open?path=notes-app""#));
    assert!(page.contains(r#"href="/-/open?path=notes-app%2Ftasks">tasks/</a>"#));
    assert!(page.contains("Go to demo"));
    assert!(page.contains("2 issue files")); // the archive folder
    assert!(!page.contains(".hidden"));
    let inner = viewer
        .get_query("/-/open", &[("path", "notes-app/tasks")])
        .text;
    assert!(inner.contains("2 issue files in this folder"));
    assert!(inner.contains(r#"href="/-/open?path=notes-app" aria-label="Up one folder""#));
    assert!(inner.contains(r#"name="path" value="notes-app/tasks""#));
}

#[test]
fn opening_a_folder_adds_it_as_a_project_and_shows_its_issues() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    let reply = open_folder(&viewer, "notes-app/tasks", &[]);
    assert_eq!(reply.status, 303);
    assert_eq!(reply.location(), "/notes-app/issues");
    let listing = viewer.get("/notes-app/issues").text;
    assert!(listing.contains("Another old idea"));
    assert!(listing.contains(OPENED_LABEL));
    let home = viewer.get("/").text;
    assert!(home.contains(r#"href="/notes-app/issues""#));
    assert!(home.contains(OPENED_LABEL));
    assert!(!viewer.get("/demo/issues").text.contains(OPENED_LABEL));
}

#[test]
fn opening_a_folder_again_or_one_from_projects_goes_to_the_existing_project() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    assert_eq!(
        open_folder(&viewer, "notes-app/tasks", &[]).location(),
        "/notes-app/issues"
    );
    assert_eq!(
        open_folder(&viewer, "notes-app/tasks/", &[]).location(),
        "/notes-app/issues"
    );
    assert_eq!(open_folder(&viewer, "demo", &[]).location(), "/demo/issues");
    assert_eq!(viewer.project_names(), ["demo", "notes-app"]);
    assert!(
        viewer
            .get_query("/-/open", &[("path", "notes-app")])
            .text
            .contains("Go to notes-app")
    );
}

#[test]
fn an_opened_folder_can_be_changed_when_the_open_root_is_read_write() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    open_folder(&viewer, "notes-app/tasks", &[]);
    assert_eq!(
        viewer
            .post("/notes-app/issues/2/comments", &[("action", "close")])
            .status,
        303
    );
    assert!(
        read(&viewer.root.join("notes-app/tasks/002-another-old-idea.md")).contains("state: done")
    );
}

#[test]
fn an_opened_folder_is_read_only_when_the_open_root_is() {
    let viewer = Viewer::with_open_root(Open::ReadOnly);
    let path = viewer.root.join("notes-app/tasks/002-another-old-idea.md");
    let before = std::fs::read(&path).unwrap();
    assert_eq!(open_folder(&viewer, "notes-app/tasks", &[]).status, 303);
    assert!(viewer.get("/notes-app/issues/2").text.contains("Read-only"));
    assert_eq!(
        viewer
            .post("/notes-app/issues/2/comments", &[("action", "close")])
            .status,
        403
    );
    assert_eq!(std::fs::read(&path).unwrap(), before);
}

#[test]
fn opening_needs_the_token_and_a_request_from_the_viewers_own_pages() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    assert_eq!(
        viewer
            .post_raw("/-/open", &[("path", "notes-app/tasks")], &[])
            .status,
        403
    );
    assert_eq!(
        open_folder(
            &viewer,
            "notes-app/tasks",
            &[("origin", "https://attacker.example")]
        )
        .status,
        403
    );
    assert_eq!(
        open_folder(
            &viewer,
            "notes-app/tasks",
            &[("sec-fetch-site", "cross-site")]
        )
        .status,
        403
    );
    assert_eq!(viewer.project_names(), ["demo"]);
}

#[test]
fn hidden_folders_and_ways_out_of_the_root_can_be_neither_listed_nor_opened() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    for path in [
        "..",
        "../..",
        "notes-app/../..",
        ".hidden",
        ".hidden/tasks",
        "missing",
        "demo/README.md",
    ] {
        assert_eq!(
            viewer.get_query("/-/open", &[("path", path)]).status,
            404,
            "{path}"
        );
        assert_eq!(open_folder(&viewer, path, &[]).status, 404, "{path}");
    }
    assert_eq!(viewer.project_names(), ["demo"]);
}

#[test]
fn a_link_to_a_folder_that_is_no_longer_open_says_why() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    let reply = viewer.get("/notes-app/issues");
    assert_eq!(reply.status, 404);
    assert!(reply.text.contains("forgotten when the viewer restarts"));
    assert!(reply.text.contains(r#"href="/-/open""#));
}
