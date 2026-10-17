//! The parts of an issue file, and the edits the viewer makes to them.
//!
//! An issue file is front matter, an optional `# #N Title` heading, the description, and an
//! optional `## Comments` section at the end with one `### YYYY-MM-DD HH:MM` heading per comment.
//! Parsing records where each part starts and ends; an edit replaces one of those spans and
//! nothing else, so whatever the viewer was not asked to change stays byte for byte.

use std::fmt;
use std::sync::LazyLock;

use jiff::civil::DateTime;
use regex::Regex;

use crate::frontmatter::{self, FrontMatter, Style};
use crate::text::{casefold, collapse, is_blank, yaml_text};

static HEADING_1: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^# +(.+?)(?:[ \t]+#+)?[ \t]*$").unwrap());
static NUMBERED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^#([0-9]+)\b[ \t]*(.*)$").unwrap());
static COMMENTS_HEADING: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^## +Comments[ \t]*$").unwrap());
pub static COMMENT_HEADING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^### +([0-9]{4}-[0-9]{2}-[0-9]{2}[ T][0-9]{2}:[0-9]{2}(?::[0-9]{2})?)(?:[ \t]+(.*?))?[ \t]*$")
        .unwrap()
});
static TASK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^((?:[ \t]*>)*[ \t]*(?:[-*+]|[0-9]{1,9}[.)])[ \t]+)\[([ xX])\]").unwrap()
});
static LEADING_BLANK_LINES: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\A(?:[ \t]*\n)+").unwrap());
pub const STAMP_FORMAT: &str = "%Y-%m-%d %H:%M";

/// The edit cannot be made to the file as it is now.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditError(pub String);

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for EditError {}

fn refuse<T>(message: impl Into<String>) -> Result<T, EditError> {
    Err(EditError(message.into()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn of<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Line {
    pub start: usize,
    /// Position of the newline, or the end of the text.
    pub end: usize,
    /// Inside a fenced code block, the fence lines included.
    pub fenced: bool,
}

impl Line {
    pub fn text<'a>(&self, text: &'a str) -> &'a str {
        &text[self.start..self.end]
    }
}

/// `(fence character, fence length, rest of the line)` for a line that is a code fence.
fn fence_of(line: &str) -> Option<(u8, usize, &str)> {
    let indent = line.bytes().take_while(|b| *b == b' ').count();
    if indent > 3 {
        return None;
    }
    let rest = &line[indent..];
    let marker = *rest.as_bytes().first()?;
    if marker != b'`' && marker != b'~' {
        return None;
    }
    let length = rest.bytes().take_while(|b| *b == marker).count();
    (length >= 3).then(|| (marker, length, &rest[length..]))
}

pub fn lines(text: &str, start: usize) -> Vec<Line> {
    let mut found = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    let mut position = start;
    while position < text.len() {
        let stop = text[position..]
            .find('\n')
            .map_or(text.len(), |offset| position + offset);
        let line = &text[position..stop];
        let matched = fence_of(line);
        match fence {
            None => {
                let opens =
                    matched.filter(|(marker, _, rest)| !(*marker == b'`' && rest.contains('`')));
                found.push(Line {
                    start: position,
                    end: stop,
                    fenced: opens.is_some(),
                });
                fence = opens.map(|(marker, length, _)| (marker, length));
            }
            Some((open_marker, open_length)) => {
                found.push(Line {
                    start: position,
                    end: stop,
                    fenced: true,
                });
                if let Some((marker, length, rest)) = matched
                    && marker == open_marker
                    && length >= open_length
                    && is_blank(rest)
                {
                    fence = None;
                }
            }
        }
        position = stop + 1;
    }
    found
}

fn line_span(text: &str, line: &Line) -> Span {
    Span {
        start: line.start,
        end: (line.end + 1).min(text.len()),
    }
}

/// The span without its leading blank lines and trailing whitespace.
fn content(text: &str, span: Span) -> Span {
    let (mut start, mut end) = (span.start, span.end);
    while start < end {
        let newline = text[start..end].find('\n').map(|offset| start + offset);
        let line_end = newline.unwrap_or(end);
        if !is_blank(&text[start..line_end]) {
            break;
        }
        start = newline.map_or(end, |n| n + 1);
    }
    while end > start && matches!(text.as_bytes()[end - 1], b' ' | b'\t' | b'\n') {
        end -= 1;
    }
    Span { start, end }
}

#[derive(Debug, Clone)]
pub struct Comment {
    pub index: usize,
    /// The timestamp as written in the heading.
    pub stamp: String,
    /// Whatever follows the timestamp on the heading line.
    pub note: String,
    /// The heading line, newline included.
    pub heading: Span,
    /// Everything after the heading, up to the next comment.
    pub body: Span,
    /// The body without surrounding blank lines.
    pub content: Span,
}

impl Comment {
    /// The time in the heading, read without a zone.
    pub fn when(&self) -> Option<DateTime> {
        let stamp = self.stamp.as_bytes();
        let number = |range: std::ops::Range<usize>| self.stamp.get(range)?.parse::<i32>().ok();
        let second = if stamp.len() > 16 { number(17..19)? } else { 0 };
        DateTime::new(
            number(0..4)? as i16,
            number(5..7)? as i8,
            number(8..10)? as i8,
            number(11..13)? as i8,
            number(14..16)? as i8,
            second as i8,
            0,
        )
        .ok()
    }
}

#[derive(Debug, Clone)]
pub struct IssueFile {
    pub text: String,
    pub front: FrontMatter,
    /// The `# #N Title` line, newline included.
    pub heading: Option<Span>,
    /// N when the heading starts with #N.
    pub heading_number: Option<u64>,
    pub heading_title: Option<String>,
    /// Between the heading (or the front matter) and the comments.
    pub description: Span,
    pub description_content: Span,
    pub comments_heading: Option<Span>,
    pub comments: Vec<Comment>,
}

impl IssueFile {
    pub fn description_text(&self) -> &str {
        self.description_content.of(&self.text)
    }

    pub fn comment_text(&self, comment: &Comment) -> &str {
        comment.content.of(&self.text)
    }
}

fn same(left: &str, right: &str) -> bool {
    casefold(&collapse(left)) == casefold(&collapse(right))
}

pub fn parse(text: &str) -> IssueFile {
    let front = frontmatter::split(text);
    let title = front.get("title").map(yaml_text);
    let body_lines = lines(text, front.body_start);

    let (mut heading, mut heading_number, mut heading_title) = (None, None, None);
    if let Some(line) = body_lines.iter().find(|line| !is_blank(line.text(text)))
        && !line.fenced
        && let Some(captures) = HEADING_1.captures(line.text(text))
    {
        let inner = captures[1].trim();
        let numbered = NUMBERED.captures(inner);
        let number = numbered.as_ref().and_then(|c| c[1].parse::<u64>().ok());
        // Without a title in the front matter, the first heading is the only title there is.
        if number.is_some() || title.as_ref().is_none_or(|title| same(inner, title)) {
            heading = Some(line_span(text, line));
            heading_number = number;
            heading_title = Some(match (&numbered, number) {
                (Some(numbered), Some(_)) => numbered[2].trim().to_string(),
                _ => inner.to_string(),
            });
        }
    }

    let description_start = heading.map_or(front.body_start, |span: Span| span.end);
    let rest: Vec<&Line> = body_lines
        .iter()
        .filter(|line| line.start >= description_start)
        .collect();
    let mut comments_heading = None;
    for (index, line) in rest.iter().enumerate() {
        if line.fenced || !COMMENTS_HEADING.is_match(line.text(text)) {
            continue;
        }
        let following = rest[index + 1..]
            .iter()
            .find(|later| !is_blank(later.text(text)));
        if following.is_none_or(|f| !f.fenced && COMMENT_HEADING.is_match(f.text(text))) {
            comments_heading = Some(line_span(text, line));
            break;
        }
    }

    let description = Span {
        start: description_start,
        end: comments_heading.map_or(text.len(), |s| s.start),
    };
    let comments = match comments_heading {
        Some(section_heading) => {
            let section: Vec<&Line> = rest
                .iter()
                .copied()
                .filter(|line| line.start >= section_heading.end)
                .collect();
            comments_of(text, &section)
        }
        None => Vec::new(),
    };
    IssueFile {
        text: text.to_string(),
        front,
        heading,
        heading_number,
        heading_title,
        description,
        description_content: content(text, description),
        comments_heading,
        comments,
    }
}

fn comments_of(text: &str, section: &[&Line]) -> Vec<Comment> {
    let headings: Vec<&Line> = section
        .iter()
        .copied()
        .filter(|line| !line.fenced && COMMENT_HEADING.is_match(line.text(text)))
        .collect();
    headings
        .iter()
        .enumerate()
        .map(|(index, line)| {
            let captures = COMMENT_HEADING.captures(line.text(text)).unwrap();
            let heading = line_span(text, line);
            let end = headings
                .get(index + 1)
                .map_or(text.len(), |next| next.start);
            let body = Span {
                start: heading.end,
                end,
            };
            Comment {
                index,
                stamp: captures[1].to_string(),
                note: captures
                    .get(2)
                    .map_or("", |m| m.as_str())
                    .trim()
                    .to_string(),
                heading,
                body,
                content: content(text, body),
            }
        })
        .collect()
}

/// The checkbox of a task list item at the start of `line`: the byte offset of the character
/// between the brackets, and whether it is ticked.
fn task_of(line: &str) -> Option<(usize, bool)> {
    let captures = TASK.captures(line)?;
    let whole = captures.get(0).unwrap();
    if !line[whole.end()..]
        .chars()
        .next()
        .is_none_or(|c| c == ' ' || c == '\t')
    {
        return None;
    }
    Some((captures.get(1).unwrap().end() + 1, &captures[2] != " "))
}

/// (done, total) task list items outside code blocks.
pub fn task_counts(text: &str) -> (usize, usize) {
    let (mut done, mut total) = (0, 0);
    for line in lines(text, 0) {
        if line.fenced {
            continue;
        }
        if let Some((_, checked)) = task_of(line.text(text)) {
            total += 1;
            done += usize::from(checked);
        }
    }
    (done, total)
}

pub fn normalise_block(text: &str) -> String {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    LEADING_BLANK_LINES
        .replace(&text, "")
        .trim_end()
        .to_string()
}

/// Keep a line that looks like a comment heading from starting a new comment.
pub fn escape_comment_headings(text: &str) -> String {
    lines(text, 0)
        .iter()
        .map(|line| {
            let content = line.text(text);
            if !line.fenced && COMMENT_HEADING.is_match(content) {
                format!("\\{content}")
            } else {
                content.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn replace(text: &str, span: Span, new: &str) -> String {
    let mut out = String::with_capacity(text.len() + new.len());
    out.push_str(&text[..span.start]);
    out.push_str(new);
    out.push_str(&text[span.end..]);
    out
}

// Edits. Each takes the parsed file and returns the new text of the whole file.

pub fn set_front(file: &IssueFile, key: &str, rendered: &str) -> Result<String, EditError> {
    let (text, front) = (&file.text, &file.front);
    if let Some(error) = &front.error {
        return refuse(format!("fix the front matter by hand first: {error}"));
    }
    if !front.present {
        let separator = if text.is_empty() || text.starts_with('\n') {
            ""
        } else {
            "\n"
        };
        return Ok(format!("---\n{key}: {rendered}\n---\n{separator}{text}"));
    }
    let raw = frontmatter::set_key(front.raw(text), key, Some(rendered));
    Ok(replace(
        text,
        Span {
            start: front.raw_start,
            end: front.raw_end,
        },
        &raw,
    ))
}

pub fn set_title(file: &IssueFile, title: &str) -> Result<String, EditError> {
    let mut text = file.text.clone();
    if let Some(heading) = file.heading {
        let line = match file.heading_number {
            Some(number) => format!("# #{number} {title}"),
            None => format!("# {title}"),
        };
        let ending = if heading.of(&text).ends_with('\n') {
            "\n"
        } else {
            ""
        };
        text = replace(&text, heading, &format!("{line}{ending}"));
    }
    let reparsed = parse(&text);
    let style = frontmatter::style_of(reparsed.front.raw(&text), "title");
    set_front(
        &reparsed,
        "title",
        &frontmatter::render_scalar(title, style),
    )
}

pub fn set_description(file: &IssueFile, description: &str) -> Result<String, EditError> {
    let body = normalise_block(description);
    let before = file.description.start > 0;
    let after = file.comments_heading.is_some();
    let mut new = if body.is_empty() {
        if before && after {
            "\n".to_string()
        } else {
            String::new()
        }
    } else {
        format!(
            "{}{body}\n{}",
            if before { "\n" } else { "" },
            if after { "\n" } else { "" }
        )
    };
    let prefix = &file.text[..file.description.start];
    if !prefix.is_empty() && !prefix.ends_with('\n') {
        new.insert(0, '\n');
    }
    let result = replace(&file.text, file.description, &new);
    if parse(&result).description_text() != body {
        return refuse(
            "the description would run into the comments section: a '## Comments' heading \
             followed by a dated '###' heading is how the comments start, so rename one of them",
        );
    }
    Ok(result)
}

pub fn add_comment(file: &IssueFile, stamp: &str, text: &str) -> Result<String, EditError> {
    let body = escape_comment_headings(&normalise_block(text));
    if body.is_empty() {
        return refuse("a comment cannot be empty");
    }
    let mut source = file.text.trim_end_matches('\n').to_string();
    if file.comments_heading.is_none() {
        if !source.is_empty() {
            source.push_str("\n\n");
        }
        source.push_str("## Comments\n");
    } else {
        source.push('\n');
    }
    Ok(format!("{source}\n### {stamp}\n\n{body}\n"))
}

fn comment_at(file: &IssueFile, index: usize) -> Result<&Comment, EditError> {
    file.comments
        .get(index)
        .ok_or_else(|| EditError("that comment is no longer in the file".into()))
}

pub fn edit_comment(file: &IssueFile, index: usize, text: &str) -> Result<String, EditError> {
    let comment = comment_at(file, index)?;
    let body = escape_comment_headings(&normalise_block(text));
    if body.is_empty() {
        return refuse("a comment cannot be empty; delete it instead");
    }
    let last = index == file.comments.len() - 1;
    Ok(replace(
        &file.text,
        comment.body,
        &format!("\n{body}\n{}", if last { "" } else { "\n" }),
    ))
}

pub fn delete_comment(file: &IssueFile, index: usize) -> Result<String, EditError> {
    let comment = comment_at(file, index)?;
    if file.comments.len() == 1 {
        // The last comment takes the section heading and the blank lines before it along.
        let section = file
            .comments_heading
            .expect("comments come with their heading");
        let head = file.text[..section.start].trim_end_matches('\n');
        return Ok(if head.is_empty() {
            String::new()
        } else {
            format!("{head}\n")
        });
    }
    let mut text = replace(
        &file.text,
        Span {
            start: comment.heading.start,
            end: comment.body.end,
        },
        "",
    );
    if index == file.comments.len() - 1 {
        text = format!("{}\n", text.trim_end_matches('\n'));
    }
    Ok(text)
}

/// Tick or clear the checkbox on a line of the description or of one comment.
pub fn set_task(
    file: &IssueFile,
    part: &str,
    line: i64,
    checked: bool,
) -> Result<String, EditError> {
    let content = if part == "description" {
        file.description_content
    } else if !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) {
        let index = part.parse::<usize>().unwrap_or(usize::MAX);
        comment_at(file, index)?.content
    } else {
        return refuse("unknown part of the issue");
    };
    let part_text = content.of(&file.text);
    let part_lines: Vec<&str> = part_text.split('\n').collect();
    let found = usize::try_from(line)
        .ok()
        .and_then(|line| Some((line, task_of(part_lines.get(line)?)?)));
    let Some((line, (offset, ticked))) = found else {
        return refuse("that checkbox is no longer where it was");
    };
    if ticked == checked {
        return Ok(file.text.clone());
    }
    let line_start: usize = part_lines[..line].iter().map(|l| l.len() + 1).sum();
    let position = content.start + line_start + offset;
    let mut text = file.text.clone();
    text.replace_range(position..position + 1, if checked { "x" } else { " " });
    Ok(text)
}

pub fn new_issue_text<S: AsRef<str>>(
    number: u64,
    title: &str,
    labels: &[S],
    description: &str,
) -> String {
    let mut text = format!(
        "---\ntitle: {}\nlabels: {}\nstate: open\n---\n\n# #{number} {title}\n",
        frontmatter::render_scalar(title, Style::Double),
        frontmatter::render_list(labels),
    );
    let body = normalise_block(description);
    if !body.is_empty() {
        text.push_str(&format!("\n{body}\n"));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use yaml_rust2::Yaml;

    const ISSUE: &str = "\
---
title: \"Search notes\"
labels: [enhancement, ui]
state: open
---

# #3 Search notes

**Depends on:** #2 · **Blocks:** –

- [x] Typing filters the list
- [ ] Matches are highlighted

## Comments

### 2026-09-20 10:15

First comment.

### 2026-09-21 16:40 · by hand

Second comment.

- [ ] A task in a comment
";

    fn stamps(file: &IssueFile) -> Vec<&str> {
        file.comments.iter().map(|c| c.stamp.as_str()).collect()
    }

    #[test]
    fn parse_finds_the_heading_the_description_and_the_comments() {
        let parsed = parse(ISSUE);
        assert_eq!(parsed.heading_number, Some(3));
        assert_eq!(parsed.heading_title.as_deref(), Some("Search notes"));
        assert!(parsed.description_text().starts_with("**Depends on:** #2"));
        assert!(
            parsed
                .description_text()
                .ends_with("- [ ] Matches are highlighted")
        );
        assert_eq!(stamps(&parsed), ["2026-09-20 10:15", "2026-09-21 16:40"]);
        assert_eq!(parsed.comments[1].note, "· by hand");
        assert_eq!(parsed.comment_text(&parsed.comments[0]), "First comment.");
        assert_eq!(parsed.comments[0].when().unwrap().hour(), 10);
    }

    #[test]
    fn a_first_heading_that_is_not_the_title_stays_in_the_description() {
        let parsed = parse("---\ntitle: \"Real title\"\n---\n\n# Background\n\nText\n");
        assert!(parsed.heading.is_none());
        assert_eq!(parsed.description_text(), "# Background\n\nText");
    }

    #[test]
    fn a_heading_that_repeats_the_title_without_a_number_is_the_title_heading() {
        let parsed = parse("---\ntitle: \"Real title\"\n---\n\n# Real  title\n\nText\n");
        assert!(parsed.heading.is_some());
        assert_eq!(parsed.heading_number, None);
        assert_eq!(parsed.description_text(), "Text");
    }

    #[test]
    fn a_comments_heading_followed_by_prose_belongs_to_the_description() {
        let parsed = parse("---\ntitle: x\n---\n\n## Comments\n\nHow comments should work.\n");
        assert!(parsed.comments_heading.is_none());
        assert_eq!(
            parsed.description_text(),
            "## Comments\n\nHow comments should work."
        );
    }

    #[test]
    fn a_comments_section_inside_a_code_block_is_not_the_comments_section() {
        let text = "---\ntitle: x\n---\n\n```\n## Comments\n\n### 2026-01-01 00:00\n\nno\n```\n";
        let parsed = parse(text);
        assert!(parsed.comments_heading.is_none());
        assert!(parsed.comments.is_empty());
    }

    #[test]
    fn setting_the_title_changes_the_front_matter_and_the_heading_and_nothing_else() {
        let result = set_title(&parse(ISSUE), "Search notes quickly").unwrap();
        assert_eq!(
            result,
            ISSUE
                .replace("title: \"Search notes\"", "title: \"Search notes quickly\"")
                .replace("# #3 Search notes\n", "# #3 Search notes quickly\n")
        );
    }

    #[test]
    fn setting_the_state_changes_one_line() {
        let result = set_front(&parse(ISSUE), "state", "done").unwrap();
        assert_eq!(result, ISSUE.replace("state: open", "state: done"));
    }

    #[test]
    fn setting_the_description_keeps_the_front_matter_the_heading_and_the_comments() {
        let result =
            set_description(&parse(ISSUE), "\r\nNew text.\r\n\r\n- [ ] one\r\n\n\n").unwrap();
        assert!(result.starts_with(&ISSUE[..ISSUE.find("**Depends").unwrap()]));
        assert!(result.contains("New text.\n\n- [ ] one\n\n## Comments\n"));
        assert!(result.ends_with(&ISSUE[ISSUE.find("## Comments").unwrap()..]));
        assert_eq!(parse(&result).description_text(), "New text.\n\n- [ ] one");
    }

    #[test]
    fn an_empty_description_leaves_one_blank_line_before_the_comments() {
        let result = set_description(&parse(ISSUE), "   ").unwrap();
        assert!(result.contains("# #3 Search notes\n\n## Comments\n"));
        assert_eq!(parse(&result).description_text(), "");
    }

    #[test]
    fn a_description_that_would_start_the_comments_section_is_refused() {
        let file = parse("---\ntitle: x\n---\n");
        assert!(set_description(&file, "## Comments\n\n### 2026-01-01 00:00\n\nx").is_err());
    }

    #[test]
    fn adding_the_first_comment_creates_the_comments_section() {
        let text = "---\ntitle: x\n---\n\n# #1 x\n\nBody\n\n\n";
        let result = add_comment(&parse(text), "2026-09-24 14:05", "Looks good.\r\n").unwrap();
        assert_eq!(
            result,
            "---\ntitle: x\n---\n\n# #1 x\n\nBody\n\n## Comments\n\n### 2026-09-24 14:05\n\nLooks good.\n"
        );
    }

    #[test]
    fn adding_a_comment_appends_it_after_the_last_one() {
        let result = add_comment(&parse(ISSUE), "2026-09-24 14:05", "Third.").unwrap();
        assert_eq!(result, format!("{ISSUE}\n### 2026-09-24 14:05\n\nThird.\n"));
        assert_eq!(parse(&result).comments.len(), 3);
    }

    #[test]
    fn a_line_in_a_comment_that_looks_like_a_comment_heading_is_escaped() {
        let result = add_comment(
            &parse(ISSUE),
            "2026-09-24 14:05",
            "Log:\n### 2026-09-01 10:00\nstarted",
        )
        .unwrap();
        let parsed = parse(&result);
        assert_eq!(parsed.comments.len(), 3);
        assert_eq!(
            parsed.comment_text(&parsed.comments[2]),
            "Log:\n\\### 2026-09-01 10:00\nstarted"
        );
    }

    #[test]
    fn an_empty_comment_is_refused() {
        assert!(add_comment(&parse(ISSUE), "2026-09-24 14:05", " \n ").is_err());
    }

    #[test]
    fn editing_a_comment_replaces_only_its_text() {
        let result = edit_comment(&parse(ISSUE), 0, "Changed.").unwrap();
        assert_eq!(result, ISSUE.replace("First comment.", "Changed."));
    }

    #[test]
    fn editing_the_last_comment_keeps_the_file_ending_in_one_newline() {
        let result = edit_comment(&parse(ISSUE), 1, "Changed.").unwrap();
        assert!(result.ends_with("### 2026-09-21 16:40 · by hand\n\nChanged.\n"));
    }

    #[test]
    fn deleting_a_comment_keeps_the_others() {
        let result = delete_comment(&parse(ISSUE), 0).unwrap();
        assert_eq!(stamps(&parse(&result)), ["2026-09-21 16:40"]);
        assert!(result.starts_with(&ISSUE[..ISSUE.find("### 2026-09-20").unwrap()]));
    }

    #[test]
    fn deleting_the_last_comment_trims_the_blank_lines_it_leaves() {
        let result = delete_comment(&parse(ISSUE), 1).unwrap();
        assert!(result.ends_with("### 2026-09-20 10:15\n\nFirst comment.\n"));
    }

    #[test]
    fn deleting_the_only_comment_removes_the_section() {
        let once = delete_comment(&parse(ISSUE), 1).unwrap();
        let result = delete_comment(&parse(&once), 0).unwrap();
        assert!(result.ends_with("- [ ] Matches are highlighted\n"));
        assert!(!result.contains("## Comments"));
    }

    #[test]
    fn ticking_a_checkbox_changes_one_character() {
        let parsed = parse(ISSUE);
        let line = parsed
            .description_text()
            .split('\n')
            .position(|l| l == "- [ ] Matches are highlighted")
            .unwrap() as i64;
        let result = set_task(&parsed, "description", line, true).unwrap();
        assert_eq!(result, ISSUE.replace("- [ ] Matches", "- [x] Matches"));
        assert_eq!(
            set_task(&parse(&result), "description", line, false).unwrap(),
            ISSUE
        );
    }

    #[test]
    fn ticking_a_checkbox_in_a_comment() {
        let result = set_task(&parse(ISSUE), "1", 2, true).unwrap();
        assert_eq!(
            result,
            ISSUE.replace("- [ ] A task in a comment", "- [x] A task in a comment")
        );
    }

    #[test]
    fn a_checkbox_that_is_no_longer_on_its_line_is_refused() {
        assert!(set_task(&parse(ISSUE), "description", 0, true).is_err());
        assert!(set_task(&parse(ISSUE), "7", 0, true).is_err());
        assert!(set_task(&parse(ISSUE), "description", -1, true).is_err());
        assert!(set_task(&parse(ISSUE), "title", 0, true).is_err());
    }

    #[test]
    fn ticking_a_box_that_is_already_ticked_changes_nothing() {
        assert_eq!(
            set_task(&parse(ISSUE), "description", 2, true).unwrap(),
            ISSUE
        );
    }

    #[test]
    fn task_counts_skip_code_blocks() {
        let text =
            "- [x] one\n- [ ] two\n\n```\n- [ ] not a task\n```\n  * [X] three\n1. [ ] four\n";
        assert_eq!(task_counts(text), (2, 4));
    }

    #[test]
    fn a_bracket_followed_by_text_is_not_a_task() {
        assert_eq!(task_counts("- [x]done\n- [ ]\n"), (0, 1));
    }

    #[test]
    fn editing_a_file_without_front_matter_adds_one() {
        let result = set_front(&parse("# Plain\n\nText\n"), "state", "done").unwrap();
        assert_eq!(result, "---\nstate: done\n---\n\n# Plain\n\nText\n");
    }

    #[test]
    fn front_matter_that_cannot_be_read_is_not_edited() {
        assert!(set_front(&parse("---\ntitle: [broken\n---\n"), "state", "done").is_err());
    }

    #[test]
    fn a_new_issue_has_front_matter_a_numbered_heading_and_the_body() {
        let text = new_issue_text(52, "Export: CSV", &["bug", "needs design"], "\nBody\n\n");
        assert_eq!(
            text,
            "---\ntitle: \"Export: CSV\"\nlabels: [bug, needs design]\nstate: open\n---\n\n\
             # #52 Export: CSV\n\nBody\n"
        );
        let parsed = parse(&text);
        let string = |s: &str| Yaml::String(s.to_string());
        assert_eq!(parsed.front.get("title"), Some(&string("Export: CSV")));
        assert_eq!(
            parsed.front.get("labels"),
            Some(&Yaml::Array(vec![string("bug"), string("needs design")]))
        );
        assert_eq!(parsed.front.get("state"), Some(&string("open")));
        assert_eq!(parsed.front.data.len(), 3);
        assert_eq!(parsed.description_text(), "Body");
    }
}
