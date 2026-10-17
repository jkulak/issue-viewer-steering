//! The search box: the parts of GitHub's issue query syntax that make sense for files.
//!
//! `is:open`, `is:closed`, `state:<raw state>`, `reason:completed|not-planned`, `label:a`
//! (repeated for AND, `label:a,b` for OR), `-label:a`, `no:label`, `sort:<key>`, and free text that
//! has to appear in the title or the file. Quotes keep a value with spaces together:
//! `label:"good first"`.

use crate::issues::{Issue, Reason};
use crate::text::casefold;

pub const DEFAULT: &str = "is:issue is:open";
pub const SORTS: [(&str, &str); 6] = [
    ("created-desc", "Newest"),
    ("created-asc", "Oldest"),
    ("comments-desc", "Most commented"),
    ("comments-asc", "Least commented"),
    ("updated-desc", "Recently updated"),
    ("updated-asc", "Least recently updated"),
];
pub const DEFAULT_SORT: &str = "created-desc";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Query {
    /// `Some(true)` for open issues only, `Some(false)` for closed ones, `None` for both.
    pub open: Option<bool>,
    pub raw_states: Vec<String>,
    pub reason: Option<Reason>,
    /// Every group must match one of its names.
    pub labels: Vec<Vec<String>>,
    pub excluded_labels: Vec<String>,
    pub no_label: bool,
    pub words: Vec<String>,
    pub sort: &'static str,
}

impl Default for Query {
    fn default() -> Self {
        Query {
            open: None,
            raw_states: Vec::new(),
            reason: None,
            labels: Vec::new(),
            excluded_labels: Vec::new(),
            no_label: false,
            words: Vec::new(),
            sort: DEFAULT_SORT,
        }
    }
}

/// Split a query into words the way a POSIX shell would: quotes group, a backslash escapes. An
/// unbalanced quote falls back to splitting on whitespace, so a half-typed query still searches.
pub fn tokenize(text: &str) -> Vec<String> {
    shell_split(text).unwrap_or_else(|| text.split_whitespace().map(str::to_string).collect())
}

fn shell_split(text: &str) -> Option<Vec<String>> {
    let mut tokens = Vec::new();
    let mut token = String::new();
    let mut in_token = false;
    let mut chars = text.chars();
    while let Some(char) = chars.next() {
        match char {
            ' ' | '\t' | '\r' | '\n' => {
                if in_token {
                    tokens.push(std::mem::take(&mut token));
                    in_token = false;
                }
            }
            '\\' => {
                token.push(chars.next()?);
                in_token = true;
            }
            '\'' => {
                in_token = true;
                loop {
                    match chars.next()? {
                        '\'' => break,
                        other => token.push(other),
                    }
                }
            }
            '"' => {
                in_token = true;
                loop {
                    match chars.next()? {
                        '"' => break,
                        '\\' => match chars.next()? {
                            escaped @ ('"' | '\\') => token.push(escaped),
                            other => {
                                token.push('\\');
                                token.push(other);
                            }
                        },
                        other => token.push(other),
                    }
                }
            }
            other => {
                token.push(other);
                in_token = true;
            }
        }
    }
    if in_token {
        tokens.push(token);
    }
    Some(tokens)
}

fn sort_key(value: &str) -> Option<&'static str> {
    SORTS.iter().map(|(key, _)| *key).find(|key| *key == value)
}

pub fn parse(text: &str) -> Query {
    let mut query = Query::default();
    for token in tokenize(text) {
        let negated = token.starts_with('-') && token.contains(':');
        let bare = if negated { &token[1..] } else { token.as_str() };
        if let Some((key, value)) = bare.split_once(':')
            && !value.is_empty()
        {
            let (key, lowered) = (key.to_lowercase(), value.to_lowercase());
            let lowered = lowered.as_str();
            match (key.as_str(), lowered) {
                ("is", "open" | "closed") => {
                    query.open = Some((lowered == "open") != negated);
                    continue;
                }
                ("is", "issue" | "issues") => continue,
                ("state", "open" | "closed") => {
                    query.open = Some(lowered == "open");
                    continue;
                }
                ("state", _) => {
                    query.raw_states.push(lowered.to_string());
                    continue;
                }
                ("reason", "completed") => {
                    query.reason = Some(Reason::Completed);
                    continue;
                }
                ("reason", "not-planned" | "not_planned") => {
                    query.reason = Some(Reason::NotPlanned);
                    continue;
                }
                ("label", _) => {
                    let names: Vec<String> = value
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_string)
                        .collect();
                    if negated {
                        query.excluded_labels.extend(names);
                    } else {
                        query.labels.push(names);
                    }
                    continue;
                }
                ("no", "label") => {
                    query.no_label = true;
                    continue;
                }
                ("sort", _) => {
                    if let Some(key) = sort_key(lowered) {
                        query.sort = key;
                        continue;
                    }
                }
                _ => {}
            }
        }
        query.words.push(casefold(&token));
    }
    query
}

/// What a query looks at: an issue file, or an issue or pull request on GitHub.
pub trait Searchable {
    fn is_open(&self) -> bool;
    /// The state as written, lower case.
    fn raw_state(&self) -> &str;
    fn reason(&self) -> Option<Reason>;
    fn labels(&self) -> &[String];
    /// The text free words are looked for in, casefolded.
    fn search_text(&self) -> &str;
}

impl Searchable for Issue {
    fn is_open(&self) -> bool {
        self.is_open
    }
    fn raw_state(&self) -> &str {
        &self.state
    }
    fn reason(&self) -> Option<Reason> {
        self.reason
    }
    fn labels(&self) -> &[String] {
        &self.labels
    }
    fn search_text(&self) -> &str {
        Issue::search_text(self)
    }
}

impl<T: Searchable + ?Sized> Searchable for &T {
    fn is_open(&self) -> bool {
        (**self).is_open()
    }
    fn raw_state(&self) -> &str {
        (**self).raw_state()
    }
    fn reason(&self) -> Option<Reason> {
        (**self).reason()
    }
    fn labels(&self) -> &[String] {
        (**self).labels()
    }
    fn search_text(&self) -> &str {
        (**self).search_text()
    }
}

impl<T: Searchable + ?Sized> Searchable for std::sync::Arc<T> {
    fn is_open(&self) -> bool {
        (**self).is_open()
    }
    fn raw_state(&self) -> &str {
        (**self).raw_state()
    }
    fn reason(&self) -> Option<Reason> {
        (**self).reason()
    }
    fn labels(&self) -> &[String] {
        (**self).labels()
    }
    fn search_text(&self) -> &str {
        (**self).search_text()
    }
}

pub fn matches(query: &Query, item: &impl Searchable, ignore_state: bool) -> bool {
    if !ignore_state && !state_matches(query, item) {
        return false;
    }
    if !query.raw_states.is_empty() && !query.raw_states.iter().any(|s| s == item.raw_state()) {
        return false;
    }
    if query.reason.is_some() && item.reason() != query.reason {
        return false;
    }
    let names: Vec<String> = item.labels().iter().map(|label| casefold(label)).collect();
    let has = |name: &String| names.contains(&casefold(name));
    if query.labels.iter().any(|group| !group.iter().any(has)) {
        return false;
    }
    if query.excluded_labels.iter().any(has) {
        return false;
    }
    if query.no_label && !item.labels().is_empty() {
        return false;
    }
    query
        .words
        .iter()
        .all(|word| item.search_text().contains(word.as_str()))
}

pub fn state_matches(query: &Query, item: &impl Searchable) -> bool {
    query.open.is_none_or(|open| open == item.is_open())
}

pub fn ordered<T: AsRef<Issue>>(issues: &mut [T], key: &str) {
    let issue = |item: &T| -> (u64, jiff::Timestamp, usize) {
        let issue = item.as_ref();
        (issue.number, issue.updated, issue.comments().len())
    };
    match key {
        "created-asc" => issues.sort_by_key(|i| issue(i).0),
        "updated-desc" => issues.sort_by_key(|i| std::cmp::Reverse((issue(i).1, issue(i).0))),
        "updated-asc" => issues.sort_by_key(|i| (issue(i).1, issue(i).0)),
        "comments-desc" => {
            issues.sort_by_key(|i| (std::cmp::Reverse(issue(i).2), std::cmp::Reverse(issue(i).0)))
        }
        "comments-asc" => issues.sort_by_key(|i| (issue(i).2, std::cmp::Reverse(issue(i).0))),
        _ => issues.sort_by_key(|i| std::cmp::Reverse(issue(i).0)),
    }
}

// Editing a query string for the links on the list page.

/// A token written so that `tokenize` reads it back whole: quoted when it has spaces, quotes or
/// backslashes, with only the value quoted after a `key:`.
fn quote(token: &str) -> String {
    let needs_quotes = |text: &str| {
        text.chars()
            .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'))
    };
    if !needs_quotes(token) {
        return token.to_string();
    }
    let escape = |text: &str| text.replace('\\', "\\\\").replace('"', "\\\"");
    match token.split_once(':') {
        Some((key, value)) if !key.is_empty() && !needs_quotes(key) => {
            format!("{key}:\"{}\"", escape(value))
        }
        _ => format!("\"{}\"", escape(token)),
    }
}

fn join(tokens: &[String]) -> String {
    tokens
        .iter()
        .map(|token| quote(token))
        .collect::<Vec<_>>()
        .join(" ")
}

fn is_state(token: &str) -> bool {
    matches!(
        token.to_lowercase().trim_start_matches('-'),
        "is:open" | "is:closed" | "state:open" | "state:closed"
    )
}

pub fn with_state(text: &str, open: bool) -> String {
    let mut tokens: Vec<String> = tokenize(text)
        .into_iter()
        .filter(|t| !is_state(t))
        .collect();
    let position = tokens
        .iter()
        .position(|t| t.to_lowercase() == "is:issue")
        .map_or(0, |i| i + 1);
    tokens.insert(
        position,
        if open { "is:open" } else { "is:closed" }.to_string(),
    );
    join(&tokens)
}

pub fn label_token(label: &str) -> String {
    quote(&format!("label:{label}"))
}

/// Add `label:<label>`, or take it out when it is already there.
pub fn with_label(text: &str, label: &str) -> String {
    let tokens = tokenize(text);
    let wanted = casefold(&format!("label:{label}"));
    let mut kept: Vec<String> = tokens
        .iter()
        .filter(|t| casefold(t) != wanted)
        .cloned()
        .collect();
    if kept.len() == tokens.len() {
        kept.retain(|t| t.to_lowercase() != "no:label");
        kept.push(format!("label:{label}"));
    }
    join(&kept)
}

pub fn with_no_label(text: &str) -> String {
    let mut tokens: Vec<String> = tokenize(text)
        .into_iter()
        .filter(|t| !t.to_lowercase().starts_with("label:"))
        .collect();
    if !tokens.iter().any(|t| t.to_lowercase() == "no:label") {
        tokens.push("no:label".into());
    }
    join(&tokens)
}

pub fn with_sort(text: &str, key: &str) -> String {
    let mut tokens: Vec<String> = tokenize(text)
        .into_iter()
        .filter(|t| !t.to_lowercase().starts_with("sort:"))
        .collect();
    if key != DEFAULT_SORT {
        tokens.push(format!("sort:{key}"));
    }
    join(&tokens)
}

pub fn has_label(query: &Query, label: &str) -> bool {
    let wanted = casefold(label);
    query
        .labels
        .iter()
        .any(|group| group.len() == 1 && casefold(&group[0]) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::FileText;
    use crate::issues::build;
    use jiff::Timestamp;
    use std::path::Path;

    fn make(
        number: u64,
        title: &str,
        labels: &str,
        state: &str,
        updated: i64,
        body: &str,
        comments: usize,
    ) -> Issue {
        let comment_text: String = (0..comments)
            .map(|i| format!("\n### 2026-09-2{i} 10:00\n\nc{i}\n"))
            .collect();
        let section = if comments > 0 {
            format!("\n## Comments\n{comment_text}")
        } else {
            String::new()
        };
        let text = format!(
            "---\ntitle: \"{title}\"\nlabels: {labels}\nstate: {state}\n---\n\n{body}\n{section}"
        );
        let content = FileText {
            text,
            newline: "\n",
            version: "v".into(),
            readable: true,
        };
        let path = format!("/x/{number:03}-x.md");
        build(
            Path::new(&path),
            number,
            content,
            Timestamp::from_second(updated).unwrap(),
        )
    }

    fn issues() -> Vec<Issue> {
        vec![
            make(1, "Skeleton", "[infra]", "done", 50, "", 0),
            make(
                2,
                "Search notes",
                "[ui, enhancement]",
                "open",
                10,
                "Matches highlighted",
                2,
            ),
            make(3, "Crash on save", "[bug]", "open", 30, "", 1),
            make(4, "Dark theme", "[ui, good first]", "open", 20, "", 0),
            make(5, "Old idea", "[]", "wontfix", 40, "", 0),
        ]
    }

    fn numbers(text: &str) -> Vec<u64> {
        let parsed = parse(text);
        let all = issues();
        let mut found: Vec<&Issue> = all.iter().filter(|i| matches(&parsed, i, false)).collect();
        ordered(&mut found, parsed.sort);
        found.iter().map(|issue| issue.number).collect()
    }

    #[test]
    fn the_default_query_shows_open_issues_newest_first() {
        assert_eq!(numbers(DEFAULT), [4, 3, 2]);
        assert_eq!(numbers("is:closed"), [5, 1]);
        assert_eq!(numbers(""), [5, 4, 3, 2, 1]);
        assert_eq!(numbers("-is:open"), [5, 1]);
    }

    #[test]
    fn repeated_labels_must_all_match_and_a_comma_means_either() {
        assert_eq!(numbers("label:ui"), [4, 2]);
        assert_eq!(numbers("label:ui label:enhancement"), [2]);
        assert_eq!(numbers("label:bug,infra"), [3, 1]);
        assert_eq!(numbers("label:\"good first\""), [4]);
        assert_eq!(numbers("label:UI"), [4, 2]);
    }

    #[test]
    fn a_label_can_be_excluded_and_no_label_finds_the_bare_ones() {
        assert_eq!(numbers("is:open -label:ui"), [3]);
        assert_eq!(numbers("no:label"), [5]);
    }

    #[test]
    fn free_text_matches_the_title_and_the_body() {
        assert_eq!(numbers("search"), [2]);
        assert_eq!(numbers("highlighted"), [2]);
        assert_eq!(numbers("\"crash on\""), [3]);
        assert_eq!(numbers("#3"), [3]);
    }

    #[test]
    fn raw_states_and_reasons() {
        assert_eq!(numbers("state:wontfix"), [5]);
        assert_eq!(numbers("reason:not-planned"), [5]);
        assert_eq!(numbers("reason:completed"), [1]);
    }

    #[test]
    fn each_sort_qualifier_orders_the_issues_its_own_way() {
        assert_eq!(numbers("sort:created-asc"), [1, 2, 3, 4, 5]);
        assert_eq!(numbers("sort:updated-desc"), [1, 5, 3, 4, 2]);
        assert_eq!(numbers("sort:updated-asc"), [2, 4, 3, 5, 1]);
        assert_eq!(numbers("sort:comments-desc"), [2, 3, 5, 4, 1]);
        assert_eq!(numbers("sort:comments-asc"), [5, 4, 1, 3, 2]);
    }

    #[test]
    fn switching_the_state_replaces_the_state_qualifier() {
        assert_eq!(
            with_state("is:issue is:open label:ui", false),
            "is:issue is:closed label:ui"
        );
        assert_eq!(with_state("label:ui", true), "is:open label:ui");
    }

    #[test]
    fn adding_a_label_twice_takes_it_out_again_and_quotes_spaces() {
        let once = with_label("is:open", "good first");
        assert_eq!(once, "is:open label:\"good first\"");
        assert_eq!(with_label(&once, "good first"), "is:open");
        assert_eq!(with_label("is:open no:label", "bug"), "is:open label:bug");
        assert_eq!(
            with_no_label("is:open label:\"good first\""),
            "is:open no:label"
        );
    }

    #[test]
    fn a_label_with_a_quote_survives_a_round_trip() {
        let query = with_label("is:open", "won't fix");
        assert_eq!(parse(&query).labels, [vec!["won't fix".to_string()]]);
        let query = with_label("is:open", "say \"hi\"");
        assert_eq!(parse(&query).labels, [vec!["say \"hi\"".to_string()]]);
    }

    #[test]
    fn the_default_sort_is_left_out_of_the_query() {
        assert_eq!(
            with_sort("is:open", "updated-desc"),
            "is:open sort:updated-desc"
        );
        assert_eq!(
            with_sort("is:open sort:updated-desc", "created-desc"),
            "is:open"
        );
    }

    #[test]
    fn an_unbalanced_quote_does_not_break_the_search() {
        assert_eq!(parse("label:\"oops").labels, [vec!["\"oops".to_string()]]);
    }

    #[test]
    fn a_hash_starts_a_word_rather_than_a_comment() {
        assert_eq!(
            tokenize("#12 a\\ b '' \"x\\y\""),
            ["#12", "a b", "", "x\\y"]
        );
    }
}
