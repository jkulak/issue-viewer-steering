use issue_viewer::web::openapi::{ROUTES, schema};

use crate::support::{JSON, Open, Viewer, contents, issue_path, read};

#[test]
fn a_write_without_the_token_or_with_a_wrong_one_is_refused() {
    let viewer = Viewer::new();
    let before = contents(&viewer.demo);
    assert_eq!(
        viewer
            .post_raw("/demo/issues/4/comments", &[("action", "close")], &[])
            .status,
        403
    );
    let reply = viewer.post_raw(
        "/demo/issues/4/comments",
        &[("action", "close"), ("_csrf", "guess")],
        &[],
    );
    assert_eq!(reply.status, 403);
    assert!(reply.text.contains("Reload"));
    assert_eq!(contents(&viewer.demo), before);
}

#[test]
fn a_write_from_another_origin_is_refused_even_with_the_token() {
    let viewer = Viewer::new();
    let before = contents(&viewer.demo);
    let token = viewer.token();
    let fields = [("_csrf", token.as_str()), ("action", "close")];
    for origin in ["https://attacker.example", "null", "http://localhost:9999"] {
        let reply = viewer.post_raw("/demo/issues/4/comments", &fields, &[("origin", origin)]);
        assert_eq!(reply.status, 403, "{origin}");
    }
    let reply = viewer.post_raw(
        "/demo/issues/4/comments",
        &fields,
        &[("sec-fetch-site", "cross-site")],
    );
    assert_eq!(reply.status, 403);
    assert_eq!(contents(&viewer.demo), before);
}

#[test]
fn a_write_from_the_viewers_own_page_goes_through() {
    let viewer = Viewer::new();
    let token = viewer.token();
    let reply = viewer.post_raw(
        "/demo/issues/4/comments",
        &[("_csrf", token.as_str()), ("action", "close")],
        &[
            ("origin", "http://localhost"),
            ("sec-fetch-site", "same-origin"),
        ],
    );
    assert_eq!(reply.status, 303);
    assert!(read(&issue_path(&viewer.demo, 4)).contains("state: done"));
}

#[test]
fn a_host_name_the_viewer_does_not_answer_to_is_refused() {
    let viewer = Viewer::new();
    assert_eq!(
        viewer
            .get_with("/demo/issues", &[("host", "rebound.attacker.example")])
            .status,
        400
    );
    assert_eq!(
        viewer
            .get_with("/demo/issues", &[("host", "127.0.0.1:8080")])
            .status,
        200
    );
}

#[test]
fn every_write_to_a_read_only_project_is_refused() {
    let viewer = Viewer::new();
    let before = contents(&viewer.archive);
    let writes: [(&str, &[(&str, &str)]); 6] = [
        ("/archive/issues", &[("title", "x")]),
        ("/archive/issues/2/title", &[("title", "x")]),
        ("/archive/issues/2/description", &[("text", "x")]),
        ("/archive/issues/2/labels", &[("labels", "x")]),
        (
            "/archive/issues/2/comments",
            &[("text", "x"), ("action", "close")],
        ),
        ("/archive/issues/2/comments/0/delete", &[]),
    ];
    for (url, fields) in writes {
        assert_eq!(viewer.post(url, fields).status, 403, "{url}");
    }
    let token = viewer.token();
    let reply = viewer.post_raw(
        "/archive/issues/2/tasks",
        &[("part", "description"), ("line", "0"), ("checked", "1")],
        &[("x-csrf-token", &token), JSON],
    );
    assert_eq!(reply.status, 403);
    assert_eq!(contents(&viewer.archive), before);
}

#[test]
fn no_address_reaches_a_file_outside_the_configured_directories() {
    let viewer = Viewer::new();
    for url in [
        "/../etc/passwd",
        "/%2e%2e/%2e%2e/etc/passwd",
        "/demo/issues/..%2F..%2Fetc%2Fpasswd",
        "/demo/issues/-1",
        "/static/../app.py",
        "/static/%2e%2e/app.py",
        "/static/..%2FCargo.toml",
    ] {
        let reply = viewer.get(url);
        assert_eq!(reply.status, 404, "{url}");
        assert!(!reply.text.contains("root:"));
    }
}

#[test]
fn a_body_over_the_limit_is_refused_rather_than_read_as_an_empty_form() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 3);
    let before = read(&path);
    let text = "x".repeat(5 * 1024 * 1024);
    let reply = viewer.post(
        "/demo/issues/3/description",
        &[("text", &text), ("version", &viewer.version_of(3))],
    );
    assert_eq!(reply.status, 413);
    assert_eq!(read(&path), before);
}

#[test]
fn a_known_address_with_the_wrong_method_is_refused() {
    let viewer = Viewer::new();
    assert_eq!(viewer.get("/demo/issues/3/title").status, 405);
    assert_eq!(
        viewer.send("DELETE", "/demo/issues/3", &[], None).status,
        405
    );
}

#[test]
fn the_published_openapi_schema_is_up_to_date() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("openapi.json");
    let published: serde_json::Value = serde_json::from_str(&read(&path)).unwrap();
    assert!(
        published == schema(),
        "run `make openapi` and commit openapi.json"
    );
}

#[test]
fn every_route_in_the_schema_answers() {
    let viewer = Viewer::with_open_root(Open::ReadWrite);
    for route in ROUTES {
        let url = route
            .path
            .replace("{project_name}", "demo")
            .replace("{number}", "3")
            .replace("{index}", "0");
        let reply = if route.method == "get" {
            viewer.get(&url)
        } else {
            let version = viewer.version_of(3);
            let fields = [
                ("path", "demo"),
                ("title", "A title"),
                ("text", "Some text"),
                ("action", "comment"),
                ("part", "description"),
                ("line", "0"),
                ("checked", "1"),
                ("version", version.as_str()),
            ];
            viewer.post(&url, &fields)
        };
        assert!(
            ![404, 405].contains(&reply.status),
            "{} {url}: {}",
            route.method,
            reply.status
        );
    }
}
