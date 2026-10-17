//! The templates and what the pages hand them.
//!
//! The templates see plain values made here, never the project's own types, so a page shows only
//! what it was given and the templates read the same whichever way the data is stored.

use std::fmt::Write as _;

use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use jiff::Timestamp;
use jiff::tz::TimeZone;
use minijinja::syntax::SyntaxConfig;
use minijinja::value::{Serde, Value};
use minijinja::{AutoEscape, Environment, Error, ErrorKind, Output, State, context};
use serde::Serialize;

use super::{App, assets};
use crate::issues::Issue;
use crate::project::{Project, Snapshot};
use crate::{icons, labels, timefmt};

const TEMPLATES: [(&str, &str); 10] = [
    ("_macros.html", include_str!("../../templates/_macros.html")),
    ("base.html", include_str!("../../templates/base.html")),
    ("board.html", include_str!("../../templates/board.html")),
    ("error.html", include_str!("../../templates/error.html")),
    ("issue.html", include_str!("../../templates/issue.html")),
    ("issues.html", include_str!("../../templates/issues.html")),
    ("labels.html", include_str!("../../templates/labels.html")),
    (
        "new_issue.html",
        include_str!("../../templates/new_issue.html"),
    ),
    ("open.html", include_str!("../../templates/open.html")),
    (
        "projects.html",
        include_str!("../../templates/projects.html"),
    ),
];

/// Escape the way Jinja2 does, so the pages come out as they always have: `/` stays as it is.
fn escape_html(out: &mut Output, text: &str) -> std::fmt::Result {
    let mut start = 0;
    for (position, char) in text.char_indices() {
        let entity = match char {
            '&' => "&amp;",
            '<' => "&lt;",
            '>' => "&gt;",
            '"' => "&#34;",
            '\'' => "&#39;",
            _ => continue,
        };
        out.write_str(&text[start..position])?;
        out.write_str(entity)?;
        start = position + 1;
    }
    out.write_str(&text[start..])
}

fn format_value(out: &mut Output, state: &mut State, value: &Value) -> Result<(), Error> {
    if value.is_undefined() || value.is_none() {
        return Ok(());
    }
    if value.is_safe() || *state.auto_escape() == AutoEscape::None {
        return minijinja::escape_formatter(out, state, value);
    }
    match value.as_str() {
        Some(text) => escape_html(out, text),
        None => escape_html(out, &value.to_string()),
    }
    .map_err(Error::from)
}

fn icon(name: &str, size: Option<u32>, class: Option<&str>) -> Result<Value, Error> {
    icons::icon(name, size.unwrap_or(16), class.unwrap_or_default())
        .map(Value::from_safe_string)
        .ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidOperation,
                format!("there is no icon called {name:?}"),
            )
        })
}

pub struct Templates {
    environment: Environment<'static>,
}

impl Templates {
    pub fn new() -> Self {
        let mut environment = Environment::new();
        environment.set_syntax(
            SyntaxConfig::builder()
                .trim_blocks(true)
                .lstrip_blocks(true)
                .build()
                .expect("a valid syntax"),
        );
        environment.set_auto_escape_callback(|_| AutoEscape::Html);
        environment.set_formatter(format_value);
        for (name, source) in TEMPLATES {
            environment
                .add_template(name, source)
                .expect("the built-in templates parse");
        }
        environment.add_function("icon", icon);
        environment.add_function("progress", |done: usize, total: usize| {
            Value::from_safe_string(icons::progress(done, total))
        });
        environment.add_function("label_style", |name: &str| labels::style(name));
        environment.add_function("label_color", |name: &str| labels::color(name));
        environment.add_function("avatar_style", |name: &str| labels::avatar_style(name));
        environment.add_global("asset_version", assets::VERSION.as_str());
        Templates { environment }
    }

    pub fn render(&self, name: &str, context: Value, status: StatusCode) -> Response {
        let rendered = self
            .environment
            .get_template(name)
            .and_then(|template| template.render(context));
        match rendered {
            Ok(html) => (
                status,
                [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
                html,
            )
                .into_response(),
            Err(error) => {
                let mut detail = format!("{error:#}");
                let mut source = std::error::Error::source(&error);
                while let Some(cause) = source {
                    let _ = write!(detail, ": {cause}");
                    source = cause.source();
                }
                tracing::error!("rendering {name}: {detail}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "The page could not be rendered; the log says why.",
                )
                    .into_response()
            }
        }
    }
}

/// A value built from a view struct; it is turned into template values at once, so it may borrow.
pub fn serialized<T: Serialize>(value: T) -> Value {
    Value::from(Serde(value))
}

pub fn seconds(moment: Timestamp) -> i64 {
    moment.as_second()
}

/// What every page gets: the token for its forms, the header, and the date helpers.
pub fn base(app: &App, project: Option<(&Project, &Snapshot)>) -> Value {
    let now = Timestamp::now();
    let zone: TimeZone = app.settings.timezone.clone();
    let absolute_zone = zone.clone();
    context! {
        csrf_token => app.csrf_token(),
        refresh_seconds => app.settings.refresh_seconds,
        app_version => env!("CARGO_PKG_VERSION"),
        can_open => app.settings.open_root.is_some(),
        project => project.map(|(project, _)| serialized(ProjectView::of(project))),
        snapshot => project.map(|(_, snapshot)| serialized(SnapshotView::of(snapshot))),
        open_total => project.map_or(0, |(_, snapshot)| snapshot.open_count()),
        label_total => project.map_or(0, |(_, snapshot)| snapshot.labels().len()),
        ago => Value::from_function(move |moment: i64| -> String {
            Timestamp::from_second(moment).map_or_else(|_| String::new(), |moment| timefmt::relative(moment, now, &zone))
        }),
        absolute => Value::from_function(move |moment: i64| -> String {
            Timestamp::from_second(moment).map_or_else(|_| String::new(), |moment| timefmt::absolute(moment, &absolute_zone))
        }),
    }
}

#[derive(Serialize)]
pub struct ProjectView<'a> {
    pub name: &'a str,
    pub path: String,
    pub writable: bool,
    pub exists: bool,
    pub opened: bool,
}

impl<'a> ProjectView<'a> {
    pub fn of(project: &'a Project) -> Self {
        ProjectView {
            name: project.name(),
            path: project.path().display().to_string(),
            writable: project.writable(),
            exists: project.exists(),
            opened: project.config.opened,
        }
    }
}

#[derive(Serialize)]
pub struct SnapshotView<'a> {
    pub labels: &'a [String],
    pub duplicates: &'a [String],
    pub total: usize,
    pub open_count: usize,
    pub closed_count: usize,
}

impl<'a> SnapshotView<'a> {
    pub fn of(snapshot: &'a Snapshot) -> Self {
        SnapshotView {
            labels: snapshot.labels(),
            duplicates: &snapshot.duplicates,
            total: snapshot.issues.len(),
            open_count: snapshot.open_count(),
            closed_count: snapshot.closed_count(),
        }
    }
}

#[derive(Serialize)]
pub struct Blocker<'a> {
    pub number: u64,
    pub title: &'a str,
}

#[derive(Serialize)]
pub struct IssueView<'a> {
    pub number: u64,
    pub title: &'a str,
    pub labels: &'a [String],
    pub state: &'a str,
    pub is_open: bool,
    pub reason: Option<&'static str>,
    pub state_is_custom: bool,
    pub updated: i64,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub problems: &'a [String],
    pub filename: &'a str,
    pub path: String,
    pub version: &'a str,
    pub comment_count: usize,
    pub description: &'a str,
    /// The open issues this one waits for, shown in the issue list.
    pub blockers: Vec<Blocker<'a>>,
}

impl<'a> IssueView<'a> {
    pub fn of(issue: &'a Issue) -> Self {
        IssueView {
            number: issue.number,
            title: &issue.title,
            labels: &issue.labels,
            state: &issue.state,
            is_open: issue.is_open,
            reason: issue.reason.map(|reason| reason.as_str()),
            state_is_custom: issue.state_is_custom(),
            updated: seconds(issue.updated),
            tasks_done: issue.tasks_done,
            tasks_total: issue.tasks_total,
            problems: &issue.problems,
            filename: &issue.filename,
            path: issue.path.display().to_string(),
            version: &issue.version,
            comment_count: issue.comments().len(),
            description: issue.description(),
            blockers: Vec::new(),
        }
    }

    pub fn with_blockers(mut self, blockers: &'a [std::sync::Arc<Issue>]) -> Self {
        self.blockers = blockers
            .iter()
            .map(|issue| Blocker {
                number: issue.number,
                title: &issue.title,
            })
            .collect();
        self
    }
}

#[derive(Serialize)]
pub struct Flash {
    pub kind: &'static str,
    pub text: String,
}

/// Text the person typed into a form that was not saved, put back into that form.
#[derive(Serialize)]
pub struct Draft {
    pub target: String,
    pub text: String,
}

impl Draft {
    pub fn new(target: impl Into<String>, text: &str) -> Self {
        Draft {
            target: target.into(),
            text: text.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render(source: &str, context: Value) -> String {
        let mut templates = Templates::new();
        templates
            .environment
            .add_template_owned("test.html", source.to_string())
            .unwrap();
        templates
            .environment
            .get_template("test.html")
            .unwrap()
            .render(context)
            .unwrap()
    }

    #[test]
    fn text_is_escaped_like_jinja2_does_it_and_slashes_are_left_alone() {
        let html = render("{{ text }}", context! { text => "a/b & <c> \"d\" 'e'" });
        assert_eq!(html, "a/b &amp; &lt;c&gt; &#34;d&#34; &#39;e&#39;");
    }

    #[test]
    fn safe_markup_and_icons_are_not_escaped_and_none_prints_nothing() {
        let html = render("{{ icon('x') }}|{{ nothing }}|{{ none }}", context! {});
        assert!(html.starts_with("<svg class=\"icon icon-x\""));
        assert!(html.ends_with("||"));
    }

    #[test]
    fn an_unknown_icon_is_an_error_not_an_empty_space() {
        let mut templates = Templates::new();
        templates
            .environment
            .add_template_owned("test.html", "{{ icon('nope') }}".to_string())
            .unwrap();
        assert!(
            templates
                .environment
                .get_template("test.html")
                .unwrap()
                .render(context! {})
                .is_err()
        );
    }

    #[test]
    fn numbers_round_like_python() {
        assert_eq!(
            render(
                "{{ (100 * 1 / 3)|round(1) }} {{ (100 * 2 / 4)|round(1) }}",
                context! {}
            ),
            "33.3 50.0"
        );
    }
}
