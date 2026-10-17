//! Walking the open root: which folders a person may see, and what is in them.
//!
//! Open is the only way a request names a directory, so everything it is given is a path relative
//! to the root and is checked here before anything touches the disk under it.

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;
use serde::Serialize;

use crate::config::is_project_name;
use crate::issues::FILENAME;
use crate::text::casefold;

pub const ISSUE_FOLDERS: [&str; 2] = ["tasks", "issues"];
static UNSAFE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^A-Za-z0-9._-]+").unwrap());

/// The path is not a directory Open may show.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowseError(pub String);

impl fmt::Display for BrowseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for BrowseError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Folder {
    pub name: String,
    pub relative: String,
    /// Issue files directly inside it.
    pub issues: usize,
    /// `tasks/` or `issues/` folders inside it.
    pub issue_folders: Vec<&'static str>,
}

/// The directory `relative` names under `root`.
///
/// Hidden folders and `..` are refused outright; a symbolic link is followed only as far as the
/// root, so it cannot lead anywhere the root does not contain.
pub fn resolve(root: &Path, relative: &str) -> Result<PathBuf, BrowseError> {
    let parts: Vec<&str> = relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect();
    if parts.iter().any(|part| part.starts_with('.')) {
        return Err(BrowseError(
            "hidden folders and “..” cannot be opened".into(),
        ));
    }
    let missing = || BrowseError(format!("there is no folder “{relative}” to open"));
    let base = fs::canonicalize(root).map_err(|_| missing())?;
    let path = fs::canonicalize(
        parts
            .iter()
            .fold(base.clone(), |path, part| path.join(part)),
    )
    .map_err(|_| missing())?;
    if !path.starts_with(&base) || !path.is_dir() {
        return Err(missing());
    }
    Ok(path)
}

/// The path as Open addresses it: relative to the root, with `/`, empty for the root.
pub fn relative_to(root: &Path, path: &Path) -> String {
    let base = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    path.strip_prefix(&base)
        .map(|relative| {
            relative
                .components()
                .map(|part| part.as_os_str().to_string_lossy())
                .collect::<Vec<_>>()
                .join("/")
        })
        .unwrap_or_default()
}

pub fn count_issues(directory: &Path) -> usize {
    let Ok(entries) = fs::read_dir(directory) else {
        return 0;
    };
    entries
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| FILENAME.is_match(name))
        })
        .filter(|entry| entry.path().is_file())
        .count()
}

fn is_real_directory(path: &Path) -> bool {
    fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_dir())
}

/// The visible folders directly inside `directory`, by name.
pub fn folders(root: &Path, directory: &Path) -> Vec<Folder> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort_by_cached_key(|name| (casefold(name), name.clone()));
    let base = relative_to(root, directory);
    names
        .into_iter()
        .map(|name| {
            let path = directory.join(&name);
            Folder {
                relative: if base.is_empty() {
                    name.clone()
                } else {
                    format!("{base}/{name}")
                },
                issues: count_issues(&path),
                issue_folders: ISSUE_FOLDERS
                    .into_iter()
                    .filter(|folder| is_real_directory(&path.join(folder)))
                    .collect(),
                name,
            }
        })
        .collect()
}

/// A project name for an opened folder, made from its path and free in `taken`.
///
/// `notes-app/tasks` becomes `notes-app`: the folder that holds the issues says less than the one
/// around it. A name longer than a project name may be keeps its end, the most specific part.
pub fn project_name(root: &Path, relative: &str, taken: impl Fn(&str) -> bool) -> String {
    let mut parts: Vec<String> = relative
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        parts.push(
            root.file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }
    if parts.len() > 1
        && parts
            .last()
            .is_some_and(|last| ISSUE_FOLDERS.contains(&last.as_str()))
    {
        parts.pop();
    }
    let slugs: Vec<String> = parts
        .iter()
        .map(|part| {
            UNSAFE
                .replace_all(part, "-")
                .trim_matches(['-', '.'])
                .to_string()
        })
        .filter(|slug| !slug.is_empty())
        .collect();
    let joined = slugs.join(".");
    // Only ASCII is left, so a byte offset is a character offset.
    let tail = &joined[joined.len().saturating_sub(64)..];
    let mut name = tail.trim_start_matches(['-', '.', '_']).to_string();
    if name.is_empty() || !is_project_name(&name) {
        name = "folder".into();
    }
    let mut candidate = name.clone();
    let mut number = 1;
    while taken(&candidate) {
        number += 1;
        let suffix = format!("-{number}");
        candidate = format!("{}{suffix}", &name[..name.len().min(64 - suffix.len())]);
    }
    candidate
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    /// A made-up tree: two apps with issue folders, a hidden folder, a file and a way out.
    fn tree() -> (tempfile::TempDir, PathBuf) {
        let directory = tempfile::tempdir().unwrap();
        let base = directory.path().join("root");
        let tasks = base.join("notes-app/tasks");
        fs::create_dir_all(&tasks).unwrap();
        fs::write(tasks.join("001-first.md"), "# #1 First\n").unwrap();
        fs::write(tasks.join("002-second.md"), "# #2 Second\n").unwrap();
        fs::write(tasks.join("README.md"), "# Issues\n").unwrap();
        fs::create_dir_all(base.join("notes-app/docs/plan/issues")).unwrap();
        fs::create_dir(base.join("Archive 2025")).unwrap();
        fs::create_dir(base.join(".git")).unwrap();
        fs::write(base.join("readme.txt"), "not a folder\n").unwrap();
        fs::create_dir(directory.path().join("outside")).unwrap();
        std::os::unix::fs::symlink(directory.path().join("outside"), base.join("escape")).unwrap();
        (directory, base)
    }

    fn real(path: PathBuf) -> PathBuf {
        fs::canonicalize(path).unwrap()
    }

    #[test]
    fn a_path_under_the_root_resolves_to_that_folder() {
        let (_directory, root) = tree();
        assert_eq!(
            resolve(&root, "notes-app/tasks").unwrap(),
            real(root.join("notes-app/tasks"))
        );
        assert_eq!(resolve(&root, "").unwrap(), real(root.clone()));
        assert_eq!(
            resolve(&root, "/notes-app/./tasks/").unwrap(),
            real(root.join("notes-app/tasks"))
        );
    }

    #[test]
    fn hidden_folders_ways_out_files_and_missing_folders_cannot_be_opened() {
        let (_directory, root) = tree();
        for path in [
            "..",
            "notes-app/../..",
            ".git",
            "notes-app/.hidden",
            "escape",
            "readme.txt",
            "missing",
            "a\0b",
        ] {
            assert!(resolve(&root, path).is_err(), "{path:?} was opened");
        }
    }

    #[test]
    fn the_listing_shows_visible_folders_with_their_issue_counts() {
        let (_directory, root) = tree();
        let found: Vec<(String, String, usize, Vec<&str>)> =
            folders(&root, &resolve(&root, "").unwrap())
                .into_iter()
                .map(|f| (f.name, f.relative, f.issues, f.issue_folders))
                .collect();
        assert_eq!(
            found,
            [
                ("Archive 2025".into(), "Archive 2025".into(), 0, vec![]),
                ("notes-app".into(), "notes-app".into(), 0, vec!["tasks"]),
            ]
        );
        let inner: Vec<(String, usize)> = folders(&root, &resolve(&root, "notes-app").unwrap())
            .into_iter()
            .map(|f| (f.relative, f.issues))
            .collect();
        assert_eq!(
            inner,
            [("notes-app/docs".into(), 0), ("notes-app/tasks".into(), 2)]
        );
    }

    #[test]
    fn only_numbered_markdown_files_count_as_issues() {
        let (_directory, root) = tree();
        assert_eq!(count_issues(&root.join("notes-app/tasks")), 2);
        assert_eq!(count_issues(&root.join("missing")), 0);
    }

    #[test]
    fn the_relative_path_of_the_root_is_empty() {
        let (_directory, root) = tree();
        assert_eq!(relative_to(&root, &resolve(&root, "").unwrap()), "");
        assert_eq!(
            relative_to(&root, &resolve(&root, "notes-app/tasks").unwrap()),
            "notes-app/tasks"
        );
    }

    #[test]
    fn a_name_comes_from_the_path_without_the_folder_that_holds_the_issues() {
        let (_directory, root) = tree();
        for (relative, name) in [
            ("notes-app/tasks", "notes-app"),
            ("notes-app/docs/plan/issues", "notes-app.docs.plan"),
            ("tasks", "tasks"),
            ("Archive 2025/to do!", "Archive-2025.to-do"),
            ("", "root"),
        ] {
            assert_eq!(project_name(&root, relative, |_| false), name);
        }
    }

    #[test]
    fn a_name_that_is_taken_gets_the_next_number() {
        let (_directory, root) = tree();
        let taken: HashSet<&str> = HashSet::from(["notes-app"]);
        assert_eq!(
            project_name(&root, "notes-app/tasks", |name| taken.contains(name)),
            "notes-app-2"
        );
        let taken: HashSet<&str> = HashSet::from(["notes-app", "notes-app-2"]);
        assert_eq!(
            project_name(&root, "notes-app/tasks", |name| taken.contains(name)),
            "notes-app-3"
        );
    }

    #[test]
    fn a_long_path_keeps_the_end_of_its_name_and_stays_a_valid_name() {
        let (_directory, root) = tree();
        let relative = format!("{}/tasks", ["a-very-long-folder-name"; 6].join("/"));
        let name = project_name(&root, &relative, |_| false);
        assert!(name.len() <= 64);
        assert!(name.ends_with("a-very-long-folder-name"));
        assert!(is_project_name(&name));
    }

    #[test]
    fn a_name_that_would_shadow_a_route_is_not_used() {
        let (_directory, root) = tree();
        assert_eq!(project_name(&root, "static/tasks", |_| false), "folder");
    }
}
