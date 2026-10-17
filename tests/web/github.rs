//! GitHub on the board, answered by the fake gh in `tests/fixtures/github`: where issues and pull
//! requests land, what a card says, choosing repositories, and what happens when gh fails.

use std::path::Path;

use crate::board::{branch_with_a_commit, card_html, columns, git};
use crate::support::{Viewer, capture, read};

fn id(repository: &str, number: u64) -> String {
    format!("github/acme-notes/{repository}/{number}")
}

fn shows(cards: &[String], repository: &str, number: u64) -> bool {
    cards.contains(&id(repository, number))
}

fn github_cards(cards: &[String]) -> Vec<&str> {
    cards
        .iter()
        .map(String::as_str)
        .filter(|card| card.starts_with("github/"))
        .collect()
}

fn read_viewer() -> Viewer {
    let viewer = Viewer::with_github("acme-notes/*");
    viewer.refresh_github();
    viewer
}

fn version(viewer: &Viewer, uri: &str) -> String {
    viewer.get(uri).json()["version"]
        .as_str()
        .unwrap()
        .to_string()
}

/// Change one of the fake gh's answers.
fn rewrite(viewer: &Viewer, file: &str, change: impl FnOnce(String) -> String) {
    let path = viewer.github.join(file);
    std::fs::write(&path, change(read(&path))).unwrap();
}

#[test]
fn github_issues_go_to_the_column_their_labels_pull_requests_and_state_say() {
    let viewer = read_viewer();
    let reply = viewer.get("/-/board?done=all");
    assert_eq!(reply.status, 200);
    let found = columns(&reply.text);
    let gh = |key: &str| github_cards(&found[key]);
    assert_eq!(
        gh("backlog"),
        [id("notes-api", 16), id("notes-web", 5), id("notes-api", 10)]
    );
    assert_eq!(gh("selected"), [id("notes-web", 3)]);
    // A label and a draft pull request agree; a draft that closes nothing is a card of its own.
    assert_eq!(
        gh("in-progress"),
        [id("notes-api", 12), id("notes-api", 18)]
    );
    // A pull request ready for review moves its issue; an approved one that closes nothing is a
    // card of its own.
    assert_eq!(gh("review"), [id("notes-api", 11), id("notes-api", 17)]);
    assert_eq!(gh("blocked"), [id("notes-api", 13)]);
    assert_eq!(
        gh("done"),
        [id("notes-api", 9), id("notes-api", 8), id("notes-web", 4)]
    );
    assert!(gh("testing").is_empty());
    // The pull requests that close a shown issue are on its card, not cards of their own.
    for key in found.keys() {
        assert!(!shows(&found[key], "notes-api", 14), "{key}");
        assert!(!shows(&found[key], "notes-api", 15), "{key}");
    }
    // The issue files are still there.
    assert!(found["in-progress"].contains(&"demo-10".to_string()));
}

#[test]
fn a_github_card_links_to_github_in_a_new_tab_and_cannot_be_moved() {
    let viewer = read_viewer();
    let html = viewer.get("/-/board?done=all").text;
    let card = card_html(&html, &id("notes-api", 11));
    assert!(card.contains(
        r#"href="https://github.com/acme-notes/notes-api/issues/11" target="_blank" rel="noopener noreferrer""#
    ));
    assert!(card.contains("Search notes by tag"));
    assert!(card.contains("acme-notes/notes-api"));
    assert!(card.contains(">from PR<"));
    assert!(card.contains(r#"aria-label="Assigned to dev-ada""#));
    assert!(card.contains(r#"aria-label="3 comments""#));
    assert!(card.contains(r#"href="https://github.com/acme-notes/notes-api/pull/14""#));
    assert!(card.contains(">#14</a>"));
    assert!(card.contains("review required"));
    assert!(card.contains("checks passing"));
    assert!(card.contains(">enhancement<"));
    for missing in [
        "draggable",
        "data-status-url",
        "js-card",
        "Move #",
        "/status",
    ] {
        assert!(!card.contains(missing), "{missing}");
    }

    let draft = card_html(&html, &id("notes-api", 12));
    assert!(draft.contains(">#15</a>"));
    assert!(draft.contains(">draft<"));
    assert!(draft.contains("checks pending"));
    assert!(!draft.contains(">from PR<"), "the label already says so");
    assert!(!draft.contains(">in-progress<"), "the column says so");

    let pull = card_html(&html, &id("notes-api", 17));
    assert!(pull.contains("Open pull request"));
    assert!(pull.contains(r#"href="https://github.com/acme-notes/notes-api/pull/17""#));
    assert!(pull.contains("bump-json-parser"));
    assert!(pull.contains(">approved<"));
    assert!(pull.contains("checks failing"));
    assert!(card_html(&html, &id("notes-api", 18)).contains("Draft pull request"));

    // Only open blockers count, and one in another repository is named with it.
    let blocked = card_html(&html, &id("notes-api", 13));
    assert!(blocked.contains(r#"href="https://github.com/acme-notes/notes-api/issues/12""#));
    assert!(blocked.contains(">#12</a>"));
    assert!(!blocked.contains("#9"));
    let elsewhere = card_html(&html, &id("notes-web", 5));
    assert!(elsewhere.contains(">acme-notes/notes-api#12</a>"));

    let not_planned = card_html(&html, &id("notes-api", 8));
    assert!(not_planned.contains("Closed as not planned"));
}

#[test]
fn the_projects_menu_lists_github_repositories_and_the_board_shows_the_chosen_ones() {
    let viewer = read_viewer();
    let html = viewer.get("/-/board").text;
    for name in ["acme-notes/notes-api", "acme-notes/notes-web"] {
        assert!(
            html.contains(&format!(r#"name="project" value="{name}""#)),
            "{name}"
        );
    }

    let reply = viewer.get("/-/board?project=acme-notes/notes-web");
    assert_eq!(reply.status, 200);
    let found = columns(&reply.text);
    let all: Vec<&String> = found.values().flatten().collect();
    assert_eq!(all.len(), 3, "{all:?}");
    assert!(
        all.iter()
            .all(|card| card.starts_with("github/acme-notes/notes-web/"))
    );
    assert!(
        reply
            .text
            .contains(r#"value="acme-notes/notes-web" checked"#)
    );

    let both = viewer.get("/-/board?project=demo&project=acme-notes/notes-api");
    let found = columns(&both.text);
    let all: Vec<&String> = found.values().flatten().collect();
    assert!(all.iter().any(|card| card.starts_with("demo-")));
    assert!(
        all.iter()
            .any(|card| card.starts_with("github/acme-notes/notes-api/"))
    );
    assert!(!all.iter().any(|card| card.contains("notes-web")));

    // A project's own board stays its own.
    let own = viewer.get("/demo/board").text;
    assert!(!own.contains("github/"));
}

#[test]
fn a_repository_that_is_not_configured_is_not_found() {
    let viewer = read_viewer();
    let reply = viewer.get("/-/board?project=someone-else/notes");
    assert_eq!(reply.status, 404);
    assert!(
        reply
            .text
            .contains("There is no GitHub repository “someone-else/notes” in GITHUB_REPOS.")
    );
    assert_eq!(
        viewer
            .get("/-/board/version?project=someone-else/notes")
            .status,
        404
    );

    // An owner's repository GitHub did not list: archived, or a fork.
    let reply = viewer.get("/-/board?project=acme-notes/old-notes");
    assert_eq!(reply.status, 200);
    assert!(reply.text.contains("GitHub listed no repository called"));

    let named = Viewer::with_github("acme-notes/notes-api");
    named.refresh_github();
    assert_eq!(
        named.get("/-/board?project=acme-notes/notes-web").status,
        404
    );
    assert_eq!(
        named.get("/-/board?project=acme-notes/notes-api").status,
        200
    );
}

#[test]
fn the_first_board_says_it_is_reading_github_and_its_version_changes_when_the_answer_arrives() {
    let viewer = Viewer::with_github("acme-notes/notes-api");
    let reply = viewer.get("/-/board");
    assert!(reply.text.contains("Reading GitHub."));
    assert!(
        github_cards(
            &columns(&reply.text)
                .into_values()
                .flatten()
                .collect::<Vec<_>>()
        )
        .is_empty()
    );
    let before = capture(r#"data-version="([0-9a-f]+)""#, &reply.text);
    // The board asked for a refresh in the background; this one waits for it and reads again.
    viewer.refresh_github();
    let after = version(&viewer, "/-/board/version");
    assert_ne!(before, after);
    let reply = viewer.get("/-/board");
    assert!(!reply.text.contains("Reading GitHub."));
    assert!(reply.text.contains("GitHub read "));
    assert!(reply.text.contains(&id("notes-api", 11)));
}

#[test]
fn the_version_changes_when_github_s_answer_does_and_not_when_it_is_the_same() {
    let viewer = read_viewer();
    let first = version(&viewer, "/-/board/version");
    let without_github = version(&viewer, "/-/board/version?project=demo");
    viewer.refresh_github();
    assert_eq!(version(&viewer, "/-/board/version"), first);
    rewrite(&viewer, "acme-notes--notes-web.json", |text| {
        text.replace("Dark mode", "Dark and light mode")
    });
    viewer.refresh_github();
    let changed = version(&viewer, "/-/board/version");
    assert_ne!(changed, first);
    // A board without GitHub does not change with it.
    assert_eq!(
        version(&viewer, "/-/board/version?project=demo"),
        without_github
    );
}

#[test]
fn a_repository_that_cannot_be_read_is_reported_and_keeps_its_last_cards() {
    let viewer = read_viewer();
    std::fs::remove_file(viewer.github.join("acme-notes--notes-web.json")).unwrap();
    viewer.refresh_github();
    let html = viewer.get("/-/board").text;
    assert!(html.contains("Could not read <strong>acme-notes/notes-web</strong> from GitHub"));
    assert!(html.contains("Could not resolve to a Repository with the name"));
    assert!(html.contains("Its cards are from the last answer that could be read."));
    let found = columns(&html);
    assert!(shows(&found["selected"], "notes-web", 3));
    assert!(shows(&found["review"], "notes-api", 11));

    // A repository that was never read has nothing to keep.
    let fresh = Viewer::with_github("acme-notes/notes-api,acme-notes/missing");
    fresh.refresh_github();
    let html = fresh.get("/-/board").text;
    assert!(html.contains("Could not read <strong>acme-notes/missing</strong>"));
    assert!(!html.contains("Its cards are from the last answer"));
    assert!(shows(&columns(&html)["review"], "notes-api", 11));
}

#[test]
fn an_owner_that_cannot_be_listed_keeps_the_repositories_listed_before() {
    let viewer = read_viewer();
    std::fs::remove_file(viewer.github.join("acme-notes.repos.json")).unwrap();
    viewer.refresh_github();
    let html = viewer.get("/-/board").text;
    assert!(html.contains("Could not list the repositories of acme-notes: HTTP 404"));
    assert!(shows(&columns(&html)["selected"], "notes-web", 3));
}

#[test]
fn a_missing_gh_is_named_as_the_problem() {
    let viewer = Viewer::with_github("acme-notes/notes-api");
    std::fs::remove_file(viewer.github.join("gh")).unwrap();
    viewer.refresh_github();
    let html = viewer.get("/-/board").text;
    assert!(html.contains("gh is not installed, or not on the PATH the viewer was started with"));
}

#[test]
fn more_open_issues_than_the_board_reads_are_noted() {
    let viewer = Viewer::with_github("acme-notes/notes-api");
    rewrite(&viewer, "acme-notes--notes-api.json", |text| {
        text.replacen(r#""totalCount": 5"#, r#""totalCount": 250"#, 1)
    });
    viewer.refresh_github();
    let html = viewer.get("/-/board").text;
    assert!(html.contains(
        "acme-notes/notes-api has 250 open issues; the board shows the 5 most recently updated."
    ));
}

#[test]
fn the_filter_applies_to_github_cards() {
    let viewer = read_viewer();
    let cards = |query: &str| -> Vec<String> {
        let html = viewer.get_query("/-/board", &[("q", query)]).text;
        github_cards(&columns(&html).into_values().flatten().collect::<Vec<_>>())
            .into_iter()
            .map(str::to_string)
            .collect()
    };
    assert_eq!(cards("label:bug"), [id("notes-api", 10)]);
    assert_eq!(cards("is:closed reason:not-planned"), [id("notes-api", 8)]);
    assert_eq!(cards("bump-json-parser"), [id("notes-api", 17)]);
    let assigned = cards("dev-ada");
    assert!(assigned.contains(&id("notes-api", 11)), "{assigned:?}");
    assert!(!assigned.contains(&id("notes-api", 12)), "{assigned:?}");
}

#[test]
fn a_numbered_branch_in_a_checkout_puts_its_issue_in_progress() {
    let viewer = Viewer::with_github("acme-notes/notes-api={root}/notes-api");
    let checkout = viewer.root.join("notes-api");
    std::fs::create_dir_all(&checkout).unwrap();
    start(&checkout);
    branch_with_a_commit(&checkout, "10-large-attachments", None);
    viewer.refresh_github();
    let html = viewer.get("/-/board").text;
    assert!(shows(&columns(&html)["in-progress"], "notes-api", 10));
    let card = card_html(&html, &id("notes-api", 10));
    assert!(card.contains(">from git<"));
    assert!(card.contains("10-large-attachments"));
    assert!(card.contains("1 ahead"));
}

fn start(directory: &Path) {
    git(
        directory,
        &["init", "--quiet", "--initial-branch=main"],
        None,
    );
    std::fs::write(directory.join("README.md"), "notes\n").unwrap();
    git(directory, &["add", "."], None);
    git(directory, &["commit", "--quiet", "-m", "Start"], None);
}

#[test]
fn without_github_repositories_the_board_says_nothing_about_github() {
    let viewer = Viewer::new();
    let html = viewer.get("/-/board").text;
    assert!(!html.contains("GitHub"));
    assert!(!html.contains("github/"));
}
