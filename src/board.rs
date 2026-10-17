//! The kanban board: which column an issue belongs in, and what moving it there writes.
//!
//! A column comes from the issue's own file first: a closed state, then status words in its labels
//! or in a custom state. Without one, a branch that is being worked on puts an open issue in
//! progress (see `git`), and anything else waits in the backlog for a decision.

use std::time::Duration;

use jiff::Timestamp;

use crate::frontmatter::{render_list, render_scalar, style_of};
use crate::issuefile::{EditError, parse, set_front};
use crate::issues::{Issue, STATE_DONE, STATE_OPEN};
use crate::text::casefold;

/// The columns, left to right. The order is also the precedence: when an issue carries status
/// words of two columns, the one further right wins, so nothing that waits for a person hides
/// behind an older label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Column {
    Backlog,
    Selected,
    InProgress,
    Testing,
    Review,
    Blocked,
    Done,
}

impl Column {
    pub const ALL: [Column; 7] = [
        Column::Backlog,
        Column::Selected,
        Column::InProgress,
        Column::Testing,
        Column::Review,
        Column::Blocked,
        Column::Done,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Column::Backlog => "backlog",
            Column::Selected => "selected",
            Column::InProgress => "in-progress",
            Column::Testing => "testing",
            Column::Review => "review",
            Column::Blocked => "blocked",
            Column::Done => "done",
        }
    }

    pub fn from_key(key: &str) -> Option<Column> {
        Column::ALL.into_iter().find(|column| column.key() == key)
    }

    pub fn title(self) -> &'static str {
        match self {
            Column::Backlog => "Backlog",
            Column::Selected => "Selected for development",
            Column::InProgress => "In progress",
            Column::Testing => "Testing",
            Column::Review => "Review",
            Column::Blocked => "Blocked",
            Column::Done => "Done",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Column::Backlog => "Waiting for a decision whether to do it",
            Column::Selected => "Decided, waiting to be picked up",
            Column::InProgress => "Being worked on",
            Column::Testing => "Built, being tested",
            Column::Review => "Waiting for a review or a merge",
            Column::Blocked => "Waiting for a decision or an action",
            Column::Done => "Finished or dropped",
        }
    }

    /// The columns where the next move is the owner's, not whoever works on the issue.
    pub fn needs_you(self) -> bool {
        matches!(self, Column::Backlog | Column::Review | Column::Blocked)
    }

    /// The status words that put an issue in this column, already normalised.
    fn words(self) -> &'static [&'static str] {
        match self {
            Column::Backlog => &["backlog", "later", "icebox", "someday"],
            Column::Selected => &["selected", "ready", "todo", "to-do", "approved", "next"],
            Column::InProgress => &["in-progress", "doing", "wip", "started"],
            Column::Testing => &["testing", "in-testing", "qa"],
            Column::Review => &["review", "in-review", "needs-review", "awaiting-review"],
            Column::Blocked => &["blocked", "on-hold", "waiting", "needs-decision"],
            Column::Done => &["done"],
        }
    }

    /// The label a move writes; Done is a state instead.
    pub fn label(self) -> Option<&'static str> {
        (self != Column::Done).then(|| self.words()[0])
    }
}

/// A word compared loosely: case-insensitive, with spaces and underscores read as `-`.
fn normalised(word: &str) -> String {
    let folded = casefold(word.trim()).replace([' ', '_'], "-");
    let mut out = String::with_capacity(folded.len());
    for char in folded.chars() {
        if !(char == '-' && out.ends_with('-')) {
            out.push(char);
        }
    }
    out
}

/// The column a label or a state value names, if it is a status word.
pub fn column_named(word: &str) -> Option<Column> {
    let word = normalised(word);
    Column::ALL
        .into_iter()
        .find(|column| column.words().contains(&word.as_str()))
}

pub fn is_status_label(label: &str) -> bool {
    column_named(label).is_some()
}

/// The column the issue's own file gives it, if it gives one.
pub fn stated_column(issue: &Issue) -> Option<Column> {
    if !issue.is_open {
        return Some(Column::Done);
    }
    issue
        .labels
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(issue.state.as_str()))
        .filter_map(column_named)
        .max()
}

/// Why a card is in its column.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Placement {
    /// The labels or the state say so.
    Stated,
    /// A branch for the issue is being worked on.
    Inferred,
    /// Nothing says anything: the issue waits for a decision.
    Undecided,
    /// An open pull request that closes the issue says how far it got.
    PullRequest,
}

impl Placement {
    pub fn as_str(self) -> &'static str {
        match self {
            Placement::Stated => "stated",
            Placement::Inferred => "inferred",
            Placement::Undecided => "undecided",
            Placement::PullRequest => "pull-request",
        }
    }
}

/// A branch older than this, with no worktree, is not taken as work in progress.
pub const RECENT: Duration = Duration::from_secs(14 * 24 * 60 * 60);

/// What a branch for an issue says about the work on it.
#[derive(Debug, Clone, Copy)]
pub struct WorkSignal {
    pub checked_out: bool,
    pub ahead: u32,
    pub committed: Timestamp,
}

impl WorkSignal {
    pub fn is_work(&self, now: Timestamp) -> bool {
        let recent = now
            .as_second()
            .saturating_sub(self.committed.as_second())
            .unsigned_abs()
            <= RECENT.as_secs();
        self.checked_out || (self.ahead > 0 && recent)
    }
}

pub fn place(issue: &Issue, work: &[WorkSignal], now: Timestamp) -> (Column, Placement) {
    if let Some(column) = stated_column(issue) {
        return (column, Placement::Stated);
    }
    if work.iter().any(|signal| signal.is_work(now)) {
        return (Column::InProgress, Placement::Inferred);
    }
    (Column::Backlog, Placement::Undecided)
}

/// The new text of the issue moved to `column`: every status label replaced by that column's,
/// written the way the project already writes it, and the state opened or closed to match.
pub fn moved(issue: &Issue, column: Column, existing: &[String]) -> Result<String, EditError> {
    let mut labels: Vec<String> = issue
        .labels
        .iter()
        .filter(|label| !is_status_label(label))
        .cloned()
        .collect();
    if let Some(label) = column.label() {
        let spelled = existing
            .iter()
            .find(|known| normalised(known) == label)
            .or_else(|| {
                existing
                    .iter()
                    .find(|known| column_named(known) == Some(column))
            })
            .map_or(label, String::as_str);
        labels.push(spelled.to_string());
    }
    let state = match column {
        Column::Done if issue.is_open => Some(STATE_DONE),
        Column::Done => None,
        _ if !issue.is_open || column_named(&issue.state).is_some() => Some(STATE_OPEN),
        _ => None,
    };
    let mut text = issue.file.text.clone();
    if labels != issue.labels {
        text = set_front(&issue.file, "labels", &render_list(&labels))?;
    }
    if let Some(state) = state {
        let parsed = parse(&text);
        let style = style_of(parsed.front.raw(&text), "state");
        text = set_front(&parsed, "state", &render_scalar(state, style))?;
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::FileText;
    use crate::issues::build;
    use std::path::Path;

    fn issue(front: &str) -> Issue {
        let content = FileText::new(&format!("---\n{front}---\n\n# #7 Thing\n\nBody\n"));
        build(
            Path::new("/issues/007-thing.md"),
            7,
            content,
            Timestamp::UNIX_EPOCH,
        )
    }

    fn column(front: &str) -> Option<Column> {
        stated_column(&issue(front))
    }

    const NOW: i64 = 1_800_000_000;

    fn signal(checked_out: bool, ahead: u32, days_ago: i64) -> WorkSignal {
        WorkSignal {
            checked_out,
            ahead,
            committed: Timestamp::from_second(NOW - days_ago * 24 * 60 * 60).unwrap(),
        }
    }

    #[test]
    fn status_words_are_compared_without_regard_to_case_spaces_or_underscores() {
        for word in [
            "in-progress",
            "In Progress",
            "in_progress",
            "IN--PROGRESS",
            "wip",
        ] {
            assert_eq!(column_named(word), Some(Column::InProgress), "{word}");
        }
        assert_eq!(column_named("Needs Review"), Some(Column::Review));
        assert_eq!(column_named("approved"), Some(Column::Selected));
        assert_eq!(column_named("bug"), None);
        assert_eq!(column_named("open"), None);
    }

    #[test]
    fn a_closed_state_is_done_whatever_the_labels_say() {
        assert_eq!(
            column("labels: [blocked]\nstate: closed\n"),
            Some(Column::Done)
        );
        assert_eq!(column("state: wontfix\n"), Some(Column::Done));
        assert_eq!(column("labels: [done]\n"), Some(Column::Done));
    }

    #[test]
    fn labels_and_a_custom_state_both_count_and_the_column_further_right_wins() {
        assert_eq!(column("labels: [bug, testing]\n"), Some(Column::Testing));
        assert_eq!(column("state: in-review\n"), Some(Column::Review));
        assert_eq!(
            column("labels: [backlog, blocked, review]\n"),
            Some(Column::Blocked)
        );
        assert_eq!(
            column("labels: [in-progress]\nstate: in-review\n"),
            Some(Column::Review)
        );
        assert_eq!(column("labels: [bug]\nstate: open\n"), None);
    }

    #[test]
    fn an_issue_without_a_status_is_in_progress_only_while_a_branch_is_being_worked_on() {
        let now = Timestamp::from_second(NOW).unwrap();
        let plain = issue("labels: [bug]\n");
        let undecided = (Column::Backlog, Placement::Undecided);
        let inferred = (Column::InProgress, Placement::Inferred);
        assert_eq!(place(&plain, &[], now), undecided);
        assert_eq!(place(&plain, &[signal(true, 0, 400)], now), inferred);
        assert_eq!(place(&plain, &[signal(false, 2, 3)], now), inferred);
        assert_eq!(place(&plain, &[signal(false, 2, 30)], now), undecided);
        assert_eq!(place(&plain, &[signal(false, 0, 1)], now), undecided);
        let stated = issue("labels: [review]\n");
        assert_eq!(
            place(&stated, &[signal(true, 1, 0)], now),
            (Column::Review, Placement::Stated)
        );
    }

    #[test]
    fn moving_replaces_the_status_labels_and_keeps_the_others() {
        let issue = issue("title: Thing\nlabels: [bug, backlog, Later]\nstate: open\n");
        let text = moved(&issue, Column::InProgress, &[]).unwrap();
        assert!(text.contains("labels: [bug, in-progress]\nstate: open\n"));
        assert!(text.ends_with("# #7 Thing\n\nBody\n"));
    }

    #[test]
    fn moving_writes_the_status_label_the_way_the_project_spells_it() {
        let issue = issue("labels: [bug]\n");
        let existing = ["In Progress".to_string(), "bug".to_string()];
        let text = moved(&issue, Column::InProgress, &existing).unwrap();
        assert!(text.contains("labels: [bug, In Progress]\n"));
        let existing = ["WIP".to_string()];
        let text = moved(&issue, Column::InProgress, &existing).unwrap();
        assert!(text.contains("labels: [bug, WIP]\n"));
    }

    #[test]
    fn moving_to_done_closes_the_issue_and_moving_out_reopens_it() {
        let open = issue("labels: [review]\nstate: open\n");
        let text = moved(&open, Column::Done, &[]).unwrap();
        assert!(text.contains("labels: []\nstate: done\n"));
        let closed = issue("labels: [bug]\nstate: 'closed'\n");
        let text = moved(&closed, Column::Blocked, &[]).unwrap();
        assert!(text.contains("labels: [bug, blocked]\nstate: 'open'\n"));
    }

    #[test]
    fn a_status_kept_in_the_state_moves_into_the_labels() {
        let custom = issue("state: in-review\n");
        let text = moved(&custom, Column::Testing, &[]).unwrap();
        assert!(text.contains("state: open\nlabels: [testing]\n"));
    }

    #[test]
    fn moving_an_issue_that_is_already_closed_to_done_leaves_its_state() {
        let dropped = issue("labels: [bug]\nstate: wontfix\n");
        let text = moved(&dropped, Column::Done, &[]).unwrap();
        assert_eq!(text, dropped.file.text);
    }

    #[test]
    fn a_file_without_front_matter_gets_one() {
        let content = FileText::new("# #7 Thing\n");
        let plain = build(
            Path::new("/issues/007-thing.md"),
            7,
            content,
            Timestamp::UNIX_EPOCH,
        );
        let text = moved(&plain, Column::Selected, &[]).unwrap();
        assert_eq!(text, "---\nlabels: [selected]\n---\n\n# #7 Thing\n");
    }
}
