//! Keeping the index table in the issues directory's README.md in step with the issues.
//!
//! The viewer rewrites only the row of the issue it changed and appends a row for an issue it
//! created. Everything else stays as it was: hand-shortened titles, the other rows, the text around
//! the table. A README without an index table is left alone.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use crate::files;

pub const README: &str = "README.md";
pub const EMPTY: &str = "–";
static LINK: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\[(.*)\]\(([^)]*)\)$").unwrap());
const DEPENDS_COLUMNS: [&str; 3] = ["depends on", "depends", "blocked by"];

/// The cells of a table row, split on every `|` that is not escaped.
fn cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = line.as_bytes();
    for (position, byte) in bytes.iter().enumerate() {
        if *byte == b'|' && (position == 0 || bytes[position - 1] != b'\\') {
            parts.push(&line[start..position]);
            start = position + 1;
        }
    }
    parts.push(&line[start..]);
    if parts.first().is_some_and(|p| p.trim().is_empty()) {
        parts.remove(0);
    }
    if parts.last().is_some_and(|p| p.trim().is_empty()) {
        parts.pop();
    }
    parts
        .into_iter()
        .map(|part| part.trim().to_string())
        .collect()
}

fn is_row(line: &str) -> bool {
    line.trim_start().starts_with('|')
}

fn is_separator_cell(cell: &str) -> bool {
    let inner = cell.strip_prefix(':').unwrap_or(cell);
    let inner = inner.strip_suffix(':').unwrap_or(inner);
    !inner.is_empty() && inner.bytes().all(|b| b == b'-')
}

/// (column names, first row, end) of the first table whose first column is `#`.
fn find_table(lines: &[&str]) -> Option<(Vec<String>, usize, usize)> {
    for index in 0..lines.len().saturating_sub(1) {
        if !is_row(lines[index]) || !is_row(lines[index + 1]) {
            continue;
        }
        let header = cells(lines[index]);
        let separator = cells(lines[index + 1]);
        if header.first().map(String::as_str) != Some("#")
            || !separator.iter().all(|cell| is_separator_cell(cell))
        {
            continue;
        }
        let mut end = index + 2;
        while end < lines.len() && is_row(lines[end]) {
            end += 1;
        }
        let columns = header
            .iter()
            .map(|name| name.trim().to_lowercase())
            .collect();
        return Some((columns, index + 2, end));
    }
    None
}

fn row_number(cells: &[String]) -> Option<u64> {
    let first = cells.first()?;
    let start = first.find(|c: char| c.is_ascii_digit())?;
    let digits: String = first[start..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn escape(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('|', "\\|")
        .replace('[', "\\[")
        .replace(']', "\\]")
}

fn refs(numbers: &[u64]) -> String {
    if numbers.is_empty() {
        return EMPTY.into();
    }
    numbers
        .iter()
        .map(|n| format!("#{n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn join_labels(labels: &[String]) -> String {
    if labels.is_empty() {
        EMPTY.into()
    } else {
        labels.join(", ")
    }
}

fn join_row(cells: &[String]) -> String {
    format!("| {} |", cells.join(" | "))
}

fn column(columns: &[String], names: &[&str]) -> Option<usize> {
    columns
        .iter()
        .position(|column| names.contains(&column.as_str()))
}

/// What changed in an issue, for its row.
#[derive(Debug, Clone, Default)]
pub struct RowChange {
    pub title: Option<String>,
    pub filename: Option<String>,
    pub labels: Option<Vec<String>>,
    pub state: Option<String>,
    pub depends_on: Option<Vec<u64>>,
}

impl RowChange {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.labels.is_none()
            && self.state.is_none()
            && self.depends_on.is_none()
    }
}

/// The README with the row of `number` updated, or None when there is nothing to change.
pub fn updated_row(readme: &str, number: u64, change: &RowChange) -> Option<String> {
    let mut lines: Vec<String> = readme.split('\n').map(str::to_string).collect();
    let borrowed: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (columns, first, end) = find_table(&borrowed)?;
    for index in first..end {
        let mut row = cells(&lines[index]);
        if row_number(&row) != Some(number) {
            continue;
        }
        if row.len() < columns.len() {
            row.resize(columns.len(), String::new());
        }
        if let (Some(title), Some(position)) = (&change.title, column(&columns, &["title"])) {
            let target = LINK
                .captures(&row[position])
                .map(|link| link[2].to_string())
                .or_else(|| change.filename.clone());
            row[position] = match target {
                Some(target) if !target.is_empty() => format!("[{}]({target})", escape(title)),
                _ => escape(title),
            };
        }
        if let (Some(labels), Some(position)) = (&change.labels, column(&columns, &["labels"])) {
            row[position] = join_labels(labels);
        }
        if let (Some(state), Some(position)) = (&change.state, column(&columns, &["state"])) {
            row[position] = state.clone();
        }
        if let (Some(depends_on), Some(position)) =
            (&change.depends_on, column(&columns, &DEPENDS_COLUMNS))
        {
            row[position] = refs(depends_on);
        }
        let joined = join_row(&row);
        if joined == lines[index] {
            return None;
        }
        lines[index] = joined;
        return Some(lines.join("\n"));
    }
    None
}

/// The README with a row for a new issue at the end of the index table.
pub fn appended_row(
    readme: &str,
    number: u64,
    title: &str,
    filename: &str,
    labels: &[String],
    state: &str,
    depends_on: &[u64],
) -> Option<String> {
    let mut lines: Vec<&str> = readme.split('\n').collect();
    let (columns, first, end) = find_table(&lines)?;
    if (first..end).any(|index| row_number(&cells(lines[index])) == Some(number)) {
        let change = RowChange {
            title: Some(title.to_string()),
            filename: Some(filename.to_string()),
            labels: Some(labels.to_vec()),
            state: Some(state.to_string()),
            depends_on: Some(depends_on.to_vec()),
        };
        return updated_row(readme, number, &change);
    }
    let row: Vec<String> = columns
        .iter()
        .map(|column| match column.as_str() {
            "#" => number.to_string(),
            "title" => format!("[{}]({filename})", escape(title)),
            "labels" => join_labels(labels),
            "state" => state.to_string(),
            other if DEPENDS_COLUMNS.contains(&other) => refs(depends_on),
            _ => EMPTY.to_string(),
        })
        .collect();
    let joined = join_row(&row);
    lines.insert(end, &joined);
    Some(lines.join("\n"))
}

/// Apply `change(readme text) -> new text` to the README, if there is one.
///
/// The README belongs to the person keeping the notes, so a failure here is logged and never
/// undoes the change to the issue itself.
pub fn rewrite(directory: &Path, change: impl Fn(&str) -> Option<String>) {
    let path = directory.join(README);
    let result = (|| -> std::io::Result<()> {
        for _ in 0..3 {
            if !path.is_file() {
                return Ok(());
            }
            let current = files::read(&path)?;
            if !current.readable {
                return Ok(());
            }
            let Some(new) = change(&current.text).filter(|new| *new != current.text) else {
                return Ok(());
            };
            if files::read(&path)?.version != current.version {
                continue; // someone else wrote it meanwhile; start again from their version
            }
            return files::write_atomic(&path, &new, current.newline);
        }
        Ok(())
    })();
    if let Err(error) = result {
        tracing::warn!("could not update {}: {error}", path.display());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const README_TEXT: &str = "\
# Issues

Intro text stays.

| # | Title | Labels | State | Depends on |
|---|---|---|---|---|
| 1 | [Skeleton](001-skeleton.md) | enhancement, infra | done | – |
| 2 | [Short title by hand](002-storage.md) | data | open | #1 |

Order of work: 1 → 2.
";

    fn state(value: &str) -> RowChange {
        RowChange {
            state: Some(value.into()),
            ..RowChange::default()
        }
    }

    #[test]
    fn updating_a_row_changes_only_the_given_cells_of_that_row() {
        let result = updated_row(README_TEXT, 2, &state("done")).unwrap();
        assert_eq!(
            result,
            README_TEXT.replace(
                "| 2 | [Short title by hand](002-storage.md) | data | open | #1 |",
                "| 2 | [Short title by hand](002-storage.md) | data | done | #1 |",
            )
        );
    }

    #[test]
    fn updating_the_title_keeps_the_link_target() {
        let change = RowChange {
            title: Some("Store | notes [fast]".into()),
            filename: Some("ignored.md".into()),
            ..RowChange::default()
        };
        let result = updated_row(README_TEXT, 2, &change).unwrap();
        assert!(
            result.contains(
                "| 2 | [Store \\| notes \\[fast\\]](002-storage.md) | data | open | #1 |"
            )
        );
    }

    #[test]
    fn labels_and_dependencies_are_written_like_the_rest_of_the_table() {
        let change = RowChange {
            labels: Some(vec![]),
            depends_on: Some(vec![2, 3]),
            ..RowChange::default()
        };
        let result = updated_row(README_TEXT, 1, &change).unwrap();
        assert!(result.contains("| 1 | [Skeleton](001-skeleton.md) | – | done | #2, #3 |"));
    }

    #[test]
    fn nothing_to_change_gives_none() {
        assert_eq!(updated_row(README_TEXT, 2, &state("open")), None);
        assert_eq!(updated_row(README_TEXT, 9, &state("done")), None);
    }

    #[test]
    fn a_new_issue_is_appended_after_the_last_row() {
        let result = appended_row(
            README_TEXT,
            3,
            "Search",
            "003-search.md",
            &["ui".into()],
            "open",
            &[2],
        )
        .unwrap();
        assert_eq!(
            result,
            README_TEXT.replace(
                "| 2 | [Short title by hand](002-storage.md) | data | open | #1 |\n",
                "| 2 | [Short title by hand](002-storage.md) | data | open | #1 |\n\
                 | 3 | [Search](003-search.md) | ui | open | #2 |\n",
            )
        );
    }

    #[test]
    fn appending_an_issue_that_already_has_a_row_updates_it_instead() {
        let result = appended_row(
            README_TEXT,
            2,
            "Storage",
            "002-storage.md",
            &["data".into()],
            "done",
            &[1],
        )
        .unwrap();
        assert_eq!(result.matches("| 2 |").count(), 1);
        assert!(result.contains("| 2 | [Storage](002-storage.md) | data | done | #1 |"));
    }

    #[test]
    fn a_readme_without_an_index_table_is_left_alone() {
        let text = "# Notes\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(updated_row(text, 1, &state("done")), None);
        assert_eq!(
            appended_row(text, 3, "x", "003-x.md", &[], "open", &[]),
            None
        );
    }

    #[test]
    fn rewrite_writes_the_change_and_ignores_a_missing_readme() {
        let directory = tempfile::tempdir().unwrap();
        rewrite(directory.path(), |text| Some(format!("{text}never")));
        assert!(!directory.path().join(README).exists());
        std::fs::write(directory.path().join(README), README_TEXT).unwrap();
        rewrite(directory.path(), |text| {
            updated_row(text, 2, &state("done"))
        });
        let written = std::fs::read_to_string(directory.path().join(README)).unwrap();
        assert!(written.contains("| data | done | #1 |"));
    }
}
