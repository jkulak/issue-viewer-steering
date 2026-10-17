//! The forms: what a POST changes, after checking that the viewer's own page sent it.

use axum::http::StatusCode;
use jiff::Zoned;
use serde_json::json;

use super::pages::{
    browsed, find_project, issue_page, json, new_issue_page, no_issue, open_root, read_only,
    references, sentence, writable_project,
};
use super::views::{Draft, Flash};
use super::{Answer, App, Failure, Incoming, redirect};
use crate::browse;
use crate::frontmatter::{render_list, render_scalar, style_of};
use crate::issuefile::{self, EditError, STAMP_FORMAT, parse, set_front};
use crate::issues::{Issue, STATE_DONE, STATE_NOT_PLANNED, STATE_OPEN, normalise_labels};
use crate::project::{Project, ProjectError};
use crate::render;
use crate::text::{casefold, collapse, is_blank};

pub const TITLE_LIMIT: usize = 256;
pub const LABEL_LIMIT: usize = 50;

fn close_action(action: &str) -> Option<&'static str> {
    match action {
        "close" => Some(STATE_DONE),
        "close-not-planned" => Some(STATE_NOT_PLANNED),
        "reopen" => Some(STATE_OPEN),
        _ => None,
    }
}

fn same_length_equal(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .fold(0u8, |difference, (x, y)| difference | (x ^ y))
            == 0
}

/// Refuse a write that did not come from one of the viewer's own pages.
pub(super) fn same_origin(app: &App, request: &Incoming) -> Result<(), Failure> {
    let forbidden = |message: &str| Failure::new(StatusCode::FORBIDDEN, message);
    if let Some(site) = request.header("sec-fetch-site")
        && site != "same-origin"
        && site != "none"
    {
        return Err(forbidden(
            "A page on another site tried to change something here.",
        ));
    }
    if let Some(origin) = request.header("origin") {
        let authority = origin.split_once("://").map_or("", |(_, rest)| {
            rest.split(['/', '?', '#']).next().unwrap_or("")
        });
        if origin == "null" || Some(authority) != request.header("host").as_deref() {
            return Err(forbidden(
                "A request from another origin cannot change anything here.",
            ));
        }
    }
    let token = request
        .header("x-csrf-token")
        .or_else(|| request.field("_csrf").map(str::to_string));
    if !token.is_some_and(|token| same_length_equal(token.as_bytes(), app.csrf_token().as_bytes()))
    {
        return Err(forbidden(
            "This page is out of date (the viewer restarted after it was loaded). Reload it.",
        ));
    }
    Ok(())
}

fn clean_title(title: &str) -> Result<String, EditError> {
    let clean = collapse(title);
    if clean.is_empty() {
        return Err(EditError("a title is required".into()));
    }
    if clean.chars().count() > TITLE_LIMIT {
        return Err(EditError(format!(
            "a title can have at most {TITLE_LIMIT} characters"
        )));
    }
    Ok(clean)
}

/// The labels picked in a form, each written the way the project already writes it.
fn clean_labels(raw: &[&str], existing: &[String]) -> Result<Vec<String>, EditError> {
    let mut chosen = Vec::new();
    for value in raw {
        let name = collapse(value);
        if name.is_empty() {
            continue;
        }
        if name.contains(',') {
            return Err(EditError(format!(
                "a label cannot contain a comma: “{name}”"
            )));
        }
        if name.chars().count() > LABEL_LIMIT {
            return Err(EditError(format!(
                "a label can have at most {LABEL_LIMIT} characters"
            )));
        }
        let folded = casefold(&name);
        let known = existing.iter().find(|label| casefold(label) == folded);
        chosen.push(yaml_rust2::Yaml::String(known.cloned().unwrap_or(name)));
    }
    Ok(normalise_labels(Some(&yaml_rust2::Yaml::Array(chosen))))
}

fn back(project: &Project, number: u64, anchor: &str) -> Answer {
    Ok(redirect(
        StatusCode::SEE_OTHER,
        &format!("/{}/issues/{number}{anchor}", project.name()),
    ))
}

pub(super) fn failed_write(error: std::io::Error) -> Failure {
    tracing::error!("writing failed: {error}");
    Failure::new(
        StatusCode::INTERNAL_SERVER_ERROR,
        format!("The file could not be written: {error}."),
    )
}

/// Apply an edit, or answer with the page that says why it was not saved.
fn apply(
    app: &App,
    project: &Project,
    number: u64,
    edit: impl Fn(&Issue) -> Result<String, EditError>,
    expected_version: Option<&str>,
    draft: Option<Draft>,
) -> Result<Result<Issue, Answer>, Failure> {
    match project.update(number, edit, expected_version) {
        Ok(issue) => Ok(Ok(issue)),
        Err(ProjectError::ReadOnly(_)) => Err(read_only(project)),
        Err(ProjectError::NotFound(_)) => Err(no_issue(project, number)),
        Err(ProjectError::Conflict(_)) => {
            let flash = Flash {
                kind: "warn",
                text: "This issue changed on disk after you opened it, so nothing was saved. \
                       Below is the latest version, with your text kept in the editor."
                    .into(),
            };
            Ok(Err(issue_page(
                app,
                project,
                number,
                Some(flash),
                draft,
                StatusCode::CONFLICT,
            )))
        }
        Err(ProjectError::Edit(message)) => {
            let flash = Flash {
                kind: "error",
                text: format!("Not saved: {message}."),
            };
            Ok(Err(issue_page(
                app,
                project,
                number,
                Some(flash),
                draft,
                StatusCode::UNPROCESSABLE_ENTITY,
            )))
        }
        Err(ProjectError::Io(error)) => Err(failed_write(error)),
    }
}

pub(super) fn version(request: &Incoming) -> Option<&str> {
    request
        .field("version")
        .filter(|version| !version.is_empty())
}

/// Show a directory under the open root as a project, until the viewer restarts.
pub fn open(app: &App, request: &Incoming) -> Answer {
    same_origin(app, request)?;
    let root = open_root(app)?;
    let directory = browsed(root, request.text("path"))?;
    let relative = browse::relative_to(&root.path, &directory);
    let project = app.registry.open(
        &directory,
        |taken| browse::project_name(&root.path, &relative, |name| taken.contains(name)),
        root.read_only,
    );
    Ok(redirect(
        StatusCode::SEE_OTHER,
        &format!("/{}/issues", project.name()),
    ))
}

pub fn create(app: &App, request: &Incoming, name: &str) -> Answer {
    same_origin(app, request)?;
    let project = writable_project(app, name)?;
    let (title, description, labels) = (
        request.text("title"),
        request.text("description"),
        request.fields("labels"),
    );
    let created = clean_title(title)
        .and_then(|title| Ok((title, clean_labels(&labels, project.snapshot().labels())?)))
        .map_err(ProjectError::from)
        .and_then(|(title, chosen)| project.create(&title, &chosen, description));
    match created {
        Ok(issue) => Ok(redirect(
            StatusCode::SEE_OTHER,
            &format!("/{}/issues/{}", project.name(), issue.number),
        )),
        Err(ProjectError::ReadOnly(_)) => Err(read_only(&project)),
        Err(ProjectError::Io(error)) => Err(failed_write(error)),
        Err(error) => {
            let chosen: Vec<String> = labels
                .iter()
                .filter(|label| !is_blank(label))
                .map(|label| label.to_string())
                .collect();
            let message = sentence(&error.to_string());
            new_issue_page(
                app,
                &project,
                title,
                Some(description),
                &chosen,
                Some(message),
                StatusCode::UNPROCESSABLE_ENTITY,
            )
        }
    }
}

pub fn title(app: &App, request: &Incoming, name: &str, number: u64) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let title = request.text("title");
    let edit = |issue: &Issue| issuefile::set_title(&issue.file, &clean_title(title)?);
    match apply(
        app,
        &project,
        number,
        edit,
        None,
        Some(Draft::new("title", title)),
    )? {
        Ok(_) => back(&project, number, ""),
        Err(answer) => answer,
    }
}

pub fn description(app: &App, request: &Incoming, name: &str, number: u64) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let text = request.text("text");
    let edit = |issue: &Issue| issuefile::set_description(&issue.file, text);
    match apply(
        app,
        &project,
        number,
        edit,
        version(request),
        Some(Draft::new("description", text)),
    )? {
        Ok(_) => back(&project, number, ""),
        Err(answer) => answer,
    }
}

pub fn labels(app: &App, request: &Incoming, name: &str, number: u64) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let existing = project.snapshot().labels().to_vec();
    let raw = request.fields("labels");
    let edit = |issue: &Issue| {
        set_front(
            &issue.file,
            "labels",
            &render_list(&clean_labels(&raw, &existing)?),
        )
    };
    match apply(app, &project, number, edit, None, None)? {
        Ok(_) => back(&project, number, ""),
        Err(answer) => answer,
    }
}

pub fn comment(app: &App, request: &Incoming, name: &str, number: u64) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let text = request.text("text");
    let action = request.field("action").unwrap_or("comment");
    let stamp = Zoned::now()
        .with_time_zone(app.settings.timezone.clone())
        .strftime(STAMP_FORMAT)
        .to_string();
    let has_text = !is_blank(text);
    let edit = |issue: &Issue| -> Result<String, EditError> {
        let mut new = issue.file.text.clone();
        if has_text {
            new = issuefile::add_comment(&issue.file, &stamp, text)?;
        } else if close_action(action).is_none() {
            return Err(EditError("write a comment first".into()));
        }
        if let Some(state) = close_action(action) {
            let parsed = parse(&new);
            let style = style_of(parsed.front.raw(&new), "state");
            new = set_front(&parsed, "state", &render_scalar(state, style))?;
        }
        Ok(new)
    };
    match apply(
        app,
        &project,
        number,
        edit,
        None,
        Some(Draft::new("new-comment", text)),
    )? {
        Ok(issue) => {
            let anchor = match issue.comments().last() {
                Some(last) if has_text => format!("#comment-{}", last.index),
                _ => String::new(),
            };
            back(&project, number, &anchor)
        }
        Err(answer) => answer,
    }
}

pub fn edit_comment(app: &App, request: &Incoming, name: &str, number: u64, index: u64) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let text = request.text("text");
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    let edit = |issue: &Issue| issuefile::edit_comment(&issue.file, index, text);
    let draft = Draft::new(format!("comment-{index}"), text);
    match apply(app, &project, number, edit, version(request), Some(draft))? {
        Ok(_) => back(&project, number, &format!("#comment-{index}")),
        Err(answer) => answer,
    }
}

pub fn delete_comment(
    app: &App,
    request: &Incoming,
    name: &str,
    number: u64,
    index: u64,
) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let index = usize::try_from(index).unwrap_or(usize::MAX);
    let edit = |issue: &Issue| issuefile::delete_comment(&issue.file, index);
    match apply(app, &project, number, edit, version(request), None)? {
        Ok(_) => back(&project, number, "#comments"),
        Err(answer) => answer,
    }
}

/// Tick or untick one checkbox; the page's script calls it and shows the new counts.
pub fn task(app: &App, request: &Incoming, name: &str, number: u64) -> Answer {
    same_origin(app, request)?;
    let line = match request.field("line") {
        None => -1,
        Some(value) => value
            .trim()
            .parse::<i64>()
            .map_err(|_| Failure::not_understood())?,
    };
    let project = find_project(app, name)?;
    let part = request.text("part");
    let tick = matches!(request.text("checked"), "1" | "true" | "on");
    let edit = |issue: &Issue| issuefile::set_task(&issue.file, part, line, tick);
    let error = |status: StatusCode, message: String| Ok(json(status, json!({ "error": message })));
    match project.update(number, edit, version(request)) {
        Ok(issue) => Ok(json(
            StatusCode::OK,
            json!({ "version": issue.version, "done": issue.tasks_done, "total": issue.tasks_total }),
        )),
        Err(ProjectError::ReadOnly(_)) => error(
            StatusCode::FORBIDDEN,
            format!("{} is read-only.", project.name()),
        ),
        Err(ProjectError::NotFound(_)) => error(
            StatusCode::NOT_FOUND,
            format!("There is no issue #{number}."),
        ),
        Err(ProjectError::Conflict(_)) => error(
            StatusCode::CONFLICT,
            "This issue changed on disk. Reload to see the latest version first.".into(),
        ),
        Err(ProjectError::Edit(message)) => {
            error(StatusCode::UNPROCESSABLE_ENTITY, sentence(&message))
        }
        Err(ProjectError::Io(error)) => Err(failed_write(error)),
    }
}

pub fn preview(app: &App, request: &Incoming, name: &str) -> Answer {
    same_origin(app, request)?;
    let project = find_project(app, name)?;
    let refs = references(&project.snapshot());
    let context = render::Context {
        project: Some(project.name()),
        issues: Some(&refs),
        ..Default::default()
    };
    Ok(json(
        StatusCode::OK,
        json!({ "html": render::render(request.text("text"), &context) }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_title_is_collapsed_required_and_limited() {
        assert_eq!(
            clean_title("  Saving \n without  a title ").unwrap(),
            "Saving without a title"
        );
        assert_eq!(clean_title("   ").unwrap_err().0, "a title is required");
        assert!(clean_title(&"x".repeat(TITLE_LIMIT)).is_ok());
        assert_eq!(
            clean_title(&"x".repeat(TITLE_LIMIT + 1)).unwrap_err().0,
            "a title can have at most 256 characters"
        );
    }

    #[test]
    fn labels_match_the_existing_spelling_and_skip_blanks_and_repeats() {
        let existing = vec!["ui".to_string(), "Needs Design".to_string()];
        let chosen =
            clean_labels(&["UI", " needs  design ", " ", "new one", "ui"], &existing).unwrap();
        assert_eq!(chosen, ["ui", "Needs Design", "new one"]);
    }

    #[test]
    fn a_label_with_a_comma_or_too_long_is_refused() {
        assert_eq!(
            clean_labels(&["a,b"], &[]).unwrap_err().0,
            "a label cannot contain a comma: “a,b”"
        );
        assert!(clean_labels(&[&"x".repeat(LABEL_LIMIT + 1)], &[]).is_err());
    }

    #[test]
    fn tokens_compare_in_full() {
        assert!(same_length_equal(b"abc", b"abc"));
        assert!(!same_length_equal(b"abc", b"abd"));
        assert!(!same_length_equal(b"abc", b"abcd"));
        assert!(!same_length_equal(b"", b"a"));
    }

    #[test]
    fn only_the_three_close_actions_change_the_state() {
        assert_eq!(close_action("close"), Some("done"));
        assert_eq!(close_action("close-not-planned"), Some("not-planned"));
        assert_eq!(close_action("reopen"), Some("open"));
        assert_eq!(close_action("comment"), None);
    }
}
