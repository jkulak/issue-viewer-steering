//! The board: columns, filters, moving a card, and what git says about the work.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::Command;

use crate::support::{JSON, Viewer, issue_path, read};

/// The cards in each column, as `project-number`, in the order shown.
pub fn columns(html: &str) -> BTreeMap<String, Vec<String>> {
    let column =
        regex::Regex::new(r#"<section class="board-column[^"]*" data-column="([a-z-]+)""#).unwrap();
    let card = regex::Regex::new(r#"id="card-([^"]+)""#).unwrap();
    let starts: Vec<(usize, String)> = column
        .captures_iter(html)
        .map(|found| (found.get(0).unwrap().start(), found[1].to_string()))
        .collect();
    let mut found = BTreeMap::new();
    for (index, (start, key)) in starts.iter().enumerate() {
        let end = starts.get(index + 1).map_or(html.len(), |(next, _)| *next);
        let cards = card
            .captures_iter(&html[*start..end])
            .map(|found| found[1].to_string())
            .collect();
        found.insert(key.clone(), cards);
    }
    found
}

pub fn card_html<'a>(html: &'a str, id: &str) -> &'a str {
    let start = html
        .find(&format!(r#"id="card-{id}""#))
        .unwrap_or_else(|| panic!("no card {id}"));
    let end = html[start..].find("</li>").unwrap() + start;
    &html[start..end]
}

#[test]
fn the_board_puts_every_project_s_issues_in_seven_columns() {
    let viewer = Viewer::new();
    let reply = viewer.get("/-/board");
    assert_eq!(reply.status, 200);
    let found = columns(&reply.text);
    assert_eq!(
        found.keys().collect::<Vec<_>>(),
        [
            "backlog",
            "blocked",
            "done",
            "in-progress",
            "review",
            "selected",
            "testing"
        ]
    );
    // A status kept in the state counts; nothing else in the fixtures has one.
    assert_eq!(found["in-progress"], ["demo-10"]);
    for key in ["selected", "testing", "review", "blocked"] {
        assert!(found[key].is_empty(), "{key}: {:?}", found[key]);
    }
    // Open issues with no status wait for a decision, from both projects.
    for id in [
        "demo-3",
        "demo-4",
        "demo-5",
        "demo-7",
        "demo-11",
        "archive-2",
    ] {
        assert!(found["backlog"].contains(&id.to_string()), "{id}");
    }
    assert_eq!(found["backlog"].len(), 26 + 1);
    // Done: 13 closed in demo, 1 in archive, the ten most recent of them shown.
    assert_eq!(found["done"].len(), 10);
    assert!(reply.text.contains("Show 4 more"));
    assert!(
        reply
            .text
            .contains(r#"<strong class="js-count-waiting">27</strong>"#)
    );
    // With several projects, a card says which one it is from.
    assert!(card_html(&reply.text, "archive-2").contains("board-card-project"));
}

#[test]
fn done_shows_every_card_when_asked() {
    let viewer = Viewer::new();
    let reply = viewer.get("/-/board?done=all");
    assert_eq!(columns(&reply.text)["done"].len(), 14);
    assert!(!reply.text.contains("more</a>"));
}

#[test]
fn the_board_can_be_limited_to_some_projects() {
    let viewer = Viewer::new();
    let reply = viewer.get("/-/board?project=archive");
    assert_eq!(reply.status, 200);
    let found = columns(&reply.text);
    let all: Vec<&String> = found.values().flatten().collect();
    assert_eq!(all, ["archive-2", "archive-1"]);
    assert!(
        reply
            .text
            .contains(r#"<input type="checkbox" name="project" value="archive" checked>"#)
    );
    assert!(
        reply
            .text
            .contains(r#"data-version-url="/-/board/version?project=archive""#)
    );
    assert!(!card_html(&reply.text, "archive-2").contains("board-card-project"));

    let both = viewer.get("/-/board?project=demo&project=archive");
    assert_eq!(columns(&both.text)["backlog"].len(), 27);

    let unknown = viewer.get("/-/board?project=nope");
    assert_eq!(unknown.status, 404);
    assert!(unknown.text.contains("There is no project called “nope”."));
}

#[test]
fn a_project_s_board_is_a_tab_next_to_its_issues_and_the_projects_page_links_to_it() {
    let viewer = Viewer::new();
    let reply = viewer.get("/demo/board");
    assert_eq!(reply.status, 200);
    assert!(
        reply
            .text
            .contains(r#"href="/demo/board" aria-current="page""#)
    );
    assert!(!reply.text.contains("card-archive-"));
    assert!(
        reply
            .text
            .contains(r#"data-version-url="/-/board/version?project=demo""#)
    );
    let issues = viewer.get("/demo/issues");
    assert!(issues.text.contains(r#"href="/demo/board""#));
    let home = viewer.get("/");
    assert!(home.text.contains(r#"href="/demo/board""#));
    assert!(home.text.contains(r#"href="/-/board""#));
    assert_eq!(viewer.get("/nope/board").status, 404);
}

#[test]
fn the_filter_takes_the_issue_list_syntax() {
    let viewer = Viewer::new();
    let reply = viewer.get("/demo/board?q=label%3Abug+crash");
    let all: Vec<String> = columns(&reply.text).into_values().flatten().collect();
    assert_eq!(all, ["demo-4"]);
    assert!(reply.text.contains("Clear the filter"));
    let labelled = viewer.get("/demo/board?q=label%3Abug");
    let all: Vec<String> = columns(&labelled.text).into_values().flatten().collect();
    assert!(all.contains(&"demo-4".to_string()));
    assert!(!all.contains(&"demo-3".to_string()));
    let open = viewer.get("/demo/board?q=is%3Aopen");
    assert!(columns(&open.text)["done"].is_empty());
}

#[test]
fn the_version_changes_when_an_issue_file_does() {
    let viewer = Viewer::new();
    let version = |query: &str| {
        viewer.get(&format!("/-/board/version{query}")).json()["version"]
            .as_str()
            .unwrap()
            .to_string()
    };
    let (demo, archive) = (version("?project=demo"), version("?project=archive"));
    let path = issue_path(&viewer.demo, 7);
    std::fs::write(&path, read(&path).replace("labels: []", "labels: [review]")).unwrap();
    assert_ne!(version("?project=demo"), demo);
    assert_eq!(version("?project=archive"), archive);
    assert_eq!(viewer.get("/-/board/version?project=nope").status, 404);
}

#[test]
fn moving_a_card_writes_that_column_s_label_and_nothing_else() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 4);
    let before = read(&path);
    let version = viewer.version_of(4);
    let reply = viewer.post(
        "/demo/issues/4/status",
        &[
            ("status", "testing"),
            ("version", &version),
            ("back", "/-/board?q=bug"),
        ],
    );
    assert_eq!(reply.status, 303);
    assert_eq!(reply.location(), "/-/board?q=bug");
    assert_eq!(
        read(&path),
        before.replace("labels: [bug]", "labels: [bug, testing]")
    );
    let board = viewer.get("/-/board");
    assert_eq!(columns(&board.text)["testing"], ["demo-4"]);
    // The column says it; the label is not repeated on the card.
    assert!(!card_html(&board.text, "demo-4").contains(">testing<"));

    // Moving on replaces it.
    viewer.post("/demo/issues/4/status", &[("status", "review")]);
    assert_eq!(
        read(&path),
        before.replace("labels: [bug]", "labels: [bug, review]")
    );
}

#[test]
fn moving_to_done_closes_the_issue_and_moving_out_reopens_it() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 7);
    let before = read(&path);
    viewer.post("/demo/issues/7/status", &[("status", "done")]);
    assert_eq!(read(&path), before.replace("state: open", "state: done"));
    viewer.post("/demo/issues/7/status", &[("status", "selected")]);
    assert_eq!(
        read(&path),
        before.replace("labels: []", "labels: [selected]")
    );
    // The README row follows the labels and the state.
    viewer.post("/demo/issues/4/status", &[("status", "done")]);
    let index = read(&viewer.demo.join("README.md"));
    assert!(
        index.contains("(004-crash-on-empty-title.md) | bug | done |"),
        "{index}"
    );
}

#[test]
fn the_board_s_script_gets_the_new_version_and_column_as_json() {
    let viewer = Viewer::new();
    let version = viewer.version_of(3);
    let token = viewer.token();
    let reply = viewer.send(
        "POST",
        "/demo/issues/3/status",
        &[JSON, ("x-csrf-token", &token)],
        Some(format!("status=blocked&version={version}")),
    );
    assert_eq!(reply.status, 200, "{}", reply.text);
    let answer = reply.json();
    assert_eq!(answer["column"], "blocked");
    assert_eq!(answer["version"], viewer.version_of(3).as_str());
}

#[test]
fn a_move_is_refused_when_it_cannot_be_trusted_or_written() {
    let viewer = Viewer::new();
    let path = issue_path(&viewer.demo, 4);
    let before = read(&path);

    let stale = viewer.post(
        "/demo/issues/4/status",
        &[("status", "testing"), ("version", "0123")],
    );
    assert_eq!(stale.status, 409);
    assert!(stale.text.contains("Reload the board."));

    let no_token = viewer.post_raw("/demo/issues/4/status", &[("status", "testing")], &[]);
    assert_eq!(no_token.status, 403);

    let unknown = viewer.post("/demo/issues/4/status", &[("status", "someday-maybe")]);
    assert_eq!(unknown.status, 422);

    let missing = viewer.post("/demo/issues/99/status", &[("status", "testing")]);
    assert_eq!(missing.status, 404);

    let broken = viewer.post("/demo/issues/12/status", &[("status", "testing")]);
    assert_eq!(broken.status, 422);
    assert_eq!(read(&path), before);

    let archive = issue_path(&viewer.archive, 2);
    let archived = read(&archive);
    let read_only = viewer.post("/archive/issues/2/status", &[("status", "testing")]);
    assert_eq!(read_only.status, 403);
    assert_eq!(read(&archive), archived);
    // Nor does the board offer it.
    let board = viewer.get("/-/board");
    assert!(!card_html(&board.text, "archive-2").contains("draggable"));
    assert!(!card_html(&board.text, "archive-2").contains("Move to"));
    assert!(card_html(&board.text, "demo-4").contains(r#"draggable="true""#));

    // Going back never leaves the site.
    let away = viewer.post(
        "/demo/issues/4/status",
        &[("status", "testing"), ("back", "//example.com/")],
    );
    assert_eq!(away.location(), "/-/board");
}

/// Run git in `directory` with a fixed author and none of this machine's configuration.
pub fn git(directory: &Path, args: &[&str], date: Option<&str>) {
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(directory)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .stdout(std::process::Stdio::null());
    if let Some(date) = date {
        command
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
    }
    assert!(command.status().unwrap().success(), "git {args:?}");
}

/// A commit on a new branch, then back to main.
pub fn branch_with_a_commit(directory: &Path, name: &str, date: Option<&str>) {
    git(directory, &["checkout", "--quiet", "-b", name], None);
    std::fs::write(directory.join(format!("{name}.txt")), "work\n").unwrap();
    git(directory, &["add", "."], None);
    git(directory, &["commit", "--quiet", "-m", "Work"], date);
    git(directory, &["checkout", "--quiet", "main"], None);
}

#[test]
fn branches_named_after_an_issue_show_who_works_on_it() {
    let viewer = Viewer::new();
    let demo = &viewer.demo;
    git(demo, &["init", "--quiet", "--initial-branch=main"], None);
    git(demo, &["add", "."], None);
    git(demo, &["commit", "--quiet", "-m", "Start"], None);
    // Checked out in a worktree: being worked on.
    git(demo, &["branch", "007-keyboard-shortcuts"], None);
    let worktree = viewer.root.join("shortcuts");
    git(
        demo,
        &[
            "worktree",
            "add",
            "--quiet",
            worktree.to_str().unwrap(),
            "007-keyboard-shortcuts",
        ],
        None,
    );
    std::fs::write(worktree.join("draft.txt"), "draft\n").unwrap();
    // Commits ahead from this week: being worked on, even without a worktree.
    branch_with_a_commit(demo, "003-search-notes", None);
    // Commits ahead from long ago: not.
    branch_with_a_commit(demo, "013-filler", Some("2020-01-01T12:00:00Z"));
    // A status label wins over what git says.
    branch_with_a_commit(demo, "005-dark-theme", None);
    // A worktree no issue claims.
    let spike = viewer.root.join("spike");
    git(
        demo,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "spike",
            spike.to_str().unwrap(),
        ],
        None,
    );

    let reply = viewer.get("/-/board");
    let found = columns(&reply.text);
    for id in ["demo-7", "demo-3", "demo-10"] {
        assert!(found["in-progress"].contains(&id.to_string()), "{id}");
    }
    for id in ["demo-13", "demo-5"] {
        assert!(found["backlog"].contains(&id.to_string()), "{id}");
    }
    let shortcuts = card_html(&reply.text, "demo-7");
    assert!(shortcuts.contains("from git"));
    assert!(shortcuts.contains("007-keyboard-shortcuts"));
    assert!(shortcuts.contains("in shortcuts"));
    assert!(shortcuts.contains("1 uncommitted"));
    let search = card_html(&reply.text, "demo-3");
    assert!(search.contains("1 ahead"));
    assert!(!search.contains("uncommitted"));
    assert!(!card_html(&reply.text, "demo-10").contains("from git"));
    assert!(card_html(&reply.text, "demo-13").contains("013-filler"));

    let loose = &reply.text[reply.text.find("Work without an issue").unwrap()..];
    assert!(loose.contains(">spike<"));
    assert!(!loose.contains(">main<"));
    // The archive is not in a repository, and its board has no branches to show.
    let archive = viewer.get("/-/board?project=archive");
    assert!(!archive.text.contains("Work without an issue"));
}
