//! GitHub's issues and pull requests on the board, from what the hub read last. A card here links
//! to github.com and cannot be moved: the board only reads GitHub.

use std::collections::HashMap;
use std::sync::Arc;

use jiff::Timestamp;
use minijinja::context;
use minijinja::value::Value;
use serde::Serialize;

use super::{BranchView, Card, Waiting, signal};
use crate::board::{self, Column, WorkSignal};
use crate::files;
use crate::git::{Branch, Repository};
use crate::github::{self, Item, View};
use crate::query::{self, Query};
use crate::web::{App, views};

/// The GitHub repositories one board shows, as of the hub's last answer.
pub(super) struct Shown {
    view: Arc<View>,
    /// Indexes into `view.sources`.
    sources: Vec<usize>,
    /// The local checkout of each shown source, when git can read one.
    checkouts: Vec<Option<Arc<Repository>>>,
    /// Repositories the request names that GitHub did not list.
    missing: Vec<String>,
    on: bool,
}

impl Shown {
    /// Every GitHub repository when `names` is empty, the ones it names otherwise. Asking starts a
    /// refresh when the last one is old, so GitHub is read while a board is open.
    pub(super) fn new(app: &App, names: &[String]) -> Self {
        let named: Vec<&String> = names.iter().filter(|name| name.contains('/')).collect();
        let on = app.github.is_on() && (names.is_empty() || !named.is_empty());
        let view = app.github.view();
        if !on {
            return Shown {
                view,
                sources: Vec::new(),
                checkouts: Vec::new(),
                missing: Vec::new(),
                on,
            };
        }
        app.github.poke();
        let sources: Vec<usize> = (0..view.sources.len())
            .filter(|&index| {
                named.is_empty()
                    || named
                        .iter()
                        .any(|name| name.eq_ignore_ascii_case(&view.sources[index].name))
            })
            .collect();
        let missing = if view.is_reading() {
            Vec::new()
        } else {
            named
                .into_iter()
                .filter(|name| {
                    !view
                        .sources
                        .iter()
                        .any(|source| source.name.eq_ignore_ascii_case(name))
                })
                .cloned()
                .collect()
        };
        let checkouts = sources
            .iter()
            .map(|&index| {
                let checkout = view.sources[index].checkout.as_ref()?;
                app.git.repository(checkout)
            })
            .collect();
        Shown {
            view,
            sources,
            checkouts,
            missing,
            on,
        }
    }

    /// Nothing from GitHub: the board of one project.
    pub(super) fn none(app: &App) -> Self {
        Shown {
            view: app.github.view(),
            sources: Vec::new(),
            checkouts: Vec::new(),
            missing: Vec::new(),
            on: false,
        }
    }

    pub(super) fn count(&self) -> usize {
        self.sources.len()
    }

    /// Changes when GitHub's answer or a branch of a checkout does; empty when GitHub is not shown.
    pub(super) fn signature(&self) -> String {
        if !self.on {
            return String::new();
        }
        let mut listing = format!("{}\n", self.view.signature());
        for repository in self.checkouts.iter().flatten() {
            listing.push_str(&format!(
                "{}\0{}\n",
                repository.common_dir.display(),
                repository.signature()
            ));
        }
        files::digest(&listing)
    }

    fn data(&self) -> impl Iterator<Item = (usize, &github::RepoData)> {
        self.sources
            .iter()
            .enumerate()
            .filter_map(|(at, &index)| Some((at, self.view.sources[index].data.as_deref()?)))
    }

    /// The cards that match `query`: each issue with the open pull requests that close it, and the
    /// pull requests that close none of the shown issues on cards of their own.
    pub(super) fn cards(&self, query: &Query, now: Timestamp) -> Vec<(Column, Card<'_>)> {
        let issues: Vec<&Item> = self.data().flat_map(|(_, data)| &data.issues).collect();
        let mut by_key: HashMap<(String, u64), usize> = HashMap::new();
        for (index, issue) in issues.iter().enumerate() {
            by_key.entry(issue.reference().key()).or_insert(index);
        }
        let mut pulls: Vec<Vec<&Item>> = vec![Vec::new(); issues.len()];
        let mut alone: Vec<&Item> = Vec::new();
        for pull in self.data().flat_map(|(_, data)| &data.pulls) {
            let mut targets: Vec<usize> = pull
                .pull
                .iter()
                .flat_map(|facts| &facts.closes)
                .filter_map(|reference| by_key.get(&reference.key()).copied())
                .collect();
            targets.sort_unstable();
            targets.dedup();
            if targets.is_empty() {
                alone.push(pull);
            }
            for target in targets {
                pulls[target].push(pull);
            }
        }
        let mut branches: Vec<Vec<(&Repository, &Branch)>> = vec![Vec::new(); issues.len()];
        for (at, data) in self.data() {
            let Some(repository) = &self.checkouts[at] else {
                continue;
            };
            let name = data.name.to_lowercase();
            for branch in &repository.branches {
                let Some(number) = branch.issue.filter(|_| !branch.is_default) else {
                    continue;
                };
                if let Some(&index) = by_key.get(&(name.clone(), number)) {
                    branches[index].push((repository.as_ref(), branch));
                }
            }
        }
        let mut cards = Vec::new();
        for (index, issue) in issues.iter().enumerate() {
            if query::matches(query, *issue, false) {
                cards.push(card(issue, &pulls[index], &branches[index], now));
            }
        }
        for pull in alone {
            if query::matches(query, pull, false) {
                cards.push(card(pull, &[], &[], now));
            }
        }
        cards
    }

    /// What the board says about GitHub itself: reading, errors and limits. None when not shown.
    pub(super) fn notes(&self) -> Option<Value> {
        if !self.on {
            return None;
        }
        let shown = || self.sources.iter().map(|&index| &self.view.sources[index]);
        let errors: Vec<Value> = shown()
            .filter_map(|source| {
                let message = source.error.as_deref()?;
                Some(context! {
                    name => &source.name,
                    message,
                    kept => source.data.is_some(),
                })
            })
            .collect();
        let mut limits: Vec<String> = Vec::new();
        for data in shown().filter_map(|source| source.data.as_deref()) {
            if data.open_total > data.open_read {
                limits.push(format!(
                    "{} has {} open issues; the board shows the {} most recently updated.",
                    data.name, data.open_total, data.open_read
                ));
            }
            if data.pulls_total > data.pulls.len() {
                limits.push(format!(
                    "{} has {} open pull requests; the board shows the {} most recently updated.",
                    data.name,
                    data.pulls_total,
                    data.pulls.len()
                ));
            }
        }
        Some(context! {
            reading => self.view.is_reading(),
            read => self.view.read.map(views::seconds),
            errors,
            problems => &self.view.problems,
            limits,
            missing => &self.missing,
        })
    }

    /// The repositories for the Projects menu.
    pub(super) fn pickable(&self, names: &[String]) -> Vec<Value> {
        self.view
            .sources
            .iter()
            .map(|source| {
                context! {
                    name => &source.name,
                    chosen => names.iter().any(|name| name.eq_ignore_ascii_case(&source.name)),
                }
            })
            .collect()
    }
}

/// A pull request as a line on a card.
#[derive(Serialize)]
pub(super) struct PullView<'a> {
    number: u64,
    /// `#14`, or `owner/name#14` from another repository than the card's.
    label: String,
    url: &'a str,
    title: &'a str,
    draft: bool,
    branch: &'a str,
    review: Option<&'static str>,
    review_key: Option<String>,
    checks: Option<&'static str>,
}

impl<'a> PullView<'a> {
    fn of(item: &'a Item, card_repository: &str) -> Option<Self> {
        let pull = item.pull.as_ref()?;
        Some(PullView {
            number: item.number,
            label: reference_label(&item.repository, item.number, card_repository),
            url: &item.url,
            title: &item.title,
            draft: pull.draft,
            branch: &pull.branch,
            review: pull.review.map(github::Review::as_str),
            review_key: pull.review.map(|review| review.as_str().replace(' ', "-")),
            checks: pull.checks.map(github::Checks::as_str),
        })
    }
}

fn reference_label(repository: &str, number: u64, card_repository: &str) -> String {
    if repository.eq_ignore_ascii_case(card_repository) {
        format!("#{number}")
    } else {
        format!("{repository}#{number}")
    }
}

fn card<'a>(
    item: &'a Item,
    pulls: &[&'a Item],
    branches: &[(&'a Repository, &'a Branch)],
    now: Timestamp,
) -> (Column, Card<'a>) {
    let signals: Vec<WorkSignal> = branches.iter().map(|(_, branch)| signal(branch)).collect();
    let (column, placement) = github::place(item, pulls, &signals, now);
    let mut branches: Vec<BranchView> = branches
        .iter()
        .map(|(repository, branch)| BranchView::of(repository, branch, now))
        .collect();
    branches.sort_by(|a, b| b.active.cmp(&a.active).then_with(|| a.name.cmp(&b.name)));
    let updated = views::seconds(item.updated);
    let activity = branches
        .iter()
        .map(|branch| branch.active)
        .chain(pulls.iter().map(|pull| views::seconds(pull.updated)))
        .fold(updated, i64::max);
    let card = Card {
        id: format!("github/{}/{}", item.repository, item.number),
        project: &item.repository,
        href: item.url.clone(),
        external: true,
        kind: if item.pull.is_some() { "pull" } else { "issue" },
        status_url: None,
        writable: false,
        number: item.number,
        title: &item.title,
        labels: item
            .labels
            .iter()
            .map(String::as_str)
            .filter(|label| !board::is_status_label(label))
            .collect(),
        is_open: item.is_open,
        draft: item.pull.as_ref().is_some_and(|pull| pull.draft),
        reason: item.reason.map(|reason| reason.as_str()),
        updated,
        activity,
        tasks_done: 0,
        tasks_total: 0,
        comment_count: item.comments,
        version: "",
        placement: placement.as_str(),
        blockers: item
            .blocked_by
            .iter()
            .map(|blocker| Waiting {
                label: reference_label(
                    &blocker.reference.repository,
                    blocker.reference.number,
                    &item.repository,
                ),
                href: blocker.url.clone(),
                title: &blocker.title,
            })
            .collect(),
        branches,
        assignees: item.assignees.iter().map(String::as_str).collect(),
        pulls: pulls
            .iter()
            .filter_map(|pull| PullView::of(pull, &item.repository))
            .collect(),
        pull: PullView::of(item, &item.repository),
    };
    (column, card)
}
