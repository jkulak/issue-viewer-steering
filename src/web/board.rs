//! The board: the issues of one project, several or all of them, in the columns `board` gives
//! them, with the branches that are being worked on, and the move that changes an issue's column.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;

use axum::http::StatusCode;
use jiff::Timestamp;
use minijinja::context;
use minijinja::value::Value;
use serde::Serialize;
use serde_json::json;

use super::pages::{find_project, json, no_issue, read_only, sentence, url_encoded};
use super::views;
use super::writes::{self, failed_write, same_origin};
use super::{Answer, App, Failure, Incoming, redirect};
use crate::board::{self, Column, WorkSignal};
use crate::files;
use crate::git::{self, Branch, Repository};
use crate::issues::Issue;
use crate::project::{Project, ProjectError, Snapshot};
use crate::query;

mod github;

use github::{PullView, Shown};

/// Done only grows; the most recent ones are what a glance needs.
pub const DONE_LIMIT: usize = 10;
/// What the summary counts as active now.
const LATELY_SECONDS: i64 = 60 * 60;

/// The projects and GitHub repositories a request names, in its order; none means every one.
fn chosen(app: &App, request: &Incoming) -> Result<Vec<String>, Failure> {
    let mut names = Vec::new();
    for (key, value) in &request.query {
        if key == "project" && !value.is_empty() && !names.contains(value) {
            if value.contains('/') {
                if !app.github.knows(value) {
                    return Err(Failure::new(
                        StatusCode::NOT_FOUND,
                        format!("There is no GitHub repository “{value}” in GITHUB_REPOS."),
                    ));
                }
            } else {
                find_project(app, value)?;
            }
            names.push(value.clone());
        }
    }
    Ok(names)
}

/// What a board polls: its issue files and branches, and GitHub's answer when it shows GitHub.
fn signature(scope: &Scope, shown: &Shown) -> String {
    let local = scope.signature();
    match shown.signature() {
        remote if remote.is_empty() => local,
        remote => files::digest(&format!("{local}\n{remote}")),
    }
}

/// Every project, which of them the board shows, and the repositories of the shown ones.
struct Scope {
    projects: Arc<Vec<Arc<Project>>>,
    shown: Vec<bool>,
    /// Shown, or in the same repository as a shown project: its issues decide whose a branch is.
    needed: Vec<bool>,
    repositories: Vec<Option<Arc<Repository>>>,
}

impl Scope {
    fn new(app: &App, names: &[String]) -> Self {
        let projects = app.registry.all();
        let shown: Vec<bool> = projects
            .iter()
            .map(|project| names.is_empty() || names.iter().any(|name| name == project.name()))
            .collect();
        let common: Vec<Option<PathBuf>> = projects
            .iter()
            .map(|project| app.git.common_dir(project.path()))
            .collect();
        let wanted: HashSet<&PathBuf> = common
            .iter()
            .zip(&shown)
            .filter_map(|(dir, shown)| dir.as_ref().filter(|_| *shown))
            .collect();
        let needed: Vec<bool> = common
            .iter()
            .zip(&shown)
            .map(|(dir, shown)| *shown || dir.as_ref().is_some_and(|dir| wanted.contains(dir)))
            .collect();
        let repositories = projects
            .iter()
            .zip(&needed)
            .map(|(project, needed)| needed.then(|| app.git.repository(project.path())).flatten())
            .collect();
        Scope {
            projects,
            shown,
            needed,
            repositories,
        }
    }

    /// Changes when an issue file of a needed project or a branch of their repositories does.
    fn signature(&self) -> String {
        let mut listing = String::new();
        for (index, project) in self.projects.iter().enumerate() {
            if self.needed[index] {
                listing.push_str(&format!("{}\0{}\n", project.name(), project.signature()));
            }
        }
        for repository in self.unique_repositories() {
            listing.push_str(&format!(
                "{}\0{}\n",
                repository.common_dir.display(),
                repository.signature()
            ));
        }
        files::digest(&listing)
    }

    fn unique_repositories(&self) -> Vec<&Arc<Repository>> {
        let mut seen = HashSet::new();
        self.repositories
            .iter()
            .flatten()
            .filter(|repository| seen.insert(repository.common_dir.clone()))
            .collect()
    }

    fn members(&self, repository: &Repository) -> Vec<usize> {
        (0..self.projects.len())
            .filter(|&index| {
                self.repositories[index]
                    .as_ref()
                    .is_some_and(|other| other.common_dir == repository.common_dir)
            })
            .collect()
    }
}

/// Of the projects that have the branch's issue number, the ones whose issue file name shares the
/// most words with the branch name; all of them on a tie.
pub fn owners(branch_words: &[String], candidates: &[(usize, &str)]) -> Vec<usize> {
    let scored: Vec<(usize, usize)> = candidates
        .iter()
        .map(|(index, filename)| {
            let words = git::words(filename);
            let shared = branch_words
                .iter()
                .filter(|word| words.contains(word))
                .count();
            (*index, shared)
        })
        .collect();
    let best = scored.iter().map(|(_, shared)| *shared).max();
    scored
        .into_iter()
        .filter(|(_, shared)| Some(*shared) == best)
        .map(|(index, _)| index)
        .collect()
}

type Links<'a> = HashMap<(usize, u64), Vec<(&'a Repository, &'a Branch)>>;
type Loose<'a> = Vec<(&'a Repository, &'a Branch)>;

/// Each numbered branch on the issue it is for, and the worktrees no issue claims.
fn link<'a>(scope: &'a Scope, snapshots: &[Option<Snapshot>]) -> (Links<'a>, Loose<'a>) {
    let mut links: Links = HashMap::new();
    let mut loose = Vec::new();
    for repository in scope.unique_repositories() {
        let members = scope.members(repository);
        let shows_one = members.iter().any(|&index| scope.shown[index]);
        for branch in &repository.branches {
            let number = branch.issue.filter(|_| !branch.is_default);
            let candidates: Vec<(usize, &str)> = number
                .map(|number| {
                    members
                        .iter()
                        .filter_map(|&index| {
                            let issue = snapshots[index].as_ref()?.by_number.get(&number)?;
                            Some((index, issue.filename.as_str()))
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let (Some(number), false) = (number, candidates.is_empty()) {
                for index in owners(&branch.words, &candidates) {
                    links
                        .entry((index, number))
                        .or_default()
                        .push((repository.as_ref(), branch));
                }
                continue;
            }
            let worked_on = branch
                .worktree
                .as_ref()
                .is_some_and(|worktree| !branch.is_default || worktree.changes > 0);
            if shows_one && worked_on {
                loose.push((repository.as_ref(), branch));
            }
        }
    }
    (links, loose)
}

fn signal(branch: &Branch) -> WorkSignal {
    WorkSignal {
        checked_out: branch.worktree.is_some(),
        ahead: branch.ahead,
        committed: branch.committed,
    }
}

#[derive(Serialize)]
struct BranchView {
    name: String,
    repository: String,
    worktree: Option<String>,
    worktree_path: Option<String>,
    ahead: u32,
    changes: usize,
    active: i64,
    working: bool,
}

impl BranchView {
    fn of(repository: &Repository, branch: &Branch, now: Timestamp) -> Self {
        BranchView {
            name: branch.name.clone(),
            repository: repository.name(),
            worktree: branch.worktree.as_ref().map(git::Worktree::name),
            worktree_path: branch
                .worktree
                .as_ref()
                .map(|worktree| worktree.path.display().to_string()),
            ahead: branch.ahead,
            changes: branch
                .worktree
                .as_ref()
                .map_or(0, |worktree| worktree.changes),
            active: views::seconds(branch.active()),
            working: signal(branch).is_work(now),
        }
    }
}

#[derive(Serialize)]
struct Waiting<'a> {
    /// `#3`, or `owner/name#3` for an issue in another GitHub repository.
    label: String,
    href: String,
    title: &'a str,
}

#[derive(Serialize)]
struct Card<'a> {
    /// Unique on the page: the project or repository and the number.
    id: String,
    /// The issue file's project, or the GitHub repository.
    project: &'a str,
    href: String,
    /// On GitHub: opens in a new tab.
    external: bool,
    /// `issue`, or `pull` for a GitHub pull request no shown issue claims.
    kind: &'static str,
    /// Where a move goes; None for a card that cannot be moved.
    status_url: Option<String>,
    writable: bool,
    number: u64,
    title: &'a str,
    /// The labels other than status words: the column already says those.
    labels: Vec<&'a str>,
    is_open: bool,
    draft: bool,
    reason: Option<&'static str>,
    updated: i64,
    /// The last change to the file or to one of its branches.
    activity: i64,
    tasks_done: usize,
    tasks_total: usize,
    comment_count: usize,
    version: &'a str,
    placement: &'static str,
    blockers: Vec<Waiting<'a>>,
    branches: Vec<BranchView>,
    assignees: Vec<&'a str>,
    /// Open pull requests that close the issue.
    pulls: Vec<PullView<'a>>,
    /// The card's own facts when it is a pull request.
    pull: Option<PullView<'a>>,
}

#[derive(Serialize)]
struct ColumnView<'a> {
    key: &'static str,
    title: &'static str,
    hint: &'static str,
    needs_you: bool,
    total: usize,
    hidden: usize,
    cards: Vec<Card<'a>>,
}

fn card<'a>(
    project: &'a Project,
    snapshot: &'a Snapshot,
    issue: &'a Issue,
    branches: &[(&Repository, &Branch)],
    now: Timestamp,
) -> (Column, Card<'a>) {
    let signals: Vec<WorkSignal> = branches.iter().map(|(_, branch)| signal(branch)).collect();
    let (column, placement) = board::place(issue, &signals, now);
    let blockers = if issue.is_open {
        snapshot
            .open_dependencies(issue)
            .iter()
            .filter_map(|other| snapshot.by_number.get(&other.number))
            .map(|other| Waiting {
                label: format!("#{}", other.number),
                href: format!("/{}/issues/{}", project.name(), other.number),
                title: &other.title,
            })
            .collect()
    } else {
        Vec::new()
    };
    let mut branches: Vec<BranchView> = branches
        .iter()
        .map(|(repository, branch)| BranchView::of(repository, branch, now))
        .collect();
    branches.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.name.cmp(&b.name)));
    let updated = views::seconds(issue.updated);
    let writable = project.writable() && issue.editable();
    let href = format!("/{}/issues/{}", project.name(), issue.number);
    let card = Card {
        id: format!("{}-{}", project.name(), issue.number),
        project: project.name(),
        status_url: writable.then(|| format!("{href}/status")),
        href,
        external: false,
        kind: "issue",
        writable,
        number: issue.number,
        title: &issue.title,
        labels: issue
            .labels
            .iter()
            .map(String::as_str)
            .filter(|label| !board::is_status_label(label))
            .collect(),
        is_open: issue.is_open,
        draft: false,
        reason: issue.reason.map(|reason| reason.as_str()),
        updated,
        activity: branches
            .iter()
            .map(|branch| branch.active)
            .fold(updated, i64::max),
        tasks_done: issue.tasks_done,
        tasks_total: issue.tasks_total,
        comment_count: issue.comments().len(),
        version: &issue.version,
        placement: placement.as_str(),
        blockers,
        branches,
        assignees: Vec::new(),
        pulls: Vec::new(),
        pull: None,
    };
    (column, card)
}

fn board_query(names: &[String], extra: &[(&str, &str)]) -> String {
    let mut pairs: Vec<(&str, &str)> = names
        .iter()
        .map(|name| ("project", name.as_str()))
        .collect();
    pairs.extend_from_slice(extra);
    url_encoded(&pairs)
}

fn with_query(path: &str, query: &str) -> String {
    if query.is_empty() {
        path.to_string()
    } else {
        format!("{path}?{query}")
    }
}

/// `/-/board`, or with `only` the board of that one project, which is its tab.
pub fn page(app: &App, request: &Incoming, only: Option<&str>) -> Answer {
    let names = match only {
        Some(name) => vec![find_project(app, name)?.name().to_string()],
        None => chosen(app, request)?,
    };
    let scope = Scope::new(app, &names);
    let shown = match only {
        Some(_) => Shown::none(app),
        None => Shown::new(app, &names),
    };
    let signature = signature(&scope, &shown);
    let snapshots: Vec<Option<Snapshot>> = scope
        .projects
        .iter()
        .zip(&scope.needed)
        .map(|(project, needed)| needed.then(|| project.snapshot()))
        .collect();
    let (links, loose) = link(&scope, &snapshots);
    let now = Timestamp::now();

    let text = request.query("q").unwrap_or_default().trim().to_string();
    let parsed = query::parse(&text);
    let all_done = request.query("done") == Some("all");
    let mut columns: Vec<ColumnView> = Column::ALL
        .into_iter()
        .map(|column| ColumnView {
            key: column.key(),
            title: column.title(),
            hint: column.hint(),
            needs_you: column.needs_you(),
            total: 0,
            hidden: 0,
            cards: Vec::new(),
        })
        .collect();
    let mut active = 0;
    for (index, project) in scope.projects.iter().enumerate() {
        let (true, Some(snapshot)) = (scope.shown[index], &snapshots[index]) else {
            continue;
        };
        for issue in &snapshot.issues {
            if !query::matches(&parsed, issue, false) {
                continue;
            }
            let branches = links
                .get(&(index, issue.number))
                .map_or(&[][..], Vec::as_slice);
            let (column, card) = card(project, snapshot, issue, branches, now);
            if column != Column::Done && now.as_second() - card.activity <= LATELY_SECONDS {
                active += 1;
            }
            columns[column as usize].cards.push(card);
        }
    }
    for (column, card) in shown.cards(&parsed, now) {
        if column != Column::Done && now.as_second() - card.activity <= LATELY_SECONDS {
            active += 1;
        }
        columns[column as usize].cards.push(card);
    }
    for column in &mut columns {
        column.cards.sort_by(|a, b| {
            b.activity
                .cmp(&a.activity)
                .then_with(|| a.project.cmp(b.project))
                .then_with(|| a.number.cmp(&b.number))
        });
        column.total = column.cards.len();
        if column.key == Column::Done.key() && !all_done && column.total > DONE_LIMIT {
            column.hidden = column.total - DONE_LIMIT;
            column.cards.truncate(DONE_LIMIT);
        }
    }
    let total = |column: Column| columns[column as usize].total;
    let summary = context! {
        waiting => total(Column::Backlog) + total(Column::Review) + total(Column::Blocked),
        backlog => total(Column::Backlog),
        review => total(Column::Review),
        blocked => total(Column::Blocked),
        in_progress => total(Column::InProgress),
        active,
    };

    let path = match only {
        Some(name) => format!("/{name}/board"),
        None => "/-/board".to_string(),
    };
    let listed: &[String] = if only.is_some() { &[] } else { &names };
    let mut current: Vec<(&str, &str)> = Vec::new();
    if !text.is_empty() {
        current.push(("q", &text));
    }
    if all_done {
        current.push(("done", "all"));
    }
    let mut every_done = current.clone();
    if !all_done {
        every_done.push(("done", "all"));
    }
    let loose: Vec<BranchView> = loose
        .iter()
        .map(|(repository, branch)| BranchView::of(repository, branch, now))
        .collect();
    let pickable: Vec<Value> = scope
        .projects
        .iter()
        .map(|project| {
            context! {
                name => project.name(),
                chosen => names.iter().any(|name| name == project.name()),
            }
        })
        .collect();
    let heading = match (only, names.as_slice()) {
        (Some(_), _) => "Board".to_string(),
        (None, []) => "All projects".to_string(),
        (None, names) => names.join(", "),
    };
    let own = only.and_then(|_| {
        let index = scope.shown.iter().position(|shown| *shown)?;
        Some((&scope.projects[index], snapshots[index].as_ref()?))
    });
    let context = context! {
        section => "board",
        signature,
        version_url => with_query("/-/board/version", &board_query(&names, &[])),
        board_url => &path,
        back => with_query(&path, &board_query(listed, &current)),
        every_done_url => with_query(&path, &board_query(listed, &every_done)),
        clear_url => with_query(&path, &board_query(listed, &[])),
        names => listed.to_vec(),
        q => &text,
        all_done,
        multiple => scope.shown.iter().filter(|is_shown| **is_shown).count() + shown.count() > 1,
        heading,
        pickable,
        pickable_github => shown.pickable(&names),
        github => shown.notes(),
        columns => views::serialized(&columns),
        loose => views::serialized(&loose),
        summary,
        ..views::base(app, own.map(|(project, snapshot)| (project.as_ref(), snapshot)))
    };
    Ok(app.templates.render("board.html", context, StatusCode::OK))
}

/// What an open board polls to notice that an issue file or a branch changed.
pub fn version(app: &App, request: &Incoming) -> Answer {
    let names = chosen(app, request)?;
    let signature = signature(&Scope::new(app, &names), &Shown::new(app, &names));
    Ok(json(StatusCode::OK, json!({ "version": signature })))
}

/// Where to go after a move without JavaScript: a path on this site, never another site.
fn safe_back(back: Option<&str>) -> &str {
    match back {
        Some(path) if path.starts_with('/') && !path.starts_with("//") && !path.contains('\\') => {
            path
        }
        _ => "/-/board",
    }
}

/// Move an issue to a column: its status labels and state, written like any other edit.
pub fn status(app: &App, request: &Incoming, name: &str, number: u64) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let key = request.text("status");
    let column = Column::from_key(key).ok_or_else(|| {
        Failure::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            format!("There is no column called “{key}”."),
        )
    })?;
    let existing = project.snapshot().labels().to_vec();
    let edit = |issue: &Issue| board::moved(issue, column, &existing);
    let issue = match project.update(number, edit, writes::version(request)) {
        Ok(issue) => issue,
        Err(ProjectError::ReadOnly(_)) => return Err(read_only(&project)),
        Err(ProjectError::NotFound(_)) => return Err(no_issue(&project, number)),
        Err(ProjectError::Conflict(_)) => {
            return Err(Failure::new(
                StatusCode::CONFLICT,
                "This issue changed on disk after the board was loaded, so it was not moved. \
                 Reload the board.",
            ));
        }
        Err(ProjectError::Edit(message)) => {
            return Err(Failure::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                sentence(&format!("not moved: {message}")),
            ));
        }
        Err(ProjectError::Io(error)) => return Err(failed_write(error)),
    };
    if request.wants_json() {
        let now_in = board::stated_column(&issue).unwrap_or(column);
        return Ok(json(
            StatusCode::OK,
            json!({ "version": issue.version, "column": now_in.key() }),
        ));
    }
    Ok(redirect(
        StatusCode::SEE_OTHER,
        safe_back(request.field("back")),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(text: &str) -> Vec<String> {
        git::words(text)
    }

    #[test]
    fn a_branch_goes_to_the_issue_whose_file_name_shares_most_words_with_it() {
        let candidates = [
            (0, "004-kanban-board.md"),
            (1, "004-crash-on-empty-title.md"),
        ];
        assert_eq!(owners(&words("kanban-board"), &candidates), [0]);
        assert_eq!(owners(&words("crash-fix"), &candidates), [1]);
        // Nothing to tell them apart: both get it, rather than a guess.
        assert_eq!(owners(&words("misc"), &candidates), [0, 1]);
        assert_eq!(owners(&words("misc"), &candidates[..1]), [0]);
    }

    #[test]
    fn going_back_after_a_move_stays_on_this_site() {
        assert_eq!(safe_back(Some("/demo/board?q=x")), "/demo/board?q=x");
        for back in [
            None,
            Some(""),
            Some("https://example.com/"),
            Some("//example.com/"),
            Some("/\\example.com"),
            Some("demo/board"),
        ] {
            assert_eq!(safe_back(back), "/-/board", "{back:?}");
        }
    }
}
