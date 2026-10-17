//! One configured issues directory: reading it, and writing to it safely.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::ffi::CString;
use std::fmt;
use std::fs;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, RwLock};

use jiff::Timestamp;

use crate::config::ProjectConfig;
use crate::files;
use crate::index::{self, RowChange};
use crate::issuefile::{EditError, new_issue_text, parse};
use crate::issues::{FILENAME, Issue, build, slugify};
use crate::text::{casefold, quoted};

#[derive(Debug)]
pub enum ProjectError {
    NotFound(String),
    /// The project is read-only, by configuration or because the directory cannot be written.
    ReadOnly(String),
    /// The file changed on disk since the version the edit was based on.
    Conflict(String),
    Edit(String),
    Io(io::Error),
}

impl fmt::Display for ProjectError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProjectError::NotFound(message)
            | ProjectError::ReadOnly(message)
            | ProjectError::Conflict(message)
            | ProjectError::Edit(message) => f.write_str(message),
            ProjectError::Io(error) => write!(f, "{error}"),
        }
    }
}

impl std::error::Error for ProjectError {}

impl From<io::Error> for ProjectError {
    fn from(error: io::Error) -> Self {
        ProjectError::Io(error)
    }
}

impl From<EditError> for ProjectError {
    fn from(error: EditError) -> Self {
        ProjectError::Edit(error.0)
    }
}

pub type Result<T> = std::result::Result<T, ProjectError>;

/// The issues of a project as they are on disk right now, one per number.
#[derive(Debug, Default)]
pub struct Snapshot {
    pub issues: Vec<Arc<Issue>>,
    /// Files hidden because an earlier file has their number.
    pub duplicates: Vec<String>,
    pub by_number: HashMap<u64, Arc<Issue>>,
    labels: OnceLock<Vec<String>>,
}

impl Snapshot {
    fn new(issues: Vec<Arc<Issue>>, duplicates: Vec<String>) -> Self {
        let by_number = issues
            .iter()
            .map(|issue| (issue.number, issue.clone()))
            .collect();
        Snapshot {
            issues,
            duplicates,
            by_number,
            labels: OnceLock::new(),
        }
    }

    pub fn open_count(&self) -> usize {
        self.issues.iter().filter(|issue| issue.is_open).count()
    }

    pub fn closed_count(&self) -> usize {
        self.issues.len() - self.open_count()
    }

    /// Every label in use, case-insensitively sorted.
    pub fn labels(&self) -> &[String] {
        self.labels.get_or_init(|| {
            let names: BTreeSet<(String, &String)> = self
                .issues
                .iter()
                .flat_map(|issue| issue.labels.iter())
                .map(|label| (casefold(label), label))
                .collect();
            let mut sorted: Vec<String> =
                names.into_iter().map(|(_, label)| label.clone()).collect();
            sorted.dedup();
            sorted
        })
    }

    /// Issues this one blocks: its own `Blocks` list plus every issue that depends on it.
    pub fn blocking(&self, number: u64) -> Vec<u64> {
        let mut found: BTreeSet<u64> = self
            .by_number
            .get(&number)
            .map(|issue| issue.blocks.iter().copied().collect())
            .unwrap_or_default();
        found.extend(
            self.issues
                .iter()
                .filter(|other| other.depends_on.contains(&number))
                .map(|other| other.number),
        );
        found.remove(&number);
        found.into_iter().collect()
    }

    pub fn open_dependencies(&self, issue: &Issue) -> Vec<Arc<Issue>> {
        issue
            .depends_on
            .iter()
            .filter_map(|number| self.by_number.get(number))
            .filter(|dependency| dependency.is_open)
            .cloned()
            .collect()
    }
}

struct Listed {
    name: String,
    number: u64,
    modified_ns: i128,
    size: u64,
}

/// A parsed issue and the modification time and size of the file it was read from.
type Cached = (i128, u64, Arc<Issue>);

pub struct Project {
    pub config: ProjectConfig,
    cache: Mutex<HashMap<String, Cached>>,
    /// Held while writing, so two edits through this app never interleave.
    writing: Mutex<()>,
}

fn modified_ns(metadata: &fs::Metadata) -> i128 {
    i128::from(metadata.mtime()) * 1_000_000_000 + i128::from(metadata.mtime_nsec())
}

fn writable_directory(path: &Path) -> bool {
    let Ok(path) = CString::new(path.as_os_str().as_bytes()) else {
        return false;
    };
    // SAFETY: `path` is a valid NUL-terminated string that outlives the call.
    unsafe { libc::access(path.as_ptr(), libc::W_OK) == 0 }
}

impl Project {
    pub fn new(config: ProjectConfig) -> Self {
        Project {
            config,
            cache: Mutex::new(HashMap::new()),
            writing: Mutex::new(()),
        }
    }

    pub fn name(&self) -> &str {
        &self.config.name
    }

    pub fn path(&self) -> &Path {
        &self.config.path
    }

    pub fn exists(&self) -> bool {
        self.path().is_dir()
    }

    pub fn writable(&self) -> bool {
        !self.config.read_only && self.exists() && writable_directory(self.path())
    }

    fn files(&self) -> Vec<Listed> {
        let Ok(entries) = fs::read_dir(self.path()) else {
            return Vec::new();
        };
        let mut found: Vec<Listed> = entries
            .filter_map(|entry| {
                let entry = entry.ok()?;
                let name = entry.file_name().to_str()?.to_string();
                let number = FILENAME.captures(&name)?[1].parse().ok()?;
                // Follows a symbolic link, like the file it points to would be read.
                let metadata = fs::metadata(entry.path())
                    .ok()
                    .filter(fs::Metadata::is_file)?;
                Some(Listed {
                    name,
                    number,
                    modified_ns: modified_ns(&metadata),
                    size: metadata.len(),
                })
            })
            .collect();
        found.sort_by(|a, b| (a.number, &a.name).cmp(&(b.number, &b.name)));
        found
    }

    /// Changes whenever an issue file is added, removed or modified.
    pub fn signature(&self) -> String {
        let listing: String = self
            .files()
            .iter()
            .map(|file| format!("{}\0{}\0{}\n", file.name, file.modified_ns, file.size))
            .collect();
        files::digest(&listing)
    }

    fn read(&self, name: &str, number: u64) -> io::Result<Issue> {
        let path = self.path().join(name);
        let metadata = fs::metadata(&path)?;
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| Timestamp::try_from(time).ok());
        let content = files::read(&path)?;
        Ok(build(
            &path,
            number,
            content,
            modified.unwrap_or(Timestamp::UNIX_EPOCH),
        ))
    }

    pub fn snapshot(&self) -> Snapshot {
        let mut issues = Vec::new();
        let mut duplicates = Vec::new();
        let mut seen = HashSet::new();
        let mut cache = self
            .cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let listed = self.files();
        for file in &listed {
            let issue = match cache.get(&file.name) {
                Some((modified, size, issue))
                    if (*modified, *size) == (file.modified_ns, file.size) =>
                {
                    issue.clone()
                }
                _ => match self.read(&file.name, file.number) {
                    Ok(issue) => {
                        let issue = Arc::new(issue);
                        cache.insert(
                            file.name.clone(),
                            (file.modified_ns, file.size, issue.clone()),
                        );
                        issue
                    }
                    Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                    Err(error) => {
                        tracing::warn!("could not read {}/{}: {error}", self.name(), file.name);
                        continue;
                    }
                },
            };
            if seen.insert(file.number) {
                issues.push(issue);
            } else {
                duplicates.push(file.name.clone());
            }
        }
        let names: HashSet<&str> = listed.iter().map(|file| file.name.as_str()).collect();
        cache.retain(|name, _| names.contains(name.as_str()));
        Snapshot::new(issues, duplicates)
    }

    fn name_of(&self, number: u64) -> Result<String> {
        self.files()
            .into_iter()
            .find(|file| file.number == number)
            .map(|file| file.name)
            .ok_or_else(|| ProjectError::NotFound(format!("no issue #{number} in {}", self.name())))
    }

    /// The issue read from disk now, bypassing the cache.
    pub fn fresh(&self, number: u64) -> Result<Issue> {
        let name = self.name_of(number)?;
        self.read(&name, number)
            .map_err(|error| match error.kind() {
                io::ErrorKind::NotFound => {
                    ProjectError::NotFound(format!("no issue #{number} in {}", self.name()))
                }
                _ => ProjectError::Io(error),
            })
    }

    /// What a page showing this issue compares against to notice a change on disk.
    pub fn version_of(&self, number: u64) -> Result<String> {
        let name = self.name_of(number)?;
        Ok(files::read(&self.path().join(name))?.version)
    }

    fn read_only(&self) -> ProjectError {
        ProjectError::ReadOnly(format!("{} is read-only", self.name()))
    }

    /// Apply `edit(issue) -> new text` to the file and write it, if the text changed.
    ///
    /// With `expected_version`, the edit is refused when the file is no longer the version the
    /// user was looking at. Without it, the edit is applied to whatever is on disk now; a file
    /// that changes between reading and writing is read again and the edit applied again.
    pub fn update(
        &self,
        number: u64,
        edit: impl Fn(&Issue) -> std::result::Result<String, EditError>,
        expected_version: Option<&str>,
    ) -> Result<Issue> {
        if !self.writable() {
            return Err(self.read_only());
        }
        let _writing = self
            .writing
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        for _ in 0..3 {
            let issue = self.fresh(number)?;
            if !issue.editable() {
                let problems = issue.problems.join(" ");
                return Err(ProjectError::Edit(if problems.is_empty() {
                    "this file cannot be edited here".into()
                } else {
                    problems
                }));
            }
            if expected_version.is_some_and(|expected| expected != issue.version) {
                return Err(ProjectError::Conflict(format!(
                    "issue #{number} changed on disk"
                )));
            }
            let text = edit(&issue)?;
            if text == issue.file.text {
                return Ok(issue);
            }
            if parse(&text).front.error.is_some() {
                return Err(ProjectError::Edit(
                    "the edit would leave the front matter unreadable".into(),
                ));
            }
            if files::read(&issue.path)?.version != issue.version {
                continue;
            }
            files::write_atomic(&issue.path, &text, issue.newline)?;
            tracing::info!("wrote {}/{}", self.name(), issue.filename);
            let after = self.fresh(number)?;
            self.sync_index(&issue, &after);
            return Ok(after);
        }
        Err(ProjectError::Conflict(format!(
            "issue #{number} keeps changing on disk"
        )))
    }

    pub fn create(&self, title: &str, labels: &[String], description: &str) -> Result<Issue> {
        if !self.writable() {
            return Err(self.read_only());
        }
        let number = {
            let _writing = self
                .writing
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut created = None;
            for _ in 0..5 {
                let listed = self.files();
                let number = listed.iter().map(|file| file.number).max().unwrap_or(0) + 1;
                let width = listed
                    .iter()
                    .filter_map(|file| FILENAME.captures(&file.name).map(|found| found[1].len()))
                    .max()
                    .unwrap_or(3);
                let name = format!("{number:0width$}-{}.md", slugify(title));
                let path = self.path().join(&name);
                match files::create_exclusive(
                    &path,
                    &new_issue_text(number, title, labels, description),
                ) {
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                    other => other?,
                }
                // Another program may have taken the same number with a different name meanwhile.
                if self
                    .files()
                    .iter()
                    .any(|file| file.number == number && file.name != name)
                {
                    fs::remove_file(&path)?;
                    continue;
                }
                tracing::info!("created {}/{name}", self.name());
                created = Some(number);
                break;
            }
            created
                .ok_or_else(|| ProjectError::Edit("could not find a free issue number".into()))?
        };
        let issue = self.fresh(number)?;
        index::rewrite(self.path(), |readme| {
            index::appended_row(
                readme,
                issue.number,
                &issue.title,
                &issue.filename,
                &issue.labels,
                &issue.state,
                &issue.depends_on,
            )
        });
        Ok(issue)
    }

    /// Bring the README row of an issue in line with what just changed in its file.
    pub fn sync_index(&self, before: &Issue, after: &Issue) {
        let mut change = RowChange::default();
        if after.title != before.title {
            change.title = Some(after.title.clone());
            change.filename = Some(after.filename.clone());
        }
        if after.labels != before.labels {
            change.labels = Some(after.labels.clone());
        }
        if after.state != before.state {
            change.state = Some(after.state.clone());
        }
        if after.depends_on != before.depends_on {
            change.depends_on = Some(after.depends_on.clone());
        }
        if !change.is_empty() {
            index::rewrite(self.path(), |readme| {
                index::updated_row(readme, after.number, &change)
            });
        }
    }
}

fn resolved(path: &Path) -> PathBuf {
    fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

pub struct Registry {
    // Open adds projects while other requests list them: it replaces the list instead of changing
    // it, so a request that is going through the old one is not disturbed.
    projects: RwLock<Arc<Vec<Arc<Project>>>>,
    opening: Mutex<()>,
}

impl Registry {
    pub fn new(configs: Vec<ProjectConfig>) -> Self {
        let projects = configs
            .into_iter()
            .map(|config| Arc::new(Project::new(config)))
            .collect();
        Registry {
            projects: RwLock::new(Arc::new(projects)),
            opening: Mutex::new(()),
        }
    }

    /// The projects in the order they were configured, then the opened ones.
    pub fn all(&self) -> Arc<Vec<Arc<Project>>> {
        self.projects
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    pub fn get(&self, name: &str) -> Result<Arc<Project>> {
        self.all()
            .iter()
            .find(|project| project.name() == name)
            .cloned()
            .ok_or_else(|| ProjectError::NotFound(format!("no project called {}", quoted(name))))
    }

    /// The project that shows this directory, however its path was written.
    pub fn find(&self, directory: &Path) -> Option<Arc<Project>> {
        let target = resolved(directory);
        self.all()
            .iter()
            .find(|project| resolved(project.path()) == target)
            .cloned()
    }

    /// The project for `directory`, added under a new name if no project shows it yet.
    pub fn open(
        &self,
        directory: &Path,
        name_for: impl FnOnce(&HashSet<String>) -> String,
        read_only: bool,
    ) -> Arc<Project> {
        let _opening = self
            .opening
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(existing) = self.find(directory) {
            return existing;
        }
        let current = self.all();
        let taken: HashSet<String> = current
            .iter()
            .map(|project| project.name().to_string())
            .collect();
        let name = name_for(&taken);
        let project = Arc::new(Project::new(ProjectConfig {
            name: name.clone(),
            path: directory.to_path_buf(),
            read_only,
            opened: true,
        }));
        let mut projects = current.as_ref().clone();
        projects.push(project.clone());
        *self
            .projects
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Arc::new(projects);
        tracing::info!("opened {name}: {}", directory.display());
        project
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::{Path, PathBuf};

    /// A copy of one of the synthetic issue directories under `tests/fixtures`.
    pub fn fixture(name: &str, into: &Path) -> PathBuf {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        let target = into.join(name);
        std::fs::create_dir_all(&target).unwrap();
        for entry in std::fs::read_dir(source).unwrap() {
            let entry = entry.unwrap();
            std::fs::copy(entry.path(), target.join(entry.file_name())).unwrap();
        }
        target
    }
}

#[cfg(test)]
mod tests {
    use super::testing::fixture;
    use super::*;
    use crate::issuefile::{set_description, set_front};
    use std::os::unix::fs::PermissionsExt;

    fn config(name: &str, path: PathBuf, read_only: bool) -> ProjectConfig {
        ProjectConfig {
            name: name.into(),
            path,
            read_only,
            opened: false,
        }
    }

    fn demo(directory: &tempfile::TempDir) -> Project {
        Project::new(config("demo", fixture("demo", directory.path()), false))
    }

    #[test]
    fn a_snapshot_has_one_issue_per_number_and_hides_later_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let demo = demo(&directory);
        fs::write(
            demo.path().join("003-z-second-three.md"),
            "---\ntitle: dup\n---\n",
        )
        .unwrap();
        fs::write(demo.path().join("notes.md"), "not an issue").unwrap();
        fs::write(demo.path().join(".003-x.md.1234.tmp"), "half written").unwrap();
        let snapshot = demo.snapshot();
        let numbers: Vec<u64> = snapshot.issues.iter().map(|issue| issue.number).collect();
        assert_eq!(numbers, (1..=40).collect::<Vec<_>>());
        assert_eq!(snapshot.by_number[&3].filename, "003-search-notes.md");
        assert_eq!(snapshot.duplicates, ["003-z-second-three.md"]);
        assert_eq!((snapshot.open_count(), snapshot.closed_count()), (27, 13));
    }

    #[test]
    fn a_snapshot_rereads_a_file_that_changed_on_disk() {
        let directory = tempfile::tempdir().unwrap();
        let demo = demo(&directory);
        assert_eq!(
            demo.snapshot().by_number[&4].title,
            "The app crashes when a note title is empty"
        );
        let path = demo.path().join("004-crash-on-empty-title.md");
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("The app crashes", "It crashes");
        fs::write(&path, text).unwrap();
        let file = fs::File::options().write(true).open(&path).unwrap();
        file.set_modified(std::time::SystemTime::UNIX_EPOCH)
            .unwrap();
        assert_eq!(
            demo.snapshot().by_number[&4].title,
            "It crashes when a note title is empty"
        );
    }

    #[test]
    fn blocking_combines_the_blocks_line_with_the_issues_that_depend_on_this_one() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = demo(&directory).snapshot();
        assert_eq!(snapshot.blocking(3), [5, 8]);
        assert_eq!(snapshot.blocking(2), [3, 8]);
        let open: Vec<u64> = snapshot
            .open_dependencies(&snapshot.by_number[&5])
            .iter()
            .map(|issue| issue.number)
            .collect();
        assert_eq!(open, [3]);
    }

    #[test]
    fn labels_are_sorted_without_regard_to_case() {
        let directory = tempfile::tempdir().unwrap();
        let snapshot = demo(&directory).snapshot();
        let labels = snapshot.labels();
        let mut sorted = labels.to_vec();
        sorted.sort_by_key(|label| casefold(label));
        assert_eq!(labels, sorted.as_slice());
        assert!(labels.contains(&"enhancement".to_string()));
    }

    #[test]
    fn an_update_keeps_windows_newlines_and_the_file_mode() {
        let directory = tempfile::tempdir().unwrap();
        let demo = demo(&directory);
        let path = demo.path().join("004-crash-on-empty-title.md");
        let crlf = fs::read_to_string(&path).unwrap().replace('\n', "\r\n");
        fs::write(&path, crlf).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
        demo.update(4, |issue| set_front(&issue.file, "state", "done"), None)
            .unwrap();
        let data = fs::read_to_string(&path).unwrap();
        assert!(data.contains("state: done\r\n"));
        assert!(!data.replace("\r\n", "").contains('\n'));
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o640
        );
        let leftovers = fs::read_dir(demo.path())
            .unwrap()
            .filter(|entry| {
                entry
                    .as_ref()
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".tmp")
            })
            .count();
        assert_eq!(leftovers, 0);
    }

    #[test]
    fn an_update_based_on_an_old_version_is_a_conflict() {
        let directory = tempfile::tempdir().unwrap();
        let demo = demo(&directory);
        let version = demo.fresh(4).unwrap().version;
        let path = demo.path().join("004-crash-on-empty-title.md");
        let text = fs::read_to_string(&path).unwrap() + "\nEdited elsewhere.\n";
        fs::write(&path, text).unwrap();
        let result = demo.update(
            4,
            |issue| set_description(&issue.file, "mine"),
            Some(&version),
        );
        assert!(matches!(result, Err(ProjectError::Conflict(_))));
        assert!(
            fs::read_to_string(&path)
                .unwrap()
                .contains("Edited elsewhere.")
        );
    }

    #[test]
    fn an_update_syncs_the_readme_row() {
        let directory = tempfile::tempdir().unwrap();
        let demo = demo(&directory);
        demo.update(3, |issue| set_front(&issue.file, "state", "done"), None)
            .unwrap();
        let readme = fs::read_to_string(demo.path().join("README.md")).unwrap();
        assert!(
            readme.contains(
                "| 3 | [Search notes](003-search-notes.md) | enhancement, ui | done | #2 |"
            )
        );
        assert!(readme.ends_with("Order of work: 1 → 2 → 3.\n"));
    }

    #[test]
    fn create_takes_the_next_number_and_appends_a_readme_row() {
        let directory = tempfile::tempdir().unwrap();
        let demo = demo(&directory);
        let issue = demo
            .create("Export notes: CSV", &["data".into()], "Body text")
            .unwrap();
        assert_eq!(issue.number, 41);
        assert_eq!(issue.filename, "041-export-notes-csv.md");
        let text = fs::read_to_string(demo.path().join("041-export-notes-csv.md")).unwrap();
        assert!(text.starts_with(
            "---\ntitle: \"Export notes: CSV\"\nlabels: [data]\nstate: open\n---\n\n# #41 Export notes: CSV\n"
        ));
        let readme = fs::read_to_string(demo.path().join("README.md")).unwrap();
        assert!(readme.contains(
            "| 4 | [Crash on empty title](004-crash-on-empty-title.md) | bug | open | – |\n\
             | 41 | [Export notes: CSV](041-export-notes-csv.md) | data | open | – |\n"
        ));
    }

    #[test]
    fn a_read_only_project_refuses_to_write() {
        let directory = tempfile::tempdir().unwrap();
        let project = Project::new(config("demo", fixture("demo", directory.path()), true));
        let update = project.update(4, |issue| Ok(format!("{}x", issue.file.text)), None);
        assert!(matches!(update, Err(ProjectError::ReadOnly(_))));
        assert!(matches!(
            project.create("x", &[], ""),
            Err(ProjectError::ReadOnly(_))
        ));
    }

    #[test]
    fn a_missing_directory_is_an_empty_project() {
        let directory = tempfile::tempdir().unwrap();
        let project = Project::new(config("gone", directory.path().join("gone"), false));
        assert!(!project.exists());
        assert!(!project.writable());
        assert!(project.snapshot().issues.is_empty());
    }

    #[test]
    fn opening_a_directory_twice_gives_the_same_project() {
        let directory = tempfile::tempdir().unwrap();
        let path = fixture("demo", directory.path());
        let registry = Registry::new(vec![config("demo", path.clone(), false)]);
        let again = registry.open(&path.join("."), |_| unreachable!(), false);
        assert_eq!(again.name(), "demo");
        let archive = fixture("archive", directory.path());
        let opened = registry.open(&archive, |taken| format!("archive-{}", taken.len()), true);
        assert_eq!(opened.name(), "archive-1");
        assert!(opened.config.opened);
        assert_eq!(registry.all().len(), 2);
        assert!(registry.get("archive-1").is_ok());
        assert!(matches!(
            registry.get("nope"),
            Err(ProjectError::NotFound(_))
        ));
    }
}
