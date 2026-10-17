//! The pages: what a GET shows.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use minijinja::context;
use minijinja::value::Value;
use serde_json::json;

use super::views::{self, Draft, Flash, IssueView, ProjectView, SnapshotView};
use super::{Answer, App, Failure, Incoming, redirect};
use crate::browse;
use crate::config::OpenRoot;
use crate::files;
use crate::issues::Issue;
use crate::project::{Project, Snapshot};
use crate::query::{self, DEFAULT};
use crate::render::{self, References};
use crate::text::casefold;
use crate::timefmt;

pub const PAGE_SIZE: usize = 25;
pub const TEMPLATE_FILE: &str = "TEMPLATE.md";
pub const DEFAULT_TEMPLATE: &str = "\
**Depends on:** – · **Blocks:** –

## Context

## Scope

## Acceptance criteria

- [ ]

## Definition of Done

- [ ] Tests pass (unit, smoke, e2e as applicable)
- [ ] Migration has a working `down`
- [ ] Published schema / OpenAPI regenerated and committed
- [ ] CHANGELOG updated
- [ ] No new runtime dependency without an explicit approval
- [ ] Nothing secret or personal in the diff — checked, not assumed
";

/// An error message as a sentence: capitalised, with a full stop.
pub fn sentence(text: &str) -> String {
    let text = text.trim();
    let mut chars = text.chars();
    let mut result: String = chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
    if !result.ends_with('.') {
        result.push('.');
    }
    result
}

pub fn url_encoded(pairs: &[(&str, &str)]) -> String {
    form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

pub fn find_project(app: &App, name: &str) -> Result<Arc<Project>, Failure> {
    app.registry.get(name).map_err(|_| {
        let mut message = format!("There is no project called “{name}”.");
        if app.settings.open_root.is_some() {
            message
                .push_str(" A directory picked with Open is forgotten when the viewer restarts.");
        }
        Failure::new(StatusCode::NOT_FOUND, message)
    })
}

pub fn read_only(project: &Project) -> Failure {
    Failure::new(
        StatusCode::FORBIDDEN,
        format!("{} is read-only.", project.name()),
    )
}

pub fn writable_project(app: &App, name: &str) -> Result<Arc<Project>, Failure> {
    let project = find_project(app, name)?;
    if !project.writable() {
        return Err(read_only(&project));
    }
    Ok(project)
}

pub fn no_issue(project: &Project, number: u64) -> Failure {
    Failure::new(
        StatusCode::NOT_FOUND,
        format!("There is no issue #{number} in {}.", project.name()),
    )
}

pub fn references(snapshot: &Snapshot) -> References {
    snapshot
        .issues
        .iter()
        .map(|issue| (issue.number, (issue.title.clone(), issue.is_open)))
        .collect()
}

fn page(app: &App, name: &str, context: Value, status: StatusCode) -> Answer {
    Ok(app.templates.render(name, context, status))
}

pub fn home(app: &App) -> Answer {
    let rows: Vec<Value> = app
        .registry
        .all()
        .iter()
        .map(|project| {
            let snapshot = project.snapshot();
            let latest = snapshot
                .issues
                .iter()
                .map(|issue| issue.updated)
                .max()
                .map(views::seconds);
            context! {
                project => views::serialized(ProjectView::of(project)),
                snapshot => views::serialized(SnapshotView::of(&snapshot)),
                latest,
            }
        })
        .collect();
    page(
        app,
        "projects.html",
        context! { rows, ..views::base(app, None) },
        StatusCode::OK,
    )
}

pub fn open_root(app: &App) -> Result<&OpenRoot, Failure> {
    app.settings.open_root.as_ref().ok_or_else(|| {
        Failure::new(
            StatusCode::NOT_FOUND,
            "Open is off: set OPEN_ROOT in .env and restart the viewer.",
        )
    })
}

pub fn browsed(root: &OpenRoot, relative: &str) -> Result<PathBuf, Failure> {
    browse::resolve(&root.path, relative)
        .map_err(|error| Failure::new(StatusCode::NOT_FOUND, sentence(&error.0)))
}

fn browse_url(relative: &str) -> String {
    if relative.is_empty() {
        "/-/open".into()
    } else {
        format!("/-/open?{}", url_encoded(&[("path", relative)]))
    }
}

fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn open(app: &App, request: &Incoming) -> Answer {
    let root = open_root(app)?;
    let directory = browsed(root, request.query("path").unwrap_or_default())?;
    let relative = browse::relative_to(&root.path, &directory);
    let parts: Vec<&str> = if relative.is_empty() {
        Vec::new()
    } else {
        relative.split('/').collect()
    };
    let projects = app.registry.all();
    let known: HashMap<PathBuf, &Arc<Project>> = projects
        .iter()
        .map(|project| (resolved(project.path()), project))
        .collect();
    let name_of = |path: &Path| {
        known
            .get(path)
            .map(|project| context! { name => project.name() })
    };
    let rows: Vec<Value> = browse::folders(&root.path, &directory)
        .into_iter()
        .map(|folder| {
            let project = name_of(&directory.join(&folder.name));
            context! { folder => views::serialized(&folder), project }
        })
        .collect();
    let context = context! {
        root => context! {
            path => root.path.display().to_string(),
            name => root.path.file_name().map(|name| name.to_string_lossy().into_owned()),
            read_only => root.read_only,
        },
        relative => &relative,
        crumbs => views::serialized(
            parts.iter().enumerate().map(|(index, part)| (*part, parts[..=index].join("/"))).collect::<Vec<_>>(),
        ),
        parent => (!parts.is_empty()).then(|| parts[..parts.len() - 1].join("/")),
        here => name_of(&directory),
        here_issues => browse::count_issues(&directory),
        rows,
        browse_url => Value::from_function(|relative: &str| browse_url(relative)),
        ..views::base(app, None)
    };
    page(app, "open.html", context, StatusCode::OK)
}

pub fn project_root(app: &App, name: &str) -> Answer {
    let project = find_project(app, name)?;
    Ok(redirect(
        StatusCode::FOUND,
        &format!("/{}/issues", project.name()),
    ))
}

/// Page numbers around the current one, with None where a gap goes.
pub fn page_items(page: usize, pages: usize) -> Vec<Option<usize>> {
    let mut wanted: Vec<usize> = [1, pages]
        .into_iter()
        .chain(page.saturating_sub(2)..=page + 2)
        .filter(|number| (1..=pages).contains(number))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut items = Vec::new();
    let mut previous = 0;
    for number in wanted {
        if number - previous > 1 {
            items.push(None);
        }
        items.push(Some(number));
        previous = number;
    }
    items
}

fn label_query(label: &str) -> String {
    format!("{DEFAULT} {}", query::label_token(label))
}

pub fn issues(app: &App, request: &Incoming, name: &str) -> Answer {
    let project = find_project(app, name)?;
    let signature = project.signature();
    let snapshot = project.snapshot();
    let text = request.query("q").unwrap_or(DEFAULT).to_string();
    let requested = match request.query("page") {
        None => 1,
        Some(value) => value
            .trim()
            .parse::<i64>()
            .map_err(|_| Failure::not_understood())?,
    };
    let parsed = query::parse(&text);
    let candidates: Vec<&Arc<Issue>> = snapshot
        .issues
        .iter()
        .filter(|issue| query::matches(&parsed, issue, true))
        .collect();
    let open_count = candidates.iter().filter(|issue| issue.is_open).count();
    let mut shown: Vec<&Arc<Issue>> = candidates
        .iter()
        .copied()
        .filter(|issue| query::state_matches(&parsed, issue))
        .collect();
    query::ordered(&mut shown, parsed.sort);
    let pages = shown.len().div_ceil(PAGE_SIZE).max(1);
    let current = usize::try_from(requested.max(1)).unwrap_or(1).min(pages);
    let rows: Vec<Value> = shown[(current - 1) * PAGE_SIZE..(current * PAGE_SIZE).min(shown.len())]
        .iter()
        .map(|issue| {
            let blockers = if issue.is_open {
                snapshot.open_dependencies(issue)
            } else {
                Vec::new()
            };
            views::serialized(IssueView::of(issue).with_blockers(&blockers))
        })
        .collect();

    let base_url = format!("/{}/issues", project.name());
    let query_url = move |new_query: &str, new_page: Option<usize>| -> String {
        let page = new_page
            .filter(|page| *page > 1)
            .map(|page| page.to_string());
        let mut pairs = vec![("q", new_query)];
        if let Some(page) = &page {
            pairs.push(("page", page));
        }
        format!("{base_url}?{}", url_encoded(&pairs))
    };
    let state = parsed.open.map(|open| if open { "open" } else { "closed" });
    let sort_label = query::SORTS
        .iter()
        .find(|(key, _)| *key == parsed.sort)
        .map_or("", |(_, label)| label);
    let (for_state, for_label, for_no_label, for_sort) =
        (text.clone(), text.clone(), text.clone(), text.clone());
    let labelled = parsed.clone();
    let context = context! {
        section => "issues",
        signature,
        q => &text,
        default_q => DEFAULT,
        query => context! { state, no_label => parsed.no_label, sort => parsed.sort },
        rows,
        open_count,
        closed_count => candidates.len() - open_count,
        page => current,
        pages,
        page_items => page_items(current, pages),
        query_url => Value::from_function(query_url),
        with_state => Value::from_function(move |state: &str| query::with_state(&for_state, state == "open")),
        with_label => Value::from_function(move |label: &str| query::with_label(&for_label, label)),
        with_no_label => Value::from_function(move || query::with_no_label(&for_no_label)),
        with_sort => Value::from_function(move |key: &str| query::with_sort(&for_sort, key)),
        has_label => Value::from_function(move |label: &str| query::has_label(&labelled, label)),
        label_query => Value::from_function(|label: &str| label_query(label)),
        sorts => query::SORTS.to_vec(),
        sort_label,
        ..views::base(app, Some((&project, &snapshot)))
    };
    page(app, "issues.html", context, StatusCode::OK)
}

fn issue_template(project: &Project) -> String {
    let path = project.path().join(TEMPLATE_FILE);
    if !path.is_file() {
        return DEFAULT_TEMPLATE.into();
    }
    files::read(&path).map_or_else(|_| DEFAULT_TEMPLATE.into(), |file| file.text)
}

pub fn new_issue(app: &App, name: &str) -> Answer {
    let project = writable_project(app, name)?;
    new_issue_page(app, &project, "", None, &[], None, StatusCode::OK)
}

pub fn new_issue_page(
    app: &App,
    project: &Project,
    title: &str,
    description: Option<&str>,
    chosen: &[String],
    error: Option<String>,
    status: StatusCode,
) -> Answer {
    let snapshot = project.snapshot();
    let mut all_labels: Vec<String> = snapshot.labels().iter().chain(chosen).cloned().collect();
    all_labels.sort_by_cached_key(|label| (casefold(label), label.clone()));
    all_labels.dedup();
    let context = context! {
        section => "issues",
        title,
        description => description.map_or_else(|| issue_template(project), str::to_string),
        chosen => chosen.to_vec(),
        all_labels,
        error,
        ..views::base(app, Some((project, &snapshot)))
    };
    page(app, "new_issue.html", context, status)
}

pub fn issue(app: &App, name: &str, number: u64) -> Answer {
    let project = find_project(app, name)?;
    issue_page(app, &project, number, None, None, StatusCode::OK)
}

/// The issue as it is on disk now, with a message and the unsaved text when a write failed.
pub fn issue_page(
    app: &App,
    project: &Project,
    number: u64,
    flash: Option<Flash>,
    draft: Option<Draft>,
    status: StatusCode,
) -> Answer {
    let snapshot = project.snapshot();
    let issue = snapshot
        .by_number
        .get(&number)
        .ok_or_else(|| no_issue(project, number))?;
    let zone = &app.settings.timezone;
    let refs = references(&snapshot);
    let can_write = project.writable() && issue.editable();
    let markdown = |text: &str, part: &str| {
        let context = render::Context {
            project: Some(project.name()),
            issues: Some(&refs),
            part: Some(part),
            interactive: can_write,
        };
        Value::from_safe_string(render::render(text, &context))
    };
    let comments: Vec<Value> = issue
        .comments()
        .iter()
        .map(|comment| {
            let text = issue.comment_text(comment);
            context! {
                comment => context! { index => comment.index, stamp => &comment.stamp, note => &comment.note },
                text,
                html => markdown(text, &comment.index.to_string()),
                when => comment.when().and_then(|when| timefmt::local(when, zone)).map(|when| when.timestamp().as_second()),
            }
        })
        .collect();
    let related = |numbers: &[u64]| -> Vec<(u64, Option<Value>)> {
        numbers
            .iter()
            .map(|n| {
                (
                    *n,
                    snapshot
                        .by_number
                        .get(n)
                        .map(|other| views::serialized(IssueView::of(other))),
                )
            })
            .collect()
    };
    let label_base = format!("/{}/issues", project.name());
    let context = context! {
        section => "issues",
        issue => views::serialized(IssueView::of(issue)),
        can_write,
        description_html => (!issue.description().is_empty()).then(|| markdown(issue.description(), "description")),
        comments,
        blocked_by => related(&issue.depends_on),
        blocking => related(&snapshot.blocking(issue.number)),
        label_url => Value::from_function(move |label: &str| {
            format!("{label_base}?{}", url_encoded(&[("q", &label_query(label))]))
        }),
        flash => flash.map(views::serialized),
        draft => draft.map_or_else(|| context! {}, views::serialized),
        ..views::base(app, Some((project, &snapshot)))
    };
    page(app, "issue.html", context, status)
}

pub fn labels(app: &App, name: &str) -> Answer {
    let project = find_project(app, name)?;
    let signature = project.signature();
    let snapshot = project.snapshot();
    let rows: Vec<Value> = snapshot
        .labels()
        .iter()
        .map(|label| {
            let tagged: Vec<&Arc<Issue>> = snapshot
                .issues
                .iter()
                .filter(|issue| issue.labels.contains(label))
                .collect();
            let open = tagged.iter().filter(|issue| issue.is_open).count();
            context! { name => label, open, closed => tagged.len() - open }
        })
        .collect();
    let label_base = format!("/{}/issues", project.name());
    let context = context! {
        section => "labels",
        signature,
        rows,
        label_url => Value::from_function(move |label: &str, state: &str| {
            let text = format!("is:issue is:{state} {}", query::label_token(label));
            format!("{label_base}?{}", url_encoded(&[("q", &text)]))
        }),
        ..views::base(app, Some((&project, &snapshot)))
    };
    page(app, "labels.html", context, StatusCode::OK)
}

/// What open pages poll to notice that a file changed on disk.
pub fn version(app: &App, request: &Incoming, name: &str) -> Answer {
    let project = find_project(app, name)?;
    let version = match request.query("issue") {
        None => Some(project.signature()),
        Some(value) => {
            let number = value
                .trim()
                .parse::<i64>()
                .map_err(|_| Failure::not_understood())?;
            u64::try_from(number)
                .ok()
                .and_then(|number| project.version_of(number).ok())
        }
    };
    Ok(Json(json!({ "version": version })).into_response())
}

pub fn json(status: StatusCode, value: serde_json::Value) -> Response {
    (status, Json(value)).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_items_show_the_ends_and_two_on_each_side_with_gaps() {
        assert_eq!(page_items(1, 1), [Some(1)]);
        assert_eq!(page_items(1, 3), [Some(1), Some(2), Some(3)]);
        assert_eq!(
            page_items(6, 12),
            [
                Some(1),
                None,
                Some(4),
                Some(5),
                Some(6),
                Some(7),
                Some(8),
                None,
                Some(12)
            ]
        );
        assert_eq!(
            page_items(2, 9),
            [Some(1), Some(2), Some(3), Some(4), None, Some(9)]
        );
    }

    #[test]
    fn an_error_becomes_a_sentence() {
        assert_eq!(sentence("a title is required"), "A title is required.");
        assert_eq!(sentence(" étoile. "), "Étoile.");
        assert_eq!(sentence(""), ".");
    }
}
