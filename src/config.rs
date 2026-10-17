//! Configuration, read from the environment and from a `.env` file that the environment overrides.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use jiff::tz::TimeZone;
use regex::Regex;

use crate::text::quoted;

static PROJECT_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$").unwrap());
static ENV_KEY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z_][A-Za-z0-9_]*$").unwrap());
static GITHUB_OWNER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9][A-Za-z0-9-]{0,38}$").unwrap());
static GITHUB_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Za-z0-9._-]{1,100}$").unwrap());

/// A project with one of these names would shadow a route of the app itself. Sorted, because the
/// error message lists them.
pub const RESERVED_NAMES: [&str; 5] = ["-", "favicon.ico", "healthz", "openapi.json", "static"];
/// Loopback names cannot be used for DNS rebinding, so they are always accepted.
pub const LOOPBACK_HOSTS: [&str; 2] = ["localhost", "127.0.0.1"];
pub const DEFAULT_PORT: u16 = 8090;
/// Every two minutes keeps a board current without spending much of GitHub's hourly allowance.
pub const DEFAULT_GITHUB_REFRESH_SECONDS: u64 = 120;
pub const MIN_GITHUB_REFRESH_SECONDS: u64 = 30;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigError(pub String);

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ConfigError {}

fn error<T>(message: impl Into<String>) -> Result<T, ConfigError> {
    Err(ConfigError(message.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectConfig {
    pub name: String,
    pub path: PathBuf,
    pub read_only: bool,
    /// Picked with Open rather than listed in `PROJECTS`.
    pub opened: bool,
}

/// A GitHub repository the board shows, or with `name` None every repository of the owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubEntry {
    pub owner: String,
    pub name: Option<String>,
    /// A local clone, read for branches the way a project's repository is.
    pub checkout: Option<PathBuf>,
}

impl GithubEntry {
    /// `owner/name`, or `owner/*`.
    pub fn full_name(&self) -> String {
        format!("{}/{}", self.owner, self.name.as_deref().unwrap_or("*"))
    }
}

/// The directory tree that Open can browse.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenRoot {
    pub path: PathBuf,
    pub read_only: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

impl LogLevel {
    pub fn as_tracing(self) -> tracing::Level {
        match self {
            LogLevel::Debug => tracing::Level::DEBUG,
            LogLevel::Info => tracing::Level::INFO,
            LogLevel::Warning => tracing::Level::WARN,
            LogLevel::Error => tracing::Level::ERROR,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Settings {
    pub projects: Vec<ProjectConfig>,
    pub open_root: Option<OpenRoot>,
    pub allowed_hosts: Vec<String>,
    pub timezone: TimeZone,
    pub refresh_seconds: f64,
    pub log_level: LogLevel,
    pub port: u16,
    pub github: Vec<GithubEntry>,
    pub github_refresh_seconds: u64,
    /// The `gh` program; tests put a fake one here.
    pub gh: PathBuf,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            projects: Vec::new(),
            open_root: None,
            allowed_hosts: LOOPBACK_HOSTS.iter().map(|h| h.to_string()).collect(),
            timezone: TimeZone::UTC,
            refresh_seconds: 3.0,
            log_level: LogLevel::Info,
            port: DEFAULT_PORT,
            github: Vec::new(),
            github_refresh_seconds: DEFAULT_GITHUB_REFRESH_SECONDS,
            gh: PathBuf::from("gh"),
        }
    }
}

pub fn is_project_name(name: &str) -> bool {
    PROJECT_NAME.is_match(name) && !RESERVED_NAMES.contains(&name)
}

/// Parse `name=/absolute/path[:ro],…` into project configurations.
pub fn parse_projects(value: &str) -> Result<Vec<ProjectConfig>, ConfigError> {
    let mut projects: Vec<ProjectConfig> = Vec::new();
    for raw in value.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let Some((name, location)) = entry.split_once('=') else {
            return error(format!("{} is not name=/absolute/path", quoted(entry)));
        };
        let (name, location) = (name.trim(), location.trim());
        if location.is_empty() {
            return error(format!("{} is not name=/absolute/path", quoted(entry)));
        }
        if !is_project_name(name) {
            return error(format!(
                "{} cannot be a project name: use letters, digits, '.', '_' or '-', and none of {}",
                quoted(name),
                RESERVED_NAMES.join(", ")
            ));
        }
        if projects.iter().any(|project| project.name == name) {
            return error(format!("project {} is listed twice", quoted(name)));
        }
        let (path, read_only) = location_of(location, &format!("the path of {}", quoted(name)))?;
        projects.push(ProjectConfig {
            name: name.to_string(),
            path,
            read_only,
            opened: false,
        });
    }
    Ok(projects)
}

/// Parse `owner/name[=/absolute/checkout],owner/*,…`; an empty value turns GitHub off.
pub fn parse_github(value: &str) -> Result<Vec<GithubEntry>, ConfigError> {
    let mut entries: Vec<GithubEntry> = Vec::new();
    for raw in value.split(',') {
        let entry = raw.trim();
        if entry.is_empty() {
            continue;
        }
        let (repository, checkout) = match entry.split_once('=') {
            Some((repository, checkout)) => (repository.trim(), Some(checkout.trim())),
            None => (entry, None),
        };
        let malformed = || {
            ConfigError(format!(
                "GITHUB_REPOS: {} is not owner/name, owner/name=/absolute/checkout or owner/*",
                quoted(entry)
            ))
        };
        let (owner, name) = repository.split_once('/').ok_or_else(malformed)?;
        let name = match name {
            "*" => None,
            name if GITHUB_NAME.is_match(name) && name != "." && name != ".." => {
                Some(name.to_string())
            }
            _ => return Err(malformed()),
        };
        if !GITHUB_OWNER.is_match(owner) {
            return Err(malformed());
        }
        let checkout = match checkout {
            None => None,
            Some(_) if name.is_none() => {
                return error(format!(
                    "GITHUB_REPOS: {} cannot have a checkout; name the repository",
                    quoted(repository)
                ));
            }
            Some(path) if path.starts_with('/') => Some(normalise(path)),
            Some(path) => {
                return error(format!(
                    "GITHUB_REPOS: the checkout of {} must be absolute, got {}",
                    quoted(repository),
                    quoted(path)
                ));
            }
        };
        let entry = GithubEntry {
            owner: owner.to_string(),
            name,
            checkout,
        };
        // GitHub compares names without regard to case.
        let same = |other: &GithubEntry| {
            other.full_name().to_lowercase() == entry.full_name().to_lowercase()
        };
        if entries.iter().any(same) {
            return error(format!(
                "GITHUB_REPOS: {} is listed twice",
                quoted(&entry.full_name())
            ));
        }
        entries.push(entry);
    }
    Ok(entries)
}

/// Parse `/absolute/path[:ro]`; an empty value turns Open off.
pub fn parse_open_root(value: &str) -> Result<Option<OpenRoot>, ConfigError> {
    let location = value.trim();
    if location.is_empty() {
        return Ok(None);
    }
    let (path, read_only) = location_of(location, "OPEN_ROOT")?;
    Ok(Some(OpenRoot { path, read_only }))
}

fn location_of(location: &str, what: &str) -> Result<(PathBuf, bool), ConfigError> {
    let read_only = location.ends_with(":ro");
    let location = if read_only || location.ends_with(":rw") {
        &location[..location.len() - 3]
    } else {
        location
    };
    if !location.starts_with('/') {
        return error(format!("{what} must be absolute, got {}", quoted(location)));
    }
    Ok((normalise(location), read_only))
}

/// An absolute path with `.`, `..` and repeated slashes taken out, without touching the disk.
pub fn normalise(path: &str) -> PathBuf {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    PathBuf::from(format!("/{}", parts.join("/")))
}

pub fn parse_hosts(value: &str) -> Vec<String> {
    let mut hosts: Vec<String> = LOOPBACK_HOSTS.iter().map(|h| h.to_string()).collect();
    for host in value.split(',').map(|h| h.trim().to_lowercase()) {
        if !host.is_empty() && !hosts.contains(&host) {
            hosts.push(host);
        }
    }
    hosts
}

/// The zone times are shown in. Without `TZ`, the zone of this machine.
pub fn parse_timezone(value: &str) -> Result<TimeZone, ConfigError> {
    let name = value.trim().trim_start_matches(':');
    if name.is_empty() {
        return Ok(TimeZone::try_system().unwrap_or(TimeZone::UTC));
    }
    TimeZone::get(name).map_err(|_| {
        ConfigError(format!(
            "TZ={} is not a time zone the system knows",
            quoted(value)
        ))
    })
}

pub fn parse_log_level(value: &str) -> Result<LogLevel, ConfigError> {
    match value.trim().to_lowercase().as_str() {
        "" | "info" => Ok(LogLevel::Info),
        "debug" => Ok(LogLevel::Debug),
        "warning" | "warn" => Ok(LogLevel::Warning),
        "error" | "critical" => Ok(LogLevel::Error),
        _ => error(format!(
            "LOG_LEVEL={} is not one of debug, info, warning, error",
            quoted(value)
        )),
    }
}

/// Settings from a lookup of variable names, the process environment layered over `.env`.
pub fn load_settings(env: &dyn Fn(&str) -> Option<String>) -> Result<Settings, ConfigError> {
    let get = |key: &str| env(key).unwrap_or_default();
    let refresh_text = get("REFRESH_SECONDS");
    let refresh = if refresh_text.trim().is_empty() {
        3.0
    } else {
        match refresh_text.trim().parse::<f64>() {
            Ok(value) if value.is_finite() => value,
            _ => {
                return error(format!(
                    "REFRESH_SECONDS must be a number of seconds, got {}",
                    quoted(&refresh_text)
                ));
            }
        }
    };
    if refresh < 0.0 {
        return error("REFRESH_SECONDS cannot be negative");
    }
    let port_text = get("PORT");
    let port = if port_text.trim().is_empty() {
        DEFAULT_PORT
    } else {
        port_text.trim().parse::<u16>().map_err(|_| {
            ConfigError(format!(
                "PORT must be a port number from 0 to 65535, got {}",
                quoted(&port_text)
            ))
        })?
    };
    let github_refresh_text = get("GITHUB_REFRESH_SECONDS");
    let github_refresh_seconds = if github_refresh_text.trim().is_empty() {
        DEFAULT_GITHUB_REFRESH_SECONDS
    } else {
        match github_refresh_text.trim().parse::<u64>() {
            Ok(value) if value >= MIN_GITHUB_REFRESH_SECONDS => value,
            _ => {
                return error(format!(
                    "GITHUB_REFRESH_SECONDS must be a whole number of seconds, at least \
                     {MIN_GITHUB_REFRESH_SECONDS}, got {}",
                    quoted(&github_refresh_text)
                ));
            }
        }
    };
    Ok(Settings {
        projects: parse_projects(&get("PROJECTS"))?,
        open_root: parse_open_root(&get("OPEN_ROOT"))?,
        allowed_hosts: parse_hosts(&get("ALLOWED_HOSTS")),
        timezone: parse_timezone(&get("TZ"))?,
        refresh_seconds: refresh,
        log_level: parse_log_level(&get("LOG_LEVEL"))?,
        port,
        github: parse_github(&get("GITHUB_REPOS"))?,
        github_refresh_seconds,
        gh: PathBuf::from("gh"),
    })
}

/// The variables of a `.env` file: `KEY=value` lines, `#` comments, optional `export` and quotes.
pub fn parse_env_file(text: &str, name: &str) -> Result<HashMap<String, String>, ConfigError> {
    let mut values = HashMap::new();
    for (index, raw) in text.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line
            .strip_prefix("export ")
            .map(str::trim_start)
            .unwrap_or(line);
        let malformed = || ConfigError(format!("line {} of {name} is not KEY=value", index + 1));
        let (key, value) = line.split_once('=').ok_or_else(malformed)?;
        let key = key.trim();
        if !ENV_KEY.is_match(key) {
            return Err(malformed());
        }
        let value = value.trim_start();
        let value = if let Some(rest) = value.strip_prefix('"') {
            let mut out = String::new();
            let mut chars = rest.chars();
            let mut closed = false;
            while let Some(char) = chars.next() {
                match char {
                    '"' => {
                        closed = true;
                        break;
                    }
                    '\\' => match chars.next() {
                        Some('n') => out.push('\n'),
                        Some('t') => out.push('\t'),
                        Some(other) => out.push(other),
                        None => break,
                    },
                    other => out.push(other),
                }
            }
            if !closed {
                return Err(ConfigError(format!(
                    "line {} of {name} opens a quote it does not close",
                    index + 1
                )));
            }
            out
        } else if let Some(rest) = value.strip_prefix('\'') {
            match rest.find('\'') {
                Some(end) => rest[..end].to_string(),
                None => {
                    return Err(ConfigError(format!(
                        "line {} of {name} opens a quote it does not close",
                        index + 1
                    )));
                }
            }
        } else {
            // An unquoted value ends where a comment starts: a `#` after whitespace.
            let mut end = value.len();
            let bytes = value.as_bytes();
            for (position, byte) in bytes.iter().enumerate() {
                if *byte == b'#' && position > 0 && matches!(bytes[position - 1], b' ' | b'\t') {
                    end = position;
                    break;
                }
            }
            value[..end].trim_end().to_string()
        };
        values.insert(key.to_string(), value);
    }
    Ok(values)
}

/// Variables from the process environment, falling back to those read from a `.env` file.
#[derive(Debug, Default, Clone)]
pub struct Environment {
    file: HashMap<String, String>,
}

impl Environment {
    /// Read `path` if it is there; a missing file is no error unless it was asked for by name.
    pub fn load(path: &Path, required: bool) -> Result<Self, ConfigError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Environment {
                file: parse_env_file(&text, &path.display().to_string())?,
            }),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound && !required => {
                Ok(Environment::default())
            }
            Err(error) => Err(ConfigError(format!(
                "cannot read {}: {error}",
                path.display()
            ))),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        std::env::var(key)
            .ok()
            .or_else(|| self.file.get(key).cloned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings_from(pairs: &[(&str, &str)]) -> Result<Settings, ConfigError> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        load_settings(&|key| map.get(key).cloned())
    }

    #[test]
    fn projects_are_parsed_with_the_read_only_suffix() {
        let projects =
            parse_projects(" notes=/home/u/notes/tasks , archive=/srv/old/tasks/:ro,x=/y:rw")
                .unwrap();
        let found: Vec<(&str, &Path, bool)> = projects
            .iter()
            .map(|p| (p.name.as_str(), p.path.as_path(), p.read_only))
            .collect();
        assert_eq!(
            found,
            vec![
                ("notes", Path::new("/home/u/notes/tasks"), false),
                ("archive", Path::new("/srv/old/tasks"), true),
                ("x", Path::new("/y"), false),
            ]
        );
    }

    #[test]
    fn entries_that_are_not_a_name_and_an_absolute_path_are_refused() {
        for value in [
            "notes",
            "notes=",
            "=/a",
            "notes=relative/path",
            "static=/a",
            "bad name=/a",
            "a=/x,a=/y",
            "../up=/a",
        ] {
            assert!(parse_projects(value).is_err(), "{value} was accepted");
        }
    }

    #[test]
    fn an_empty_project_list_is_allowed() {
        assert!(parse_projects("").unwrap().is_empty());
    }

    #[test]
    fn a_path_with_spaces_is_kept_whole() {
        let projects = parse_projects("jira=/Users/u/dev/2026-09 Jira notes/tasks").unwrap();
        assert_eq!(
            projects[0].path,
            Path::new("/Users/u/dev/2026-09 Jira notes/tasks")
        );
    }

    #[test]
    fn loopback_hosts_are_always_allowed_and_extra_ones_are_added() {
        let settings = settings_from(&[("ALLOWED_HOSTS", "Viewer.local, e2e-app")]).unwrap();
        assert_eq!(
            settings.allowed_hosts,
            ["localhost", "127.0.0.1", "viewer.local", "e2e-app"]
        );
    }

    #[test]
    fn settings_come_from_the_environment() {
        let settings = settings_from(&[
            ("PROJECTS", "a=/a"),
            ("TZ", "Europe/Warsaw"),
            ("REFRESH_SECONDS", "0"),
            ("LOG_LEVEL", "debug"),
            ("PORT", "9000"),
        ])
        .unwrap();
        assert_eq!(settings.projects[0].name, "a");
        assert_eq!(settings.timezone.iana_name(), Some("Europe/Warsaw"));
        assert_eq!(settings.refresh_seconds, 0.0);
        assert_eq!(settings.log_level, LogLevel::Debug);
        assert_eq!(settings.port, 9000);
    }

    #[test]
    fn bad_values_are_configuration_errors() {
        for pair in [
            ("TZ", "Mars/Olympus"),
            ("REFRESH_SECONDS", "-1"),
            ("REFRESH_SECONDS", "soon"),
            ("PORT", "x"),
            ("PORT", "70000"),
            ("LOG_LEVEL", "loud"),
        ] {
            assert!(settings_from(&[pair]).is_err(), "{pair:?} was accepted");
        }
    }

    #[test]
    fn the_open_root_is_parsed_with_the_read_only_suffix() {
        let open = |path: &str, read_only| {
            Some(OpenRoot {
                path: PathBuf::from(path),
                read_only,
            })
        };
        assert_eq!(
            parse_open_root(" /home/u/dev/ ").unwrap(),
            open("/home/u/dev", false)
        );
        assert_eq!(
            parse_open_root("/home/u/dev:ro").unwrap(),
            open("/home/u/dev", true)
        );
        assert_eq!(parse_open_root("").unwrap(), None);
        assert_eq!(settings_from(&[]).unwrap().open_root, None);
    }

    #[test]
    fn a_relative_open_root_is_refused() {
        assert!(settings_from(&[("OPEN_ROOT", "dev")]).is_err());
    }

    #[test]
    fn github_repositories_are_parsed_with_an_optional_checkout() {
        let entries =
            parse_github(" acme/notes-api , acme/notes.web=/home/u/dev/notes-web/ ,other-org/* ")
                .unwrap();
        let found: Vec<(String, Option<&Path>)> = entries
            .iter()
            .map(|entry| (entry.full_name(), entry.checkout.as_deref()))
            .collect();
        assert_eq!(
            found,
            [
                ("acme/notes-api".to_string(), None),
                (
                    "acme/notes.web".to_string(),
                    Some(Path::new("/home/u/dev/notes-web"))
                ),
                ("other-org/*".to_string(), None),
            ]
        );
        assert!(parse_github("").unwrap().is_empty());
        assert!(settings_from(&[]).unwrap().github.is_empty());
    }

    #[test]
    fn github_entries_that_are_not_a_repository_or_an_owner_are_refused() {
        for value in [
            "acme",
            "acme/",
            "/notes",
            "acme/notes/extra",
            "-acme/notes",
            "acme/..",
            "acme/no tes",
            "acme/notes=relative/checkout",
            "acme/*=/home/u/dev",
            "acme/notes,ACME/Notes",
            "acme/*,acme/*",
        ] {
            assert!(parse_github(value).is_err(), "{value} was accepted");
        }
    }

    #[test]
    fn github_refresh_seconds_have_a_default_and_a_floor() {
        assert_eq!(
            settings_from(&[]).unwrap().github_refresh_seconds,
            DEFAULT_GITHUB_REFRESH_SECONDS
        );
        let settings = settings_from(&[("GITHUB_REFRESH_SECONDS", "300")]).unwrap();
        assert_eq!(settings.github_refresh_seconds, 300);
        for value in ["29", "0", "-5", "soon", "60.5"] {
            assert!(
                settings_from(&[("GITHUB_REFRESH_SECONDS", value)]).is_err(),
                "{value} was accepted"
            );
        }
    }

    #[test]
    fn an_env_file_is_read_with_comments_quotes_and_export() {
        let values = parse_env_file(
            "# comment\n\nexport A=1\nB = two words # note\nC=\"quoted # kept\"\nD='x'\nE=\n",
            ".env",
        )
        .unwrap();
        assert_eq!(values["A"], "1");
        assert_eq!(values["B"], "two words");
        assert_eq!(values["C"], "quoted # kept");
        assert_eq!(values["D"], "x");
        assert_eq!(values["E"], "");
    }

    #[test]
    fn a_malformed_env_file_names_the_line_without_its_value() {
        let error = parse_env_file("A=1\nsecret-value\n", ".env").unwrap_err();
        assert_eq!(error.0, "line 2 of .env is not KEY=value");
    }
}
