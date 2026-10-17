//! Issues and pull requests on GitHub, read with the `gh` CLI for the board.
//!
//! The viewer never holds a token: `gh` is already signed in on this machine and makes the calls.
//! It only reads, with one GraphQL query per repository, and the answers are kept in a `Hub` that
//! a board asks for a refresh; a board never waits for GitHub.

use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use serde::Deserialize;

use crate::board::{self, Column, Placement, WorkSignal};
use crate::config::{GithubEntry, Settings};
use crate::files;
use crate::issues::Reason;
use crate::process::{self, Failed};
use crate::query::Searchable;
use crate::text::casefold;

/// How many open issues one query reads; a repository with more says so on the board. It also
/// reads the 20 most recently closed issues, for Done, and 50 open pull requests.
pub const OPEN_LIMIT: usize = 100;
/// Owners with more repositories than this are better named one repository at a time.
const LISTING_LIMIT: usize = 100;
/// A GraphQL query of a large repository takes a few seconds; a stuck network should not hold a
/// refresh for longer than this.
const LIMIT: Duration = Duration::from_secs(20);
/// Repositories read at the same time.
const PARALLEL: usize = 4;
/// What the board shows of a message from `gh`.
const COMPLAINT_LIMIT: usize = 300;

pub const QUERY: &str = "\
query($owner: String!, $name: String!) {
  repository(owner: $owner, name: $name) {
    nameWithOwner
    open: issues(states: OPEN, first: 100, orderBy: {field: UPDATED_AT, direction: DESC}) {
      totalCount
      nodes { ...card }
    }
    closed: issues(states: CLOSED, first: 20, orderBy: {field: UPDATED_AT, direction: DESC}) {
      nodes { ...card }
    }
    pullRequests(states: OPEN, first: 50, orderBy: {field: UPDATED_AT, direction: DESC}) {
      totalCount
      nodes {
        number title url isDraft headRefName updatedAt reviewDecision
        author { login }
        assignees(first: 10) { nodes { login } }
        labels(first: 20) { nodes { name } }
        comments { totalCount }
        closingIssuesReferences(first: 10) { nodes { number repository { nameWithOwner } } }
        commits(last: 1) { nodes { commit { statusCheckRollup { state } } } }
      }
    }
  }
}
fragment card on Issue {
  number title url state stateReason updatedAt
  author { login }
  assignees(first: 10) { nodes { login } }
  labels(first: 20) { nodes { name } }
  comments { totalCount }
  blockedBy(first: 10) { nodes { number title url state repository { nameWithOwner } } }
}
";

/// An issue named by its repository and number.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Reference {
    pub repository: String,
    pub number: u64,
}

impl Reference {
    /// The key to look it up by: GitHub compares repository names without regard to case.
    pub fn key(&self) -> (String, u64) {
        (self.repository.to_lowercase(), self.number)
    }
}

/// An open issue another one waits for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    pub reference: Reference,
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Review {
    Required,
    Approved,
    ChangesRequested,
}

impl Review {
    pub fn as_str(self) -> &'static str {
        match self {
            Review::Required => "review required",
            Review::Approved => "approved",
            Review::ChangesRequested => "changes requested",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Checks {
    Pending,
    Passing,
    Failing,
}

impl Checks {
    pub fn as_str(self) -> &'static str {
        match self {
            Checks::Pending => "pending",
            Checks::Passing => "passing",
            Checks::Failing => "failing",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pull {
    pub draft: bool,
    pub branch: String,
    pub review: Option<Review>,
    pub checks: Option<Checks>,
    /// The issues it closes when it is merged.
    pub closes: Vec<Reference>,
}

impl Pull {
    /// An open pull request waits for a review or a merge, unless it is still being worked on: a
    /// draft, or one with changes requested.
    pub fn column(&self) -> Column {
        if self.draft || self.review == Some(Review::ChangesRequested) {
            Column::InProgress
        } else {
            Column::Review
        }
    }
}

/// An issue, or with `pull` a pull request.
#[derive(Debug, Clone)]
pub struct Item {
    pub repository: String,
    pub number: u64,
    pub title: String,
    pub url: String,
    pub labels: Vec<String>,
    pub is_open: bool,
    pub reason: Option<Reason>,
    pub updated: Timestamp,
    pub author: Option<String>,
    pub assignees: Vec<String>,
    pub comments: usize,
    pub blocked_by: Vec<Blocker>,
    pub pull: Option<Pull>,
    search_text: String,
}

impl Item {
    pub fn reference(&self) -> Reference {
        Reference {
            repository: self.repository.clone(),
            number: self.number,
        }
    }
}

impl Searchable for Item {
    fn is_open(&self) -> bool {
        self.is_open
    }
    fn raw_state(&self) -> &str {
        if self.is_open { "open" } else { "closed" }
    }
    fn reason(&self) -> Option<Reason> {
        self.reason
    }
    fn labels(&self) -> &[String] {
        &self.labels
    }
    fn search_text(&self) -> &str {
        &self.search_text
    }
}

/// What one query said about a repository.
#[derive(Debug, Clone)]
pub struct RepoData {
    /// `owner/name` as GitHub spells it.
    pub name: String,
    /// Open issues first, then the most recently closed.
    pub issues: Vec<Item>,
    pub pulls: Vec<Item>,
    pub open_total: usize,
    pub open_read: usize,
    pub pulls_total: usize,
    /// Changes when anything in the answer does.
    pub digest: String,
}

// The shape of the GraphQL answer; every list may hold a null for something deleted meanwhile.

#[derive(Deserialize)]
struct Answer {
    data: Option<Data>,
    #[serde(default)]
    errors: Vec<Problem>,
}

#[derive(Deserialize)]
struct Problem {
    message: String,
}

#[derive(Deserialize)]
struct Data {
    repository: Option<RepositoryNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryNode {
    name_with_owner: String,
    open: Connection<IssueNode>,
    closed: Connection<IssueNode>,
    pull_requests: Connection<PullNode>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Connection<T> {
    #[serde(default)]
    total_count: usize,
    #[serde(default = "Vec::new")]
    nodes: Vec<Option<T>>,
}

impl<T> Connection<T> {
    fn present(self) -> impl Iterator<Item = T> {
        self.nodes.into_iter().flatten()
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Count {
    total_count: usize,
}

#[derive(Deserialize)]
struct Login {
    login: String,
}

#[derive(Deserialize)]
struct Named {
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RepositoryName {
    name_with_owner: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReferenceNode {
    number: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    state: Option<String>,
    repository: RepositoryName,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct IssueNode {
    number: u64,
    title: String,
    url: String,
    state: String,
    state_reason: Option<String>,
    #[serde(deserialize_with = "timestamp")]
    updated_at: Timestamp,
    author: Option<Login>,
    assignees: Connection<Login>,
    labels: Connection<Named>,
    comments: Count,
    blocked_by: Option<Connection<ReferenceNode>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PullNode {
    number: u64,
    title: String,
    url: String,
    is_draft: bool,
    head_ref_name: String,
    #[serde(deserialize_with = "timestamp")]
    updated_at: Timestamp,
    review_decision: Option<String>,
    author: Option<Login>,
    assignees: Connection<Login>,
    labels: Connection<Named>,
    comments: Count,
    closing_issues_references: Connection<ReferenceNode>,
    commits: Connection<CommitNode>,
}

#[derive(Deserialize)]
struct CommitNode {
    commit: Commit,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Commit {
    status_check_rollup: Option<Rollup>,
}

#[derive(Deserialize)]
struct Rollup {
    state: String,
}

fn timestamp<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Timestamp, D::Error> {
    let text = String::deserialize(deserializer)?;
    text.parse().map_err(serde::de::Error::custom)
}

fn reason_of(state_reason: Option<&str>) -> Reason {
    match state_reason {
        Some("NOT_PLANNED" | "DUPLICATE") => Reason::NotPlanned,
        _ => Reason::Completed,
    }
}

fn review_of(decision: Option<&str>) -> Option<Review> {
    match decision? {
        "APPROVED" => Some(Review::Approved),
        "CHANGES_REQUESTED" => Some(Review::ChangesRequested),
        "REVIEW_REQUIRED" => Some(Review::Required),
        _ => None,
    }
}

fn checks_of(state: &str) -> Option<Checks> {
    match state {
        "SUCCESS" => Some(Checks::Passing),
        "FAILURE" | "ERROR" => Some(Checks::Failing),
        "PENDING" | "EXPECTED" => Some(Checks::Pending),
        _ => None,
    }
}

fn reference(node: &ReferenceNode) -> Reference {
    Reference {
        repository: node.repository.name_with_owner.clone(),
        number: node.number,
    }
}

fn search_text(item: &Item) -> String {
    let mut text = format!("#{} {}\n{}\n", item.number, item.title, item.repository);
    for person in item.author.iter().chain(&item.assignees) {
        text.push_str(person);
        text.push('\n');
    }
    if let Some(pull) = &item.pull {
        text.push_str(&pull.branch);
    }
    casefold(&text)
}

fn with_search_text(mut item: Item) -> Item {
    item.search_text = search_text(&item);
    item
}

fn issue(repository: &str, node: IssueNode) -> Item {
    let is_open = node.state == "OPEN";
    let blocked_by = node
        .blocked_by
        .map(|connection| {
            connection
                .present()
                .filter(|blocker| blocker.state.as_deref() == Some("OPEN"))
                .map(|blocker| Blocker {
                    reference: reference(&blocker),
                    title: blocker.title,
                    url: blocker.url,
                })
                .collect()
        })
        .unwrap_or_default();
    with_search_text(Item {
        repository: repository.to_string(),
        number: node.number,
        title: node.title,
        url: node.url,
        labels: node.labels.present().map(|label| label.name).collect(),
        is_open,
        reason: (!is_open).then(|| reason_of(node.state_reason.as_deref())),
        updated: node.updated_at,
        author: node.author.map(|author| author.login),
        assignees: node.assignees.present().map(|login| login.login).collect(),
        comments: node.comments.total_count,
        blocked_by,
        pull: None,
        search_text: String::new(),
    })
}

fn pull(repository: &str, node: PullNode) -> Item {
    let checks = node
        .commits
        .present()
        .last()
        .and_then(|last| last.commit.status_check_rollup)
        .and_then(|rollup| checks_of(&rollup.state));
    with_search_text(Item {
        repository: repository.to_string(),
        number: node.number,
        title: node.title,
        url: node.url,
        labels: node.labels.present().map(|label| label.name).collect(),
        is_open: true,
        reason: None,
        updated: node.updated_at,
        author: node.author.map(|author| author.login),
        assignees: node.assignees.present().map(|login| login.login).collect(),
        comments: node.comments.total_count,
        blocked_by: Vec::new(),
        pull: Some(Pull {
            draft: node.is_draft,
            branch: node.head_ref_name,
            review: review_of(node.review_decision.as_deref()),
            checks,
            closes: node
                .closing_issues_references
                .present()
                .map(|node| reference(&node))
                .collect(),
        }),
        search_text: String::new(),
    })
}

/// The repository in an answer to `QUERY`.
pub fn parse(json: &[u8]) -> Result<RepoData, String> {
    let answer: Answer = serde_json::from_slice(json)
        .map_err(|_| "GitHub's answer was not understood".to_string())?;
    if let Some(problem) = answer.errors.first() {
        return Err(problem.message.clone());
    }
    let repository = answer
        .data
        .and_then(|data| data.repository)
        .ok_or_else(|| {
            "GitHub has no such repository, or this account cannot see it".to_string()
        })?;
    let name = repository.name_with_owner;
    let open_total = repository.open.total_count;
    let pulls_total = repository.pull_requests.total_count;
    let mut issues: Vec<Item> = repository
        .open
        .present()
        .map(|node| issue(&name, node))
        .collect();
    let open_read = issues.len();
    issues.extend(repository.closed.present().map(|node| issue(&name, node)));
    let pulls = repository
        .pull_requests
        .present()
        .map(|node| pull(&name, node))
        .collect();
    Ok(RepoData {
        name,
        issues,
        pulls,
        open_total,
        open_read,
        pulls_total,
        digest: files::digest(&String::from_utf8_lossy(json)),
    })
}

/// The repositories in an answer to `gh repo list --json nameWithOwner`.
pub fn parse_listing(json: &[u8]) -> Result<Vec<String>, String> {
    let listed: Vec<RepositoryName> = serde_json::from_slice(json)
        .map_err(|_| "the list of repositories was not understood".to_string())?;
    Ok(listed
        .into_iter()
        .map(|repository| repository.name_with_owner)
        .collect())
}

/// The column of a GitHub card, given the open pull requests that close it and the branches of a
/// local checkout named after it. Labels mean what they mean for an issue file, a closed issue is
/// done, and the column further right wins.
pub fn place(
    item: &Item,
    closing: &[&Item],
    work: &[WorkSignal],
    now: Timestamp,
) -> (Column, Placement) {
    if !item.is_open {
        return (Column::Done, Placement::Stated);
    }
    let stated = item
        .labels
        .iter()
        .filter_map(|label| board::column_named(label))
        .max();
    if let Some(pull) = &item.pull {
        return (
            stated.unwrap_or(pull.column()).max(pull.column()),
            Placement::Stated,
        );
    }
    let from_pulls = closing
        .iter()
        .filter_map(|other| other.pull.as_ref())
        .map(Pull::column)
        .max();
    match (stated, from_pulls) {
        (Some(stated), Some(pulls)) if pulls > stated => (pulls, Placement::PullRequest),
        (Some(stated), _) => (stated, Placement::Stated),
        (None, Some(pulls)) => (pulls, Placement::PullRequest),
        (None, None) if work.iter().any(|signal| signal.is_work(now)) => {
            (Column::InProgress, Placement::Inferred)
        }
        (None, None) => (Column::Backlog, Placement::Undecided),
    }
}

/// The first lines of what `gh` wrote to stderr, short enough for the board.
fn complaint(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    let joined = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    if joined.is_empty() {
        return "gh failed without saying why".to_string();
    }
    match joined.char_indices().nth(COMPLAINT_LIMIT) {
        Some((cut, _)) => format!("{}…", &joined[..cut]),
        None => joined,
    }
}

/// A repository on the board, with the last answer about it.
#[derive(Debug, Clone)]
pub struct Source {
    /// `owner/name` as configured, or as `gh repo list` named it.
    pub name: String,
    pub checkout: Option<PathBuf>,
    /// The last answer that could be read; kept when a later refresh fails.
    pub data: Option<Arc<RepoData>>,
    /// Why the last refresh could not read it.
    pub error: Option<String>,
    /// Found through `owner/*` rather than named.
    pub listed: bool,
}

/// Everything the board shows about GitHub, as of one refresh.
#[derive(Debug, Default)]
pub struct View {
    pub sources: Vec<Source>,
    /// Owners whose repositories could not be listed.
    pub problems: Vec<String>,
    /// When the last refresh finished; None until the first one has.
    pub read: Option<Timestamp>,
    signature: String,
}

impl View {
    fn new(sources: Vec<Source>, problems: Vec<String>, read: Option<Timestamp>) -> Self {
        let mut listing = format!("{}\n", read.is_some());
        for source in &sources {
            listing.push_str(&format!(
                "{}\0{}\0{}\n",
                source.name,
                source.data.as_ref().map_or("", |data| data.digest.as_str()),
                source.error.as_deref().unwrap_or_default(),
            ));
        }
        for problem in &problems {
            listing.push_str(problem);
            listing.push('\n');
        }
        View {
            sources,
            problems,
            read,
            signature: files::digest(&listing),
        }
    }

    /// Changes when an answer or an error does; not when a refresh only finds the same.
    pub fn signature(&self) -> &str {
        &self.signature
    }

    pub fn is_reading(&self) -> bool {
        self.read.is_none()
    }
}

struct State {
    view: Arc<View>,
    running: bool,
    finished: Option<Instant>,
}

/// The GitHub repositories of `GITHUB_REPOS` and what was last read about them.
pub struct Hub {
    entries: Vec<GithubEntry>,
    program: PathBuf,
    every: Duration,
    state: Mutex<State>,
    /// Held for a whole refresh, so two never overlap and a later one sees the earlier's result.
    refreshing: Mutex<()>,
}

/// Lets the next poke start a refresh even when this one panicked.
struct Running<'a>(&'a Hub);

impl Drop for Running<'_> {
    fn drop(&mut self) {
        let mut state = self.0.lock();
        state.running = false;
        state.finished = Some(Instant::now());
    }
}

impl Hub {
    pub fn new(settings: &Settings) -> Self {
        let sources = settings
            .github
            .iter()
            .filter(|entry| entry.name.is_some())
            .map(|entry| Source {
                name: entry.full_name(),
                checkout: entry.checkout.clone(),
                data: None,
                error: None,
                listed: false,
            })
            .collect();
        Hub {
            entries: settings.github.clone(),
            program: settings.gh.clone(),
            every: Duration::from_secs(settings.github_refresh_seconds),
            state: Mutex::new(State {
                view: Arc::new(View::new(sources, Vec::new(), None)),
                running: false,
                finished: None,
            }),
            refreshing: Mutex::new(()),
        }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn is_on(&self) -> bool {
        !self.entries.is_empty()
    }

    pub fn view(&self) -> Arc<View> {
        self.lock().view.clone()
    }

    /// Whether `owner/name` is one of the configured repositories, or of a configured owner's.
    pub fn knows(&self, name: &str) -> bool {
        let Some((owner, repository)) = name.split_once('/') else {
            return false;
        };
        self.entries.iter().any(|entry| {
            entry.owner.eq_ignore_ascii_case(owner)
                && entry
                    .name
                    .as_deref()
                    .is_none_or(|named| named.eq_ignore_ascii_case(repository))
        })
    }

    /// Start a refresh in the background when the last one is older than `GITHUB_REFRESH_SECONDS`
    /// and none is running. Boards call this whenever they ask for a version, so GitHub is read
    /// only while someone is looking.
    pub fn poke(self: &Arc<Self>) {
        if !self.is_on() {
            return;
        }
        {
            let mut state = self.lock();
            let fresh = state
                .finished
                .is_some_and(|finished| finished.elapsed() < self.every);
            if state.running || fresh {
                return;
            }
            state.running = true;
        }
        let hub = Arc::clone(self);
        std::thread::spawn(move || hub.refresh());
    }

    /// Read every repository again, now; the board keeps showing the last answer meanwhile.
    pub fn refresh(&self) {
        let _serial = self
            .refreshing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let _running = Running(self);
        let previous = self.view();
        let mut wanted: Vec<(String, Option<PathBuf>, bool)> = Vec::new();
        let mut add = |name: String, checkout: Option<PathBuf>, listed: bool| {
            if !wanted
                .iter()
                .any(|(known, _, _)| known.eq_ignore_ascii_case(&name))
            {
                wanted.push((name, checkout, listed));
            }
        };
        // Named repositories first, so one named with a checkout keeps it when its owner is
        // listed too.
        for entry in self.entries.iter().filter(|entry| entry.name.is_some()) {
            add(entry.full_name(), entry.checkout.clone(), false);
        }
        let mut problems = Vec::new();
        for entry in self.entries.iter().filter(|entry| entry.name.is_none()) {
            match self.list(&entry.owner) {
                Ok(names) => names.into_iter().for_each(|name| add(name, None, true)),
                Err(message) => {
                    tracing::warn!("github: listing {}: {message}", entry.owner);
                    problems.push(format!(
                        "Could not list the repositories of {}: {message}",
                        entry.owner
                    ));
                    let owner = format!("{}/", entry.owner.to_lowercase());
                    for source in &previous.sources {
                        if source.listed && source.name.to_lowercase().starts_with(&owner) {
                            add(source.name.clone(), None, true);
                        }
                    }
                }
            }
        }
        let names: Vec<&str> = wanted.iter().map(|(name, _, _)| name.as_str()).collect();
        let results = self.fetch_all(&names);
        let sources = wanted
            .into_iter()
            .zip(results)
            .map(|((name, checkout, listed), result)| {
                let (data, error) = match result {
                    Ok(data) => (Some(Arc::new(data)), None),
                    Err(message) => {
                        tracing::warn!("github: reading {name}: {message}");
                        let kept = previous
                            .sources
                            .iter()
                            .find(|source| source.name.eq_ignore_ascii_case(&name))
                            .and_then(|source| source.data.clone());
                        (kept, Some(message))
                    }
                };
                Source {
                    name,
                    checkout,
                    data,
                    error,
                    listed,
                }
            })
            .collect();
        let view = View::new(sources, problems, Some(Timestamp::now()));
        self.lock().view = Arc::new(view);
    }

    fn fetch_all(&self, names: &[&str]) -> Vec<Result<RepoData, String>> {
        let next = AtomicUsize::new(0);
        let results: Mutex<Vec<Option<Result<RepoData, String>>>> =
            Mutex::new(names.iter().map(|_| None).collect());
        std::thread::scope(|scope| {
            for _ in 0..PARALLEL.min(names.len()) {
                scope.spawn(|| {
                    loop {
                        let index = next.fetch_add(1, Ordering::Relaxed);
                        let Some(name) = names.get(index) else { break };
                        let result = self.fetch(name);
                        results
                            .lock()
                            .unwrap_or_else(|poisoned| poisoned.into_inner())[index] = Some(result);
                    }
                });
            }
        });
        results
            .into_inner()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .into_iter()
            .map(|result| result.unwrap_or_else(|| Err("it was not read".to_string())))
            .collect()
    }

    fn fetch(&self, full_name: &str) -> Result<RepoData, String> {
        let (owner, name) = full_name
            .split_once('/')
            .ok_or_else(|| "not owner/name".to_string())?;
        let out = self.gh(&[
            "api",
            "graphql",
            "-f",
            &format!("query={QUERY}"),
            "-f",
            &format!("owner={owner}"),
            "-f",
            &format!("name={name}"),
        ])?;
        parse(&out)
    }

    fn list(&self, owner: &str) -> Result<Vec<String>, String> {
        let out = self.gh(&[
            "repo",
            "list",
            owner,
            "--source",
            "--no-archived",
            "--limit",
            &LISTING_LIMIT.to_string(),
            "--json",
            "nameWithOwner",
        ])?;
        parse_listing(&out)
    }

    /// Run `gh` without a terminal: no prompts, no update notices, no colours.
    fn gh(&self, args: &[&str]) -> Result<Vec<u8>, String> {
        let mut command = Command::new(&self.program);
        command
            .args(args)
            .env("GH_PROMPT_DISABLED", "1")
            .env("GH_NO_UPDATE_NOTIFIER", "1")
            .env("GH_NO_EXTENSION_UPDATE_NOTIFIER", "1")
            .env("GH_SPINNER_DISABLED", "1")
            .env("NO_COLOR", "1");
        match process::run(&mut command, LIMIT) {
            Ok(finished) if finished.success => Ok(finished.stdout),
            Ok(finished) => Err(complaint(&finished.stderr)),
            Err(Failed::TimedOut) => Err(format!(
                "gh did not answer within {} seconds",
                LIMIT.as_secs()
            )),
            Err(Failed::Start(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                Err("gh is not installed, or not on the PATH the viewer was started with".into())
            }
            Err(Failed::Start(error)) => Err(format!("gh could not be started: {error}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn answer(
        open: serde_json::Value,
        closed: serde_json::Value,
        pulls: serde_json::Value,
    ) -> Vec<u8> {
        let total = open.as_array().map_or(0, Vec::len);
        json!({"data": {"repository": {
            "nameWithOwner": "acme/notes",
            "open": {"totalCount": total, "nodes": open},
            "closed": {"nodes": closed},
            "pullRequests": {"totalCount": pulls.as_array().map_or(0, Vec::len), "nodes": pulls},
        }}})
        .to_string()
        .into_bytes()
    }

    fn issue_node(number: u64, state: &str, labels: &[&str]) -> serde_json::Value {
        json!({
            "number": number, "title": format!("Issue {number}"),
            "url": format!("https://github.com/acme/notes/issues/{number}"),
            "state": state, "stateReason": null, "updatedAt": "2026-10-01T12:00:00Z",
            "author": {"login": "dev-ada"},
            "assignees": {"nodes": [{"login": "dev-bo"}]},
            "labels": {"nodes": labels.iter().map(|name| json!({"name": name})).collect::<Vec<_>>()},
            "comments": {"totalCount": 2},
            "blockedBy": {"nodes": []},
        })
    }

    fn pull_node(
        number: u64,
        draft: bool,
        review: Option<&str>,
        closes: &[u64],
    ) -> serde_json::Value {
        json!({
            "number": number, "title": format!("Pull {number}"),
            "url": format!("https://github.com/acme/notes/pull/{number}"),
            "isDraft": draft, "headRefName": format!("{number}-work"),
            "updatedAt": "2026-10-02T08:30:00Z", "reviewDecision": review,
            "author": null, "assignees": {"nodes": []}, "labels": {"nodes": []},
            "comments": {"totalCount": 0},
            "closingIssuesReferences": {"nodes": closes.iter().map(|n| json!({
                "number": n, "repository": {"nameWithOwner": "acme/notes"}
            })).collect::<Vec<_>>()},
            "commits": {"nodes": [{"commit": {"statusCheckRollup": {"state": "FAILURE"}}}]},
        })
    }

    #[test]
    fn an_answer_is_read_into_issues_and_pull_requests() {
        let mut blocked = issue_node(3, "OPEN", &["Blocked"]);
        blocked["blockedBy"] = json!({"nodes": [
            {"number": 1, "title": "Issue 1", "url": "https://github.com/acme/notes/issues/1",
             "state": "OPEN", "repository": {"nameWithOwner": "acme/notes"}},
            {"number": 2, "title": "Issue 2", "url": "https://github.com/acme/notes/issues/2",
             "state": "CLOSED", "repository": {"nameWithOwner": "acme/notes"}},
        ]});
        let mut dropped = issue_node(2, "CLOSED", &[]);
        dropped["stateReason"] = json!("NOT_PLANNED");
        let data = parse(&answer(
            json!([issue_node(1, "OPEN", &["bug"]), blocked, null]),
            json!([dropped]),
            json!([pull_node(4, true, Some("CHANGES_REQUESTED"), &[1])]),
        ))
        .unwrap();
        assert_eq!(data.name, "acme/notes");
        assert_eq!((data.open_total, data.open_read), (3, 2));
        let numbers: Vec<u64> = data.issues.iter().map(|item| item.number).collect();
        assert_eq!(numbers, [1, 3, 2]);
        let first = &data.issues[0];
        assert_eq!(first.labels, ["bug"]);
        assert_eq!(first.author.as_deref(), Some("dev-ada"));
        assert_eq!(first.assignees, ["dev-bo"]);
        assert_eq!(first.comments, 2);
        assert_eq!(first.updated.to_string(), "2026-10-01T12:00:00Z");
        assert_eq!(
            data.issues[1].blocked_by.len(),
            1,
            "only open blockers count"
        );
        assert_eq!(data.issues[1].blocked_by[0].reference.number, 1);
        assert_eq!(data.issues[2].reason, Some(Reason::NotPlanned));
        let pull = data.pulls[0].pull.as_ref().unwrap();
        assert!(pull.draft);
        assert_eq!(pull.branch, "4-work");
        assert_eq!(pull.review, Some(Review::ChangesRequested));
        assert_eq!(pull.checks, Some(Checks::Failing));
        assert_eq!(pull.closes[0].key(), ("acme/notes".to_string(), 1));
        assert!(data.pulls[0].search_text().contains("4-work"));
    }

    #[test]
    fn an_answer_without_the_repository_or_with_errors_is_an_error() {
        let missing = br#"{"data": {"repository": null}}"#;
        assert!(parse(missing).unwrap_err().contains("no such repository"));
        let failed = br#"{"data": null, "errors": [{"message": "Something went wrong"}]}"#;
        assert_eq!(parse(failed).unwrap_err(), "Something went wrong");
        assert!(parse(b"not json").is_err());
    }

    #[test]
    fn the_digest_changes_with_the_answer() {
        let one = parse(&answer(
            json!([issue_node(1, "OPEN", &[])]),
            json!([]),
            json!([]),
        ))
        .unwrap();
        let two = parse(&answer(
            json!([issue_node(1, "OPEN", &["bug"])]),
            json!([]),
            json!([]),
        ))
        .unwrap();
        assert_ne!(one.digest, two.digest);
    }

    #[test]
    fn a_listing_gives_the_repository_names() {
        let names = parse_listing(br#"[{"nameWithOwner":"acme/a"},{"nameWithOwner":"acme/b"}]"#);
        assert_eq!(names.unwrap(), ["acme/a", "acme/b"]);
        assert!(parse_listing(b"{}").is_err());
    }

    fn item(labels: &[&str], open: bool, pull: Option<Pull>) -> Item {
        with_search_text(Item {
            repository: "acme/notes".into(),
            number: 1,
            title: "Thing".into(),
            url: String::new(),
            labels: labels.iter().map(|label| label.to_string()).collect(),
            is_open: open,
            reason: (!open).then_some(Reason::Completed),
            updated: Timestamp::UNIX_EPOCH,
            author: None,
            assignees: Vec::new(),
            comments: 0,
            blocked_by: Vec::new(),
            pull,
            search_text: String::new(),
        })
    }

    fn pull_request(draft: bool, review: Option<Review>) -> Item {
        item(
            &[],
            true,
            Some(Pull {
                draft,
                branch: "1-thing".into(),
                review,
                checks: None,
                closes: Vec::new(),
            }),
        )
    }

    #[test]
    fn an_open_pull_request_puts_its_issue_in_review_or_in_progress() {
        let now = Timestamp::UNIX_EPOCH;
        let plain = item(&["bug"], true, None);
        let ready = pull_request(false, Some(Review::Required));
        let draft = pull_request(true, None);
        let changes = pull_request(false, Some(Review::ChangesRequested));
        let from_pull = |column| (column, Placement::PullRequest);
        assert_eq!(
            place(&plain, &[&ready], &[], now),
            from_pull(Column::Review)
        );
        assert_eq!(
            place(&plain, &[&draft], &[], now),
            from_pull(Column::InProgress)
        );
        assert_eq!(
            place(&plain, &[&changes], &[], now),
            from_pull(Column::InProgress)
        );
        assert_eq!(
            place(&plain, &[&draft, &ready], &[], now),
            from_pull(Column::Review)
        );
        assert_eq!(
            place(&plain, &[], &[], now),
            (Column::Backlog, Placement::Undecided)
        );
    }

    #[test]
    fn a_label_further_right_than_the_pull_request_wins_and_a_closed_issue_is_done() {
        let now = Timestamp::UNIX_EPOCH;
        let ready = pull_request(false, None);
        let blocked = item(&["Blocked"], true, None);
        assert_eq!(
            place(&blocked, &[&ready], &[], now),
            (Column::Blocked, Placement::Stated)
        );
        let in_review = item(&["in review"], true, None);
        assert_eq!(
            place(&in_review, &[&ready], &[], now),
            (Column::Review, Placement::Stated)
        );
        let closed = item(&["in-progress"], false, None);
        assert_eq!(
            place(&closed, &[&ready], &[], now),
            (Column::Done, Placement::Stated)
        );
    }

    #[test]
    fn a_branch_being_worked_on_puts_an_issue_without_a_status_in_progress() {
        let now = Timestamp::from_second(1_800_000_000).unwrap();
        let plain = item(&[], true, None);
        let checked_out = WorkSignal {
            checked_out: true,
            ahead: 0,
            committed: Timestamp::UNIX_EPOCH,
        };
        assert_eq!(
            place(&plain, &[], &[checked_out], now),
            (Column::InProgress, Placement::Inferred)
        );
    }

    #[test]
    fn a_pull_request_card_is_placed_by_its_own_state_and_labels() {
        let now = Timestamp::UNIX_EPOCH;
        assert_eq!(
            place(&pull_request(false, None), &[], &[], now).0,
            Column::Review
        );
        assert_eq!(
            place(&pull_request(true, None), &[], &[], now).0,
            Column::InProgress
        );
        let mut blocked = pull_request(true, None);
        blocked.labels = vec!["on hold".into()];
        assert_eq!(place(&blocked, &[], &[], now).0, Column::Blocked);
    }

    #[test]
    fn what_gh_complains_about_is_shortened_to_its_first_lines() {
        assert_eq!(
            complaint(b"\nHTTP 401: Bad credentials\n  (https://api.github.com/graphql)\n"),
            "HTTP 401: Bad credentials (https://api.github.com/graphql)"
        );
        assert_eq!(complaint(b""), "gh failed without saying why");
        let long = "x".repeat(COMPLAINT_LIMIT + 10);
        assert_eq!(
            complaint(long.as_bytes()).chars().count(),
            COMPLAINT_LIMIT + 1
        );
    }

    #[test]
    fn a_repository_is_known_when_it_or_its_owner_is_configured() {
        let settings = Settings {
            github: crate::config::parse_github("acme/notes,other-org/*").unwrap(),
            ..Settings::default()
        };
        let hub = Hub::new(&settings);
        assert!(hub.knows("acme/notes"));
        assert!(hub.knows("ACME/Notes"));
        assert!(hub.knows("other-org/anything"));
        assert!(!hub.knows("acme/other"));
        assert!(!hub.knows("notes"));
        assert!(hub.view().is_reading());
        assert_eq!(hub.view().sources.len(), 1);
    }

    #[test]
    fn the_query_reads_as_many_open_issues_as_the_limit_says() {
        assert!(QUERY.contains(&format!("issues(states: OPEN, first: {OPEN_LIMIT},")));
    }
}
