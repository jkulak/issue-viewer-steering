//! A viewer on copies of the synthetic issues in `tests/fixtures`, driven without a socket.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{HeaderMap, Request, header};
use http_body_util::BodyExt;
use issue_viewer::config::{OpenRoot, ProjectConfig, Settings};
use issue_viewer::web::{self, App};
use tempfile::TempDir;
use tokio::runtime::Runtime;
use tower::ServiceExt;

pub const JSON: (&str, &str) = ("accept", "application/json");

/// Copy the files of one of the fixture directories into `target`.
pub fn copy_fixture(name: &str, target: &Path) -> PathBuf {
    let source = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name);
    std::fs::create_dir_all(target).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
    }
    target.to_path_buf()
}

pub fn contents(directory: &Path) -> BTreeMap<String, Vec<u8>> {
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| {
            let entry = entry.unwrap();
            (
                entry.file_name().to_string_lossy().into_owned(),
                std::fs::read(entry.path()).unwrap(),
            )
        })
        .collect()
}

pub fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap()
}

pub struct Reply {
    pub status: u16,
    pub headers: HeaderMap,
    pub text: String,
}

impl Reply {
    pub fn header(&self, name: &str) -> &str {
        self.headers
            .get(name)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default()
    }

    pub fn location(&self) -> &str {
        self.header("location")
    }

    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.text)
            .unwrap_or_else(|error| panic!("not JSON ({error}): {}", self.text))
    }
}

pub struct Viewer {
    pub app: Arc<App>,
    router: Router,
    runtime: Runtime,
    /// demo and archive live here; with an open root, so do notes-app/tasks and .hidden/tasks.
    pub root: PathBuf,
    pub demo: PathBuf,
    pub archive: PathBuf,
    _directory: TempDir,
}

pub enum Open {
    Off,
    ReadWrite,
    ReadOnly,
}

impl Viewer {
    /// demo (read-write) and archive (read-only), with Open off.
    pub fn new() -> Self {
        Viewer::build(Open::Off)
    }

    /// demo only, with the temporary directory as the open root.
    pub fn with_open_root(open: Open) -> Self {
        Viewer::build(open)
    }

    fn build(open: Open) -> Self {
        let directory = tempfile::tempdir().unwrap();
        // The canonical path, so what Open resolves matches the configured projects on macOS too.
        let root = directory.path().canonicalize().unwrap();
        let demo = copy_fixture("demo", &root.join("demo"));
        let archive = copy_fixture("archive", &root.join("archive"));
        let mut settings = Settings::default();
        let project = |name: &str, path: &Path, read_only| ProjectConfig {
            name: name.into(),
            path: path.to_path_buf(),
            read_only,
            opened: false,
        };
        settings.projects = vec![project("demo", &demo, false)];
        match open {
            Open::Off => settings.projects.push(project("archive", &archive, true)),
            Open::ReadWrite | Open::ReadOnly => {
                for folder in ["notes-app/tasks", ".hidden/tasks"] {
                    copy_fixture("archive", &root.join(folder));
                }
                let read_only = matches!(open, Open::ReadOnly);
                settings.open_root = Some(OpenRoot {
                    path: root.clone(),
                    read_only,
                });
            }
        }
        let app = App::new(settings);
        let router = web::router(app.clone());
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        Viewer {
            app,
            router,
            runtime,
            root,
            demo,
            archive,
            _directory: directory,
        }
    }

    pub fn token(&self) -> String {
        self.app.csrf_token().to_string()
    }

    pub fn send(
        &self,
        method: &str,
        uri: &str,
        headers: &[(&str, &str)],
        body: Option<String>,
    ) -> Reply {
        let mut request = Request::builder().method(method).uri(uri);
        if !headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("host"))
        {
            request = request.header(header::HOST, "localhost");
        }
        for (name, value) in headers {
            request = request.header(*name, *value);
        }
        let body = match body {
            Some(body) => {
                request = request.header(header::CONTENT_TYPE, "application/x-www-form-urlencoded");
                Body::from(body)
            }
            None => Body::empty(),
        };
        let request = request.body(body).unwrap();
        let router = self.router.clone();
        self.runtime.block_on(async move {
            let response = router.oneshot(request).await.unwrap();
            let status = response.status().as_u16();
            let headers = response.headers().clone();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            Reply {
                status,
                headers,
                text: String::from_utf8_lossy(&bytes).into_owned(),
            }
        })
    }

    pub fn get(&self, uri: &str) -> Reply {
        self.send("GET", uri, &[], None)
    }

    pub fn get_with(&self, uri: &str, headers: &[(&str, &str)]) -> Reply {
        self.send("GET", uri, headers, None)
    }

    pub fn get_query(&self, path: &str, query: &[(&str, &str)]) -> Reply {
        self.get(&format!("{path}?{}", encoded(query)))
    }

    /// A form post with the page's token, the way the viewer's own forms send it.
    pub fn post(&self, uri: &str, fields: &[(&str, &str)]) -> Reply {
        let token = self.token();
        let mut all = vec![("_csrf", token.as_str())];
        all.extend_from_slice(fields);
        self.send("POST", uri, &[], Some(encoded(&all)))
    }

    /// A form post exactly as given: no token unless `fields` or `headers` carry one.
    pub fn post_raw(&self, uri: &str, fields: &[(&str, &str)], headers: &[(&str, &str)]) -> Reply {
        self.send("POST", uri, headers, Some(encoded(fields)))
    }

    pub fn version_of(&self, number: u64) -> String {
        let reply = self.get(&format!("/demo/-/version?issue={number}"));
        reply.json()["version"].as_str().unwrap().to_string()
    }

    pub fn project_names(&self) -> Vec<String> {
        self.app
            .registry
            .all()
            .iter()
            .map(|project| project.name().to_string())
            .collect()
    }
}

pub fn encoded(pairs: &[(&str, &str)]) -> String {
    form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

pub fn issue_path(directory: &Path, number: u64) -> PathBuf {
    let prefix = format!("{number:03}-");
    std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .find(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(&prefix)
        })
        .unwrap_or_else(|| panic!("no file for issue {number}"))
}

pub fn row_numbers(html: &str) -> Vec<u64> {
    let pattern = regex::Regex::new(r#"id="issue-(\d+)""#).unwrap();
    pattern
        .captures_iter(html)
        .map(|captures| captures[1].parse().unwrap())
        .collect()
}

pub fn capture(pattern: &str, text: &str) -> String {
    let pattern = regex::Regex::new(pattern).unwrap();
    pattern
        .captures(text)
        .unwrap_or_else(|| panic!("{pattern} not found"))[1]
        .to_string()
}
