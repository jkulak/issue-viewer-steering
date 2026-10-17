//! What a project's git repository says about the work on its issues: the local branches named
//! after an issue, the worktrees they are checked out in, and when they last moved.
//!
//! Git is only read. No optional locks, so a `status` never refreshes the index under someone who
//! is working in that worktree; no fsmonitor hook; a time limit on every call; and the answer is
//! kept for a few seconds, so pages that poll do not run git on every check.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use jiff::Timestamp;
use regex::Regex;

use crate::files;

/// `004-kanban-board`, `feature/4-board`: the issue number starts the last part of the name.
static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|/)0*([0-9]{1,9})-([^/]*)$").unwrap());
static NOT_WORD: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9]+").unwrap());

/// Longer than any of these commands takes on a healthy repository.
const LIMIT: Duration = Duration::from_secs(3);
/// Which repository a directory is in hardly ever changes.
const MEMBERSHIP: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: PathBuf,
    /// Files changed and not committed, untracked ones included.
    pub changes: usize,
    /// When the most recently changed of those files was written.
    pub edited: Option<Timestamp>,
}

impl Worktree {
    pub fn name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.display().to_string(),
            |name| name.to_string_lossy().into_owned(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// The issue number the name starts with.
    pub issue: Option<u64>,
    /// The words after the number, to tell apart two projects' issues with the same number.
    pub words: Vec<String>,
    pub committed: Timestamp,
    /// Commits not on the default branch; 0 when there is no default branch to compare with.
    pub ahead: u32,
    pub is_default: bool,
    pub worktree: Option<Worktree>,
}

impl Branch {
    fn new(name: &str, committed: Timestamp, ahead: u32, is_default: bool) -> Self {
        let (issue, words) = match NUMBERED.captures(name) {
            Some(found) => (found[1].parse().ok(), words(&found[2])),
            None => (None, Vec::new()),
        };
        Branch {
            name: name.to_string(),
            issue,
            words,
            committed,
            ahead,
            is_default,
            worktree: None,
        }
    }

    /// The last sign of work: the last commit, or a file changed in the worktree since.
    pub fn active(&self) -> Timestamp {
        self.worktree
            .as_ref()
            .and_then(|worktree| worktree.edited)
            .map_or(self.committed, |edited| edited.max(self.committed))
    }
}

/// Lower-case words of a name, in order.
pub fn words(text: &str) -> Vec<String> {
    NOT_WORD
        .split(&text.to_lowercase())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

#[derive(Debug)]
pub struct Repository {
    /// The git directory every worktree of the repository shares; two projects with the same one
    /// see the same branches.
    pub common_dir: PathBuf,
    pub default_branch: Option<String>,
    pub branches: Vec<Branch>,
}

impl Repository {
    /// The folder the repository is in, to name it on the page.
    pub fn name(&self) -> String {
        let dir = &self.common_dir;
        let named = if dir.file_name().is_some_and(|name| name == ".git") {
            dir.parent().unwrap_or(dir)
        } else {
            dir
        };
        named.file_name().map_or_else(
            || named.display().to_string(),
            |name| name.to_string_lossy().trim_end_matches(".git").to_string(),
        )
    }

    /// Changes when a branch moves, appears or goes, or the number of files changed in one of its
    /// worktrees does. Not with every save: a page that reloads on this would never sit still
    /// while someone is working.
    pub fn signature(&self) -> String {
        let mut listing = String::new();
        for branch in &self.branches {
            let worktree = branch.worktree.as_ref();
            listing.push_str(&format!(
                "{}\0{}\0{}\0{}\0{}\n",
                branch.name,
                branch.committed.as_second(),
                branch.ahead,
                worktree
                    .map(|w| w.path.display().to_string())
                    .unwrap_or_default(),
                worktree.map_or(0, |w| w.changes),
            ));
        }
        files::digest(&listing)
    }
}

/// Run git read-only in `directory`; None when it fails, is missing, or takes too long.
fn git(directory: &Path, args: &[&str]) -> Option<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(directory)
        .args(["--no-optional-locks", "-c", "core.fsmonitor=false"])
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_COMMON_DIR")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    // Read while waiting, so a long answer cannot fill the pipe and stall git.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        stdout.read_to_end(&mut out).map(|_| out)
    });
    let deadline = Instant::now() + LIMIT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let out = reader.join().ok()?.ok()?;
                return status
                    .success()
                    .then(|| String::from_utf8_lossy(&out).into_owned());
            }
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(5)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                tracing::warn!(
                    "git {} in {} took too long",
                    args.join(" "),
                    directory.display()
                );
                return None;
            }
        }
    }
}

fn common_dir(directory: &Path) -> Option<PathBuf> {
    let out = git(
        directory,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    )?;
    let path = out.trim_end_matches('\n');
    (!path.is_empty()).then(|| PathBuf::from(path))
}

fn modified(path: &Path) -> Option<Timestamp> {
    let time = std::fs::symlink_metadata(path).ok()?.modified().ok()?;
    Timestamp::try_from(time).ok()
}

/// What is changed and not committed in a worktree.
fn worktree(path: &Path) -> Worktree {
    let status = git(path, &["status", "--porcelain=v1", "-z"]).unwrap_or_default();
    let mut entries = status.split('\0').filter(|entry| entry.len() > 3);
    let (mut changes, mut edited) = (0, None);
    while let Some(entry) = entries.next() {
        changes += 1;
        edited = edited.max(modified(&path.join(&entry[3..])));
        // A rename or copy is followed by the name it had before.
        if matches!(entry.as_bytes()[0], b'R' | b'C') {
            entries.next();
        }
    }
    Worktree {
        path: path.to_path_buf(),
        changes,
        edited,
    }
}

/// The local branches, compared with the first of main, master that exists.
fn branches(directory: &Path) -> Option<(Option<String>, Vec<Branch>)> {
    let format = |base: Option<&str>| {
        let ahead = base.map_or(String::new(), |base| format!("%(ahead-behind:{base})"));
        format!("--format=%(refname:short)%00%(committerdate:unix)%00%(worktreepath)%00{ahead}")
    };
    // Naming a base that does not exist makes for-each-ref fail; try the next one.
    let (base, out) = ["main", "master"]
        .into_iter()
        .find_map(|base| {
            git(
                directory,
                &["for-each-ref", &format(Some(base)), "refs/heads/"],
            )
            .map(|out| (Some(base), out))
        })
        .or_else(|| {
            git(directory, &["for-each-ref", &format(None), "refs/heads/"]).map(|out| (None, out))
        })?;
    let mut found = Vec::new();
    for line in out.lines() {
        let fields: Vec<&str> = line.split('\0').collect();
        let [name, committed, worktree_path, ahead_behind, ..] = fields.as_slice() else {
            continue;
        };
        let committed = committed
            .parse()
            .ok()
            .and_then(|seconds| Timestamp::from_second(seconds).ok())
            .unwrap_or(Timestamp::UNIX_EPOCH);
        let ahead = ahead_behind
            .split(' ')
            .next()
            .and_then(|ahead| ahead.parse().ok())
            .unwrap_or(0);
        let mut branch = Branch::new(name, committed, ahead, Some(*name) == base);
        let path = Path::new(worktree_path);
        if !worktree_path.is_empty() && path.is_dir() {
            branch.worktree = Some(worktree(path));
        }
        found.push(branch);
    }
    Some((base.map(str::to_string), found))
}

type Kept<T> = (Instant, T);

/// The repositories of the projects, read at most once every `ttl`.
pub struct Cache {
    ttl: Duration,
    common_dirs: Mutex<HashMap<PathBuf, Kept<Option<PathBuf>>>>,
    repositories: Mutex<HashMap<PathBuf, Kept<Option<Arc<Repository>>>>>,
}

impl Cache {
    pub fn new(ttl: Duration) -> Self {
        Cache {
            ttl,
            common_dirs: Mutex::new(HashMap::new()),
            repositories: Mutex::new(HashMap::new()),
        }
    }

    fn kept<T: Clone>(
        map: &Mutex<HashMap<PathBuf, Kept<T>>>,
        key: &Path,
        ttl: Duration,
        read: impl FnOnce() -> T,
    ) -> T {
        let lock = || map.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some((when, value)) = lock().get(key)
            && when.elapsed() < ttl
        {
            return value.clone();
        }
        // Read without holding the lock: a slow repository does not hold up the others.
        let value = read();
        lock().insert(key.to_path_buf(), (Instant::now(), value.clone()));
        value
    }

    /// The git directory of the repository `directory` is in, or None outside one.
    pub fn common_dir(&self, directory: &Path) -> Option<PathBuf> {
        Cache::kept(&self.common_dirs, directory, MEMBERSHIP, || {
            common_dir(directory)
        })
    }

    /// The repository `directory` is in, or None outside one (or without git).
    pub fn repository(&self, directory: &Path) -> Option<Arc<Repository>> {
        let common = self.common_dir(directory)?;
        Cache::kept(&self.repositories, &common, self.ttl, || {
            let (default_branch, branches) = branches(directory)?;
            Some(Arc::new(Repository {
                common_dir: common.clone(),
                default_branch,
                branches,
            }))
        })
    }
}

#[cfg(test)]
pub(crate) mod testing {
    use std::path::Path;
    use std::process::Command;

    /// Run git in `directory` for a test, with a fixed author and no global configuration.
    pub fn run(directory: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(directory)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .stdout(std::process::Stdio::null())
            .status()
            .unwrap();
        assert!(status.success(), "git {args:?}");
    }

    /// A repository with one commit on main.
    pub fn repository(directory: &Path) {
        run(directory, &["init", "--quiet", "--initial-branch=main"]);
        std::fs::write(directory.join("README.md"), "# Project\n").unwrap();
        run(directory, &["add", "README.md"]);
        run(directory, &["commit", "--quiet", "-m", "Start"]);
    }
}

#[cfg(test)]
mod tests {
    use super::testing::{repository, run};
    use super::*;

    #[test]
    fn a_branch_belongs_to_the_issue_its_last_part_starts_with() {
        let branch = |name: &str| Branch::new(name, Timestamp::UNIX_EPOCH, 0, false);
        assert_eq!(branch("004-kanban-board").issue, Some(4));
        assert_eq!(branch("004-kanban-board").words, ["kanban", "board"]);
        assert_eq!(branch("feature/12-Export_CSV").issue, Some(12));
        assert_eq!(branch("feature/12-Export_CSV").words, ["export", "csv"]);
        for name in ["main", "fix-typo", "12", "v2-12-x/later", "release/2026.10"] {
            assert_eq!(branch(name).issue, None, "{name}");
        }
    }

    #[test]
    fn a_directory_outside_a_repository_has_none() {
        let directory = tempfile::tempdir().unwrap();
        let cache = Cache::new(Duration::ZERO);
        assert!(cache.repository(directory.path()).is_none());
    }

    #[test]
    fn branches_say_how_far_ahead_they_are_and_where_they_are_checked_out() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("project");
        std::fs::create_dir(&root).unwrap();
        repository(&root);
        run(&root, &["branch", "003-old-idea"]);
        run(&root, &["checkout", "--quiet", "-b", "004-kanban-board"]);
        std::fs::write(root.join("board.txt"), "one\n").unwrap();
        run(&root, &["add", "board.txt"]);
        run(&root, &["commit", "--quiet", "-m", "Board"]);
        std::fs::write(root.join("board.txt"), "two\n").unwrap();
        std::fs::write(root.join("new.txt"), "new\n").unwrap();
        let elsewhere = directory.path().join("elsewhere");
        run(
            &root,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "fix-typo",
                elsewhere.to_str().unwrap(),
            ],
        );
        let tasks = root.join("tasks");
        std::fs::create_dir(&tasks).unwrap();
        std::fs::write(tasks.join("004-kanban-board.md"), "# Board\n").unwrap();

        let cache = Cache::new(Duration::from_secs(60));
        let found = cache.repository(&tasks).unwrap();
        assert_eq!(found.default_branch.as_deref(), Some("main"));
        let by_name = |name: &str| {
            found
                .branches
                .iter()
                .find(|branch| branch.name == name)
                .unwrap()
        };
        let board = by_name("004-kanban-board");
        assert_eq!((board.issue, board.ahead), (Some(4), 1));
        let worktree = board.worktree.as_ref().unwrap();
        assert_eq!(
            worktree.path.canonicalize().unwrap(),
            root.canonicalize().unwrap()
        );
        assert_eq!(worktree.changes, 3); // board.txt, new.txt and tasks/
        assert!(worktree.edited.is_some());
        assert_eq!(
            board.active(),
            worktree.edited.unwrap().max(board.committed)
        );
        assert!(by_name("003-old-idea").worktree.is_none());
        assert_eq!(by_name("003-old-idea").ahead, 0);
        assert!(by_name("main").is_default);
        let typo = by_name("fix-typo");
        assert_eq!(typo.issue, None);
        assert_eq!(typo.worktree.as_ref().unwrap().changes, 0);

        // The other worktree is the same repository, and the answer is kept.
        let again = cache.repository(&elsewhere).unwrap();
        assert!(Arc::ptr_eq(&found, &again));
        assert_eq!(found.name(), "project");
    }

    #[test]
    fn the_signature_changes_when_a_branch_moves() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        repository(root);
        let cache = Cache::new(Duration::ZERO);
        let before = cache.repository(root).unwrap().signature();
        assert_eq!(cache.repository(root).unwrap().signature(), before);
        run(root, &["branch", "007-thing"]);
        assert_ne!(cache.repository(root).unwrap().signature(), before);
    }

    #[test]
    fn a_repository_without_main_or_master_still_lists_its_branches() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        run(root, &["init", "--quiet", "--initial-branch=trunk"]);
        std::fs::write(root.join("a.txt"), "a\n").unwrap();
        run(root, &["add", "a.txt"]);
        run(root, &["commit", "--quiet", "-m", "Start"]);
        run(root, &["branch", "002-thing"]);
        let found = Cache::new(Duration::ZERO).repository(root).unwrap();
        assert_eq!(found.default_branch, None);
        assert!(found.branches.iter().any(|branch| branch.issue == Some(2)));
    }
}
