//! Issues as the viewer shows them, built from one file each.

use std::path::{Path, PathBuf};
use std::sync::{LazyLock, OnceLock};

use jiff::Timestamp;
use regex::Regex;
use unicode_normalization::UnicodeNormalization;
use yaml_rust2::Yaml;

use crate::files::FileText;
use crate::issuefile::{Comment, IssueFile, lines, parse, task_counts};
use crate::text::{casefold, issue_references, yaml_text, yaml_truthy};

pub static FILENAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9]+)(?:-[^/\\]*)?\.md$").unwrap());
static RELATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\*\*(Depends on|Blocked by|Blocks|Blocking):\*\*").unwrap());
static NOT_SLUG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9]+").unwrap());
static LEADING_NUMBER: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^[0-9]+-?").unwrap());

const COMPLETED: [&str; 7] = [
    "done",
    "closed",
    "complete",
    "completed",
    "resolved",
    "fixed",
    "merged",
];
const NOT_PLANNED: [&str; 13] = [
    "not-planned",
    "not planned",
    "not_planned",
    "wontfix",
    "won't fix",
    "wont-fix",
    "cancelled",
    "canceled",
    "duplicate",
    "invalid",
    "rejected",
    "obsolete",
    "abandoned",
];
// What the viewer writes. `done` is what the task files already use for a finished issue.
pub const STATE_OPEN: &str = "open";
pub const STATE_DONE: &str = "done";
pub const STATE_NOT_PLANNED: &str = "not-planned";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    Completed,
    NotPlanned,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Completed => "completed",
            Reason::NotPlanned => "not_planned",
        }
    }
}

/// (is open, reason) for a state value; anything not known to be closed counts as open.
pub fn classify(state: &str) -> (bool, Option<Reason>) {
    let value = state.trim().to_lowercase();
    if COMPLETED.contains(&value.as_str()) {
        (false, Some(Reason::Completed))
    } else if NOT_PLANNED.contains(&value.as_str()) {
        (false, Some(Reason::NotPlanned))
    } else {
        (true, None)
    }
}

pub fn normalise_labels(value: Option<&Yaml>) -> Vec<String> {
    let items: Vec<String> = match value {
        None | Some(Yaml::Null) => Vec::new(),
        Some(Yaml::String(text)) => text.split(',').map(str::to_string).collect(),
        Some(Yaml::Array(items)) => items
            .iter()
            .filter(|item| !matches!(item, Yaml::Null))
            .map(yaml_text)
            .collect(),
        Some(other) => vec![yaml_text(other)],
    };
    let mut labels: Vec<String> = Vec::new();
    for label in items {
        let label = label.trim();
        if !label.is_empty() && !labels.iter().any(|known| known == label) {
            labels.push(label.to_string());
        }
    }
    labels
}

fn push_unique(list: &mut Vec<u64>, numbers: impl IntoIterator<Item = u64>) {
    for number in numbers {
        if !list.contains(&number) {
            list.push(number);
        }
    }
}

/// (depends on, blocks) from the first `**Depends on:** … · **Blocks:** …` line.
pub fn relationships(description: &str) -> (Vec<u64>, Vec<u64>) {
    for line in lines(description, 0) {
        if line.fenced {
            continue;
        }
        let text = line.text(description);
        let labels: Vec<_> = RELATION.captures_iter(text).collect();
        if labels.is_empty() {
            continue;
        }
        let (mut depends, mut blocks) = (Vec::new(), Vec::new());
        for (index, label) in labels.iter().enumerate() {
            let start = label.get(0).unwrap().end();
            let end = labels
                .get(index + 1)
                .map_or(text.len(), |next| next.get(0).unwrap().start());
            let numbers = issue_references(&text[start..end])
                .into_iter()
                .map(|(_, n)| n);
            let kind = label[1].to_lowercase();
            if kind == "depends on" || kind == "blocked by" {
                push_unique(&mut depends, numbers);
            } else {
                push_unique(&mut blocks, numbers);
            }
        }
        return (depends, blocks);
    }
    (Vec::new(), Vec::new())
}

pub fn slugify(title: &str) -> String {
    const LIMIT: usize = 60;
    let text = title.replace('ł', "l").replace('Ł', "L").replace('ß', "ss");
    let ascii: String = text.nfkd().filter(char::is_ascii).collect();
    let mut slug = NOT_SLUG
        .replace_all(&ascii.to_lowercase(), "-")
        .trim_matches('-')
        .to_string();
    if slug.len() > LIMIT {
        let cut = &slug[..LIMIT];
        let shorter = cut.rfind('-').map_or(cut, |position| &cut[..position]);
        slug = if shorter.is_empty() { cut } else { shorter }.to_string();
    }
    if slug.is_empty() {
        "issue".into()
    } else {
        slug
    }
}

fn title_from_filename(filename: &str) -> String {
    let stem = filename.strip_suffix(".md").unwrap_or(filename);
    let stem = LEADING_NUMBER.replace(stem, "").replace('-', " ");
    let stem = stem.trim();
    let mut chars = stem.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => filename.to_string(),
    }
}

#[derive(Debug)]
pub struct Issue {
    pub number: u64,
    pub filename: String,
    pub path: PathBuf,
    pub title: String,
    pub labels: Vec<String>,
    /// The raw value from the front matter, lower case.
    pub state: String,
    pub is_open: bool,
    /// Why a closed issue was closed.
    pub reason: Option<Reason>,
    pub file: IssueFile,
    pub newline: &'static str,
    pub version: String,
    pub readable: bool,
    pub updated: Timestamp,
    pub depends_on: Vec<u64>,
    pub blocks: Vec<u64>,
    pub tasks_done: usize,
    pub tasks_total: usize,
    pub problems: Vec<String>,
    search_text: OnceLock<String>,
}

impl Issue {
    pub fn description(&self) -> &str {
        self.file.description_text()
    }

    pub fn comments(&self) -> &[Comment] {
        &self.file.comments
    }

    pub fn comment_text(&self, comment: &Comment) -> &str {
        self.file.comment_text(comment)
    }

    pub fn editable(&self) -> bool {
        self.readable && self.file.front.error.is_none()
    }

    /// True when the raw state says more than open or closed, e.g. `in-progress`.
    pub fn state_is_custom(&self) -> bool {
        self.state != STATE_OPEN && self.state != STATE_DONE
    }

    pub fn search_text(&self) -> &str {
        self.search_text.get_or_init(|| {
            casefold(&format!(
                "#{} {}\n{}",
                self.number, self.title, self.file.text
            ))
        })
    }
}

impl AsRef<Issue> for Issue {
    fn as_ref(&self) -> &Issue {
        self
    }
}

pub fn build(path: &Path, number: u64, content: FileText, modified: Timestamp) -> Issue {
    let parsed = parse(&content.text);
    let front = &parsed.front;
    let mut problems = Vec::new();
    if let Some(error) = &front.error {
        problems.push(format!("The front matter could not be read: {error}."));
    }
    if !content.readable {
        problems.push(
            "The file is not valid UTF-8, so it is shown with replacement characters.".into(),
        );
    }
    let filename = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_string();
    let mut title = front
        .get("title")
        .map(|value| yaml_text(value).trim().to_string())
        .unwrap_or_default();
    if title.is_empty() {
        title = parsed
            .heading_title
            .clone()
            .filter(|t| !t.is_empty())
            .unwrap_or_default();
    }
    if title.is_empty() {
        title = title_from_filename(&filename);
    }
    let state = match front.get("state") {
        Some(value) if yaml_truthy(value) => yaml_text(value),
        _ => STATE_OPEN.to_string(),
    };
    let state = state.trim().to_lowercase();
    let (is_open, reason) = classify(&state);
    let (depends_on, blocks) = relationships(parsed.description_text());
    let (tasks_done, tasks_total) = task_counts(parsed.description_text());
    let labels = normalise_labels(front.get("labels"));
    Issue {
        number,
        filename,
        path: path.to_path_buf(),
        title,
        labels,
        state,
        is_open,
        reason,
        newline: content.newline,
        version: content.version,
        readable: content.readable,
        updated: modified,
        depends_on,
        blocks,
        tasks_done,
        tasks_total,
        problems,
        file: parsed,
        search_text: OnceLock::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issue_from(text: &str, name: &str) -> Issue {
        let content = FileText {
            text: text.into(),
            newline: "\n",
            version: "v1".into(),
            readable: true,
        };
        build(
            &Path::new("/issues").join(name),
            7,
            content,
            Timestamp::from_second(1_700_000_000).unwrap(),
        )
    }

    fn issue(text: &str) -> Issue {
        issue_from(text, "007-some-thing.md")
    }

    #[test]
    fn open_is_open_and_the_rest_are_the_two_kinds_of_closed() {
        assert_eq!(classify("open"), (true, None));
        assert_eq!(classify("Done"), (false, Some(Reason::Completed)));
        assert_eq!(classify("closed"), (false, Some(Reason::Completed)));
        assert_eq!(classify("wontfix"), (false, Some(Reason::NotPlanned)));
        assert_eq!(classify("not-planned"), (false, Some(Reason::NotPlanned)));
        assert_eq!(classify("in-progress"), (true, None));
    }

    #[test]
    fn labels_can_be_a_list_a_comma_separated_string_or_missing() {
        let string = |s: &str| Yaml::String(s.into());
        let list = Yaml::Array(vec![
            string("bug"),
            string(" ui "),
            string(""),
            Yaml::Null,
            string("bug"),
        ]);
        assert_eq!(normalise_labels(Some(&list)), ["bug", "ui"]);
        assert_eq!(normalise_labels(Some(&string("bug, ui"))), ["bug", "ui"]);
        assert!(normalise_labels(None).is_empty());
        assert_eq!(normalise_labels(Some(&Yaml::Integer(2026))), ["2026"]);
    }

    #[test]
    fn relationships_come_from_the_depends_on_and_blocks_line() {
        assert_eq!(
            relationships("Intro\n\n**Depends on:** #2, #3 · **Blocks:** #9\n"),
            (vec![2, 3], vec![9])
        );
        assert_eq!(
            relationships("**Depends on:** – · **Blocks:** –"),
            (vec![], vec![])
        );
        assert_eq!(relationships("**Blocked by:** #4"), (vec![4], vec![]));
        assert_eq!(
            relationships("```\n**Depends on:** #1\n```\n"),
            (vec![], vec![])
        );
    }

    #[test]
    fn a_slug_is_short_ascii_kebab_case() {
        assert_eq!(
            slugify("Zażółć gęślą jaźń: łódź!"),
            "zazolc-gesla-jazn-lodz"
        );
        assert_eq!(slugify("   "), "issue");
        let long = slugify(&"word ".repeat(40));
        assert!(long.len() <= 60);
        assert!(!long.ends_with('-'));
        assert_eq!(
            slugify("Ünïcödé — Title, with: punctuation & more!"),
            "unicode-title-with-punctuation-more"
        );
    }

    #[test]
    fn an_issue_takes_title_labels_and_state_from_the_front_matter() {
        let issue = issue(
            "---\ntitle: \"Export\"\nlabels: [a, b]\nstate: done\n---\n\n# #7 Export\n\n\
             **Depends on:** #1 · **Blocks:** –\n\n- [x] one\n- [ ] two\n",
        );
        assert_eq!(
            (issue.title.as_str(), issue.state.as_str()),
            ("Export", "done")
        );
        assert_eq!(issue.labels, ["a", "b"]);
        assert!(!issue.is_open);
        assert_eq!(issue.reason, Some(Reason::Completed));
        assert_eq!(issue.depends_on, [1]);
        assert_eq!((issue.tasks_done, issue.tasks_total), (1, 2));
        assert!(issue.editable());
        assert!(!issue.state_is_custom());
    }

    #[test]
    fn a_title_falls_back_to_the_heading_and_then_to_the_file_name() {
        assert_eq!(issue("# Plain heading\n\nText\n").title, "Plain heading");
        assert_eq!(
            issue_from("Just text\n", "007-some-thing.md").title,
            "Some thing"
        );
    }

    #[test]
    fn a_missing_state_means_open() {
        let issue = issue("---\ntitle: x\n---\n");
        assert_eq!(issue.state, "open");
        assert!(issue.is_open);
    }

    #[test]
    fn an_unreadable_front_matter_is_a_problem_and_blocks_editing() {
        let issue = issue("---\ntitle: [broken\n---\n\n# #7 Broken\n");
        assert!(!issue.problems.is_empty());
        assert!(!issue.editable());
        assert_eq!(issue.title, "Broken");
    }

    #[test]
    fn the_search_text_is_folded_and_includes_the_number() {
        let issue = issue("---\ntitle: Straße\n---\n");
        assert!(issue.search_text().starts_with("#7 strasse\n"));
    }
}
