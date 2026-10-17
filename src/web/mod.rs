//! The web app: pages, forms, and the checks that keep other sites from using them.
//!
//! There is no login, so the protection is about where a request comes from: the Host header must
//! be one the viewer answers to (no DNS rebinding), a write must come from the viewer's own pages
//! (Origin and Sec-Fetch-Site) and carry the token those pages embed (no cross-site forms).
//!
//! Every request is answered on a blocking thread: reading the issue files is all it does.

mod assets;
pub mod openapi;
mod pages;
mod views;
mod writes;

use std::sync::Arc;

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};
use minijinja::context;

use crate::config::Settings;
use crate::files::hex;
use crate::project::Registry;

pub const CSP: &str = "default-src 'self'; img-src 'self' data:; style-src 'self' 'unsafe-inline'; \
    script-src 'self'; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";
/// More than any issue file a person writes by hand, less than what would hurt to hold in memory.
const BODY_LIMIT: usize = 4 * 1024 * 1024;

pub struct App {
    pub settings: Settings,
    pub registry: Registry,
    csrf_token: String,
    templates: views::Templates,
}

impl App {
    pub fn new(settings: Settings) -> Arc<App> {
        let mut bytes = [0u8; 32];
        getrandom::fill(&mut bytes).expect("the system has a source of random numbers");
        Arc::new(App {
            registry: Registry::new(settings.projects.clone()),
            settings,
            // A new token on every start: a page loaded before a restart has to be reloaded to
            // change anything, which is what its message says.
            csrf_token: hex(&bytes),
            templates: views::Templates::new(),
        })
    }

    pub fn csrf_token(&self) -> &str {
        &self.csrf_token
    }

    fn allows_host(&self, headers: &HeaderMap) -> bool {
        let Some(host) = headers
            .get(header::HOST)
            .and_then(|value| value.to_str().ok())
        else {
            return false;
        };
        let host = host.to_ascii_lowercase();
        let name = match host.strip_prefix('[') {
            Some(rest) => rest
                .split_once(']')
                .map_or(host.as_str(), |(inside, _)| inside),
            None => host
                .rsplit_once(':')
                .map_or(host.as_str(), |(name, _)| name),
        };
        self.settings.allowed_hosts.iter().any(|pattern| {
            pattern == "*"
                || pattern == name
                || pattern
                    .strip_prefix('*')
                    .is_some_and(|suffix| name.ends_with(suffix))
        })
    }

    fn error_page(&self, request: &Incoming, failure: Failure) -> Response {
        if request.wants_json() {
            return (
                failure.status,
                axum::Json(serde_json::json!({ "error": failure.message })),
            )
                .into_response();
        }
        let context = context! {
            status => failure.status.as_u16(),
            message => failure.message,
            ..views::base(self, None)
        };
        self.templates.render("error.html", context, failure.status)
    }
}

/// A request the app refuses, with the sentence the person sees.
#[derive(Debug)]
pub struct Failure {
    pub status: StatusCode,
    pub message: String,
}

impl Failure {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Failure {
            status,
            message: message.into(),
        }
    }

    pub fn nothing_here() -> Self {
        Failure::new(StatusCode::NOT_FOUND, "There is nothing at this address.")
    }

    pub fn not_understood() -> Self {
        Failure::new(StatusCode::BAD_REQUEST, "The request was not understood.")
    }
}

pub type Answer = Result<Response, Failure>;

/// A request, read in full, with what the routes need from it.
pub struct Incoming {
    pub method: Method,
    pub path: String,
    pub query: Vec<(String, String)>,
    pub headers: HeaderMap,
    pub form: Vec<(String, String)>,
}

impl Incoming {
    fn new(method: Method, uri: &axum::http::Uri, headers: HeaderMap, body: &[u8]) -> Self {
        let pairs = |bytes: &[u8]| {
            form_urlencoded::parse(bytes)
                .into_owned()
                .collect::<Vec<_>>()
        };
        let is_form = headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|kind| {
                kind.trim()
                    .to_ascii_lowercase()
                    .starts_with("application/x-www-form-urlencoded")
            });
        Incoming {
            method,
            path: uri.path().to_string(),
            query: pairs(uri.query().unwrap_or_default().as_bytes()),
            form: if is_form { pairs(body) } else { Vec::new() },
            headers,
        }
    }

    pub fn header(&self, name: &str) -> Option<String> {
        self.headers
            .get(name)
            .map(|value| String::from_utf8_lossy(value.as_bytes()).into_owned())
    }

    /// The last value of a query parameter, like the Python version read it.
    pub fn query(&self, name: &str) -> Option<&str> {
        self.query
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    pub fn field(&self, name: &str) -> Option<&str> {
        self.form
            .iter()
            .rev()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }

    /// A form field, empty when it was not sent.
    pub fn text(&self, name: &str) -> &str {
        self.field(name).unwrap_or_default()
    }

    pub fn fields(&self, name: &str) -> Vec<&str> {
        self.form
            .iter()
            .filter(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
            .collect()
    }

    pub fn wants_json(&self) -> bool {
        self.header("accept")
            .is_some_and(|accept| accept.contains("application/json"))
    }
}

pub fn router(app: Arc<App>) -> Router {
    Router::new().fallback(handle).with_state(app)
}

async fn handle(State(app): State<Arc<App>>, request: Request) -> Response {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_string();
    let mut response = if !app.allows_host(&parts.headers) {
        (StatusCode::BAD_REQUEST, "Invalid host header").into_response()
    } else {
        // A body that cannot be read in full is refused, never handled as an empty form: an
        // empty description would otherwise replace the one in the file.
        let (body, too_large) = match axum::body::to_bytes(body, BODY_LIMIT).await {
            Ok(body) => (body, false),
            Err(_) => (Bytes::new(), true),
        };
        let method = if parts.method == Method::HEAD {
            Method::GET
        } else {
            parts.method.clone()
        };
        let incoming = Incoming::new(method, &parts.uri, parts.headers, &body);
        let worker = app.clone();
        let answered = tokio::task::spawn_blocking(move || {
            let answer = if too_large {
                Err(Failure::new(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "The text sent is too long to save.",
                ))
            } else {
                route(&worker, &incoming)
            };
            let response = answer.unwrap_or_else(|failure| worker.error_page(&incoming, failure));
            tracing::debug!(
                "{} {} {}",
                incoming.method,
                incoming.path,
                response.status().as_u16()
            );
            response
        })
        .await;
        answered.unwrap_or_else(|error| {
            tracing::error!("{path}: {error}");
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Something went wrong; the log says what.",
            )
                .into_response()
        })
    };
    secure(&mut response, &path);
    response
}

/// The headers every answer carries, unless the route set its own.
fn secure(response: &mut Response, path: &str) {
    let headers = response.headers_mut();
    let mut set = |name: HeaderName, value: &'static str| {
        headers
            .entry(name)
            .or_insert(HeaderValue::from_static(value));
    };
    set(header::CONTENT_SECURITY_POLICY, CSP);
    set(header::X_CONTENT_TYPE_OPTIONS, "nosniff");
    set(header::REFERRER_POLICY, "same-origin");
    set(header::X_FRAME_OPTIONS, "DENY");
    if !path.starts_with("/static/") {
        set(header::CACHE_CONTROL, "no-store");
    }
}

fn percent_decoded(segment: &str) -> Option<String> {
    let bytes = segment.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%'
            && let Some(byte) = segment
                .get(index + 1..index + 3)
                .and_then(|hex| u8::from_str_radix(hex, 16).ok())
        {
            decoded.push(byte);
            index += 3;
        } else {
            decoded.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(decoded).ok()
}

/// An issue or comment number in a path: digits only, so `-1` or `..` is no address.
fn number(segment: &str) -> Option<u64> {
    if segment.is_empty() || !segment.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    segment.parse().ok()
}

pub fn redirect(status: StatusCode, location: &str) -> Response {
    let location = HeaderValue::from_str(location).unwrap_or(HeaderValue::from_static("/"));
    (status, [(header::LOCATION, location)]).into_response()
}

fn route(app: &App, request: &Incoming) -> Answer {
    let raw: Vec<&str> = request
        .path
        .strip_prefix('/')
        .unwrap_or(&request.path)
        .split('/')
        .collect();
    // `/demo/issues/` is `/demo/issues`; any other empty part is no address, so `//host` can never
    // turn into a redirect to another site.
    let (raw, trailing) = match raw.split_last() {
        Some((last, rest)) if last.is_empty() && !rest.is_empty() => (rest, true),
        _ => (raw.as_slice(), false),
    };
    let segments: Vec<String> = raw
        .iter()
        .map(|part| percent_decoded(part))
        .collect::<Option<_>>()
        .ok_or_else(Failure::nothing_here)?;
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    let segments: &[&str] = if segments == [""] { &[] } else { &segments };
    if segments.iter().any(|segment| segment.is_empty()) {
        return Err(Failure::nothing_here());
    }
    if trailing {
        let mut location = format!("/{}", raw.join("/"));
        if !request.query.is_empty() {
            location.push('?');
            location.push_str(
                &form_urlencoded::Serializer::new(String::new())
                    .extend_pairs(&request.query)
                    .finish(),
            );
        }
        return Ok(redirect(StatusCode::TEMPORARY_REDIRECT, &location));
    }

    let get = request.method == Method::GET;
    let post = request.method == Method::POST;
    let only = |allowed: bool, answer: &dyn Fn() -> Answer| -> Answer {
        if allowed {
            answer()
        } else {
            Err(Failure::new(
                StatusCode::METHOD_NOT_ALLOWED,
                "This address does not take that kind of request.",
            ))
        }
    };
    match segments {
        [] => only(get, &|| pages::home(app)),
        ["healthz"] => only(get, &|| {
            Ok(axum::Json(serde_json::json!({ "status": "ok" })).into_response())
        }),
        ["favicon.ico"] => only(get, &|| {
            Ok(redirect(
                StatusCode::MOVED_PERMANENTLY,
                "/static/favicon.svg",
            ))
        }),
        ["static", name] => only(get, &|| {
            assets::serve(request, name).ok_or_else(Failure::nothing_here)
        }),
        ["-", "open"] if post => writes::open(app, request),
        ["-", "open"] => only(get, &|| pages::open(app, request)),
        [project] => only(get, &|| pages::project_root(app, project)),
        [project, "issues"] if post => writes::create(app, request, project),
        [project, "issues"] => only(get, &|| pages::issues(app, request, project)),
        [project, "issues", "new"] => only(get, &|| pages::new_issue(app, project)),
        [project, "issues", n] if number(n).is_some() => only(get, &|| {
            pages::issue(app, project, number(n).unwrap_or_default())
        }),
        [project, "issues", n, action] if number(n).is_some() => {
            let n = number(n).unwrap_or_default();
            match *action {
                "title" => only(post, &|| writes::title(app, request, project, n)),
                "description" => only(post, &|| writes::description(app, request, project, n)),
                "labels" => only(post, &|| writes::labels(app, request, project, n)),
                "comments" => only(post, &|| writes::comment(app, request, project, n)),
                "tasks" => only(post, &|| writes::task(app, request, project, n)),
                _ => Err(Failure::nothing_here()),
            }
        }
        [project, "issues", n, "comments", i] if number(n).is_some() && number(i).is_some() => {
            let (n, i) = (number(n).unwrap_or_default(), number(i).unwrap_or_default());
            only(post, &|| writes::edit_comment(app, request, project, n, i))
        }
        [project, "issues", n, "comments", i, "delete"]
            if number(n).is_some() && number(i).is_some() =>
        {
            let (n, i) = (number(n).unwrap_or_default(), number(i).unwrap_or_default());
            only(post, &|| {
                writes::delete_comment(app, request, project, n, i)
            })
        }
        [project, "labels"] => only(get, &|| pages::labels(app, project)),
        [project, "-", "version"] => only(get, &|| pages::version(app, request, project)),
        [project, "preview"] => only(post, &|| writes::preview(app, request, project)),
        _ => Err(Failure::nothing_here()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_part_is_percent_decoded_and_bad_utf8_is_refused() {
        assert_eq!(percent_decoded("a%20b%2F..").as_deref(), Some("a b/.."));
        assert_eq!(percent_decoded("100%").as_deref(), Some("100%"));
        assert_eq!(percent_decoded("%ff"), None);
    }

    #[test]
    fn only_digits_make_a_number() {
        assert_eq!(number("007"), Some(7));
        for text in ["-1", "", "1e3", "..", "+1", "99999999999999999999999"] {
            assert_eq!(number(text), None, "{text}");
        }
    }

    #[test]
    fn the_host_header_must_name_an_allowed_host() {
        let app = App::new(Settings {
            allowed_hosts: vec![
                "localhost".into(),
                "127.0.0.1".into(),
                "*.example.test".into(),
            ],
            ..Settings::default()
        });
        let allows = |host: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(header::HOST, HeaderValue::from_str(host).unwrap());
            app.allows_host(&headers)
        };
        for host in [
            "localhost",
            "LOCALHOST:8090",
            "127.0.0.1:8080",
            "notes.example.test",
        ] {
            assert!(allows(host), "{host}");
        }
        for host in [
            "rebound.attacker.example",
            "localhost.attacker.example",
            "[::1]:8090",
            "example.test.evil",
        ] {
            assert!(!allows(host), "{host}");
        }
        assert!(!app.allows_host(&HeaderMap::new()));
    }
}
