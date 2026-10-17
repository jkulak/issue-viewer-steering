//! Front matter: the YAML block between two `---` lines at the top of an issue file.
//!
//! yaml-rust2 reads it. Writing never re-serialises the block: an edit replaces the lines of one
//! key and leaves every other line, comment and quoting style as it was, so a hand-edited file
//! changes only where the viewer meant to change it.

use std::sync::LazyLock;

use regex::Regex;
use yaml_rust2::yaml::Hash;
use yaml_rust2::{ScanError, Yaml, YamlLoader};

static KEY_LINE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([A-Za-z_][\w-]*)[ \t]*:(?:[ \t]|$)").unwrap());

/// Values that YAML 1.1 readers (PyYAML among them) take for something other than a string,
/// though yaml-rust2, which follows YAML 1.2, reads them as strings. A value like this is written
/// quoted, so every reader of the file sees the same string.
static YAML_1_1_IMPLICIT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(concat!(
        r"^(?:",
        r"y|Y|n|N|yes|Yes|YES|no|No|NO|true|True|TRUE|false|False|FALSE|on|On|ON|off|Off|OFF",
        r"|[-+]?0b[0-1_]+|[-+]?0[0-7_]+|[-+]?(?:0|[1-9][0-9_]*)|[-+]?0x[0-9a-fA-F_]+",
        r"|[-+]?[1-9][0-9_]*(?::[0-5]?[0-9])+",
        r"|[-+]?(?:[0-9][0-9_]*)\.[0-9_]*(?:[eE][-+][0-9]+)?|\.[0-9][0-9_]*(?:[eE][-+][0-9]+)?",
        r"|[-+]?[0-9][0-9_]*(?::[0-5]?[0-9])+\.[0-9_]*|[-+]?\.(?:inf|Inf|INF)|\.(?:nan|NaN|NAN)",
        r"|~|null|Null|NULL",
        r"|[0-9]{4}-[0-9]{2}-[0-9]{2}",
        r"|[0-9]{4}-[0-9]{1,2}-[0-9]{1,2}(?:[Tt]|[ \t]+)[0-9]{1,2}:[0-9]{2}:[0-9]{2}(?:\.[0-9]*)?",
        r"(?:[ \t]*(?:Z|[-+][0-9]{1,2}(?::[0-9]{2})?))?",
        r"|=|<<",
        r")$"
    ))
    .unwrap()
});

#[derive(Debug, Clone, Default, PartialEq)]
pub struct FrontMatter {
    pub present: bool,
    /// First byte of the YAML text, after the opening line.
    pub raw_start: usize,
    /// First byte of the closing line.
    pub raw_end: usize,
    /// First byte after the closing line.
    pub body_start: usize,
    pub data: Hash,
    pub error: Option<String>,
}

impl FrontMatter {
    pub fn raw<'a>(&self, text: &'a str) -> &'a str {
        &text[self.raw_start..self.raw_end]
    }

    /// The value of `key`, treating a key set to nothing like a missing one.
    pub fn get(&self, key: &str) -> Option<&Yaml> {
        get(&self.data, key)
    }
}

pub fn get<'a>(data: &'a Hash, key: &str) -> Option<&'a Yaml> {
    data.get(&Yaml::String(key.to_string()))
        .filter(|value| !matches!(value, Yaml::Null))
}

fn dashes_only(line: &str, marker: &str) -> bool {
    line.strip_prefix(marker)
        .is_some_and(|rest| rest.chars().all(|c| c == ' ' || c == '\t'))
}

pub fn split(text: &str) -> FrontMatter {
    let Some(first_end) = text.find('\n') else {
        return FrontMatter::default();
    };
    if !dashes_only(&text[..first_end], "---") {
        return FrontMatter::default();
    }
    let mut position = first_end + 1;
    while position < text.len() {
        let line_end = text[position..].find('\n').map(|offset| position + offset);
        let line = &text[position..line_end.unwrap_or(text.len())];
        if dashes_only(line, "---") || dashes_only(line, "...") {
            let (data, error) = load(&text[first_end + 1..position]);
            return FrontMatter {
                present: true,
                raw_start: first_end + 1,
                raw_end: position,
                body_start: line_end.map_or(text.len(), |end| end + 1),
                data,
                error,
            };
        }
        match line_end {
            Some(end) => position = end + 1,
            None => break,
        }
    }
    // An opening line that is never closed is a thematic break, not front matter.
    FrontMatter::default()
}

pub fn load(raw: &str) -> (Hash, Option<String>) {
    if raw.trim().is_empty() {
        return (Hash::new(), None);
    }
    match YamlLoader::load_from_str(raw) {
        Err(error) => (Hash::new(), Some(describe(&error))),
        Ok(documents) => match documents.into_iter().next() {
            None | Some(Yaml::Null) => (Hash::new(), None),
            Some(Yaml::Hash(data)) => (data, None),
            Some(_) => (
                Hash::new(),
                Some("it is not a list of key: value lines".into()),
            ),
        },
    }
}

fn describe(error: &ScanError) -> String {
    let info = error.info();
    let problem = match info.strip_suffix(": duplicated key in mapping") {
        Some(key) => {
            let key = key
                .strip_prefix("String(\"")
                .and_then(|k| k.strip_suffix("\")"))
                .unwrap_or(key);
            format!("the key “{key}” appears twice")
        }
        None => info.to_string(),
    };
    // The YAML starts on the second line of the file.
    format!("{problem} (line {})", error.marker().line() + 1)
}

fn key_of(line: &str) -> Option<&str> {
    let line = line.strip_suffix('\n').unwrap_or(line);
    KEY_LINE
        .captures(line)
        .map(|captures| captures.get(1).unwrap().as_str())
}

/// True for a line that belongs to the value of the key above it.
fn continues(line: &str) -> bool {
    !line.trim().is_empty() && (line.starts_with([' ', '\t']) || line.starts_with('-'))
}

fn key_lines(lines: &[&str], key: &str) -> Option<(usize, usize)> {
    let start = lines.iter().position(|line| key_of(line) == Some(key))?;
    let mut end = start + 1;
    while end < lines.len() {
        if continues(lines[end]) {
            end += 1;
            continue;
        }
        if lines[end].trim().is_empty() {
            // A blank line is part of the value only when the value goes on after it.
            let mut after = end;
            while after < lines.len() && lines[after].trim().is_empty() {
                after += 1;
            }
            if after < lines.len() && continues(lines[after]) {
                end = after + 1;
                continue;
            }
        }
        break;
    }
    Some((start, end))
}

/// Put `key: rendered` in place of the lines of `key`; `None` removes it; new keys go last.
pub fn set_key(raw: &str, key: &str, rendered: Option<&str>) -> String {
    let lines: Vec<&str> = raw.split_inclusive('\n').collect();
    let replacement = rendered.map(|value| format!("{key}: {value}\n"));
    match key_lines(&lines, key) {
        None => {
            let Some(replacement) = replacement else {
                return raw.to_string();
            };
            let mut out = raw.to_string();
            if !out.is_empty() && !out.ends_with('\n') {
                out.push('\n');
            }
            out + &replacement
        }
        Some((start, end)) => {
            let mut out: String = lines[..start].concat();
            out.push_str(replacement.as_deref().unwrap_or(""));
            out.push_str(&lines[end..].concat());
            out
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Style {
    Double,
    Single,
    Plain,
}

/// How the value of `key` is quoted now.
pub fn style_of(raw: &str, key: &str) -> Style {
    for line in raw.lines() {
        if key_of(line) == Some(key) {
            let value = line
                .split_once(':')
                .map_or("", |(_, value)| value)
                .trim_start();
            return if value.starts_with('"') {
                Style::Double
            } else if value.starts_with('\'') {
                Style::Single
            } else {
                Style::Plain
            };
        }
    }
    Style::Plain
}

fn reads_as_itself(value: &str) -> bool {
    !value.is_empty()
        && value == value.trim()
        && !value.contains('\n')
        && !value.starts_with(['%', '@', '`'])
        && !YAML_1_1_IMPLICIT.is_match(value)
}

fn plain_ok(value: &str) -> bool {
    if !reads_as_itself(value) {
        return false;
    }
    match YamlLoader::load_from_str(&format!("k: {value}")) {
        Ok(documents) => match documents.first() {
            Some(Yaml::Hash(hash)) => {
                hash.len() == 1 && get(hash, "k") == Some(&Yaml::String(value.to_string()))
            }
            _ => false,
        },
        Err(_) => false,
    }
}

/// A JSON string is a valid YAML double-quoted scalar.
fn double_quoted(value: &str) -> String {
    serde_json::to_string(value).expect("a string always serialises")
}

pub fn render_scalar(value: &str, style: Style) -> String {
    if style == Style::Single && !value.contains('\n') {
        return format!("'{}'", value.replace('\'', "''"));
    }
    if style == Style::Plain && plain_ok(value) {
        return value.to_string();
    }
    double_quoted(value)
}

fn flow_plain_ok(item: &str) -> bool {
    if !reads_as_itself(item) || item.contains([',', '[', ']', '{', '}', '"', '\'']) {
        return false;
    }
    match YamlLoader::load_from_str(&format!("[{item}]")) {
        Ok(documents) => match documents.first() {
            Some(Yaml::Array(items)) => items.as_slice() == [Yaml::String(item.to_string())],
            _ => false,
        },
        Err(_) => false,
    }
}

pub fn render_list<S: AsRef<str>>(items: &[S]) -> String {
    let parts: Vec<String> = items
        .iter()
        .map(|item| {
            let item = item.as_ref();
            if flow_plain_ok(item) {
                item.to_string()
            } else {
                double_quoted(item)
            }
        })
        .collect();
    format!("[{}]", parts.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn string(value: &str) -> Yaml {
        Yaml::String(value.to_string())
    }

    fn hash(pairs: Vec<(&str, Yaml)>) -> Hash {
        pairs
            .into_iter()
            .map(|(key, value)| (string(key), value))
            .collect()
    }

    #[test]
    fn front_matter_is_read_from_between_the_two_dashed_lines() {
        let text = "---\ntitle: Hello\nlabels: [a, b]\nstate: open\n---\n\nBody\n";
        let front = split(text);
        assert!(front.present);
        assert_eq!(
            front.data,
            hash(vec![
                ("title", string("Hello")),
                ("labels", Yaml::Array(vec![string("a"), string("b")])),
                ("state", string("open")),
            ])
        );
        assert_eq!(&text[front.body_start..], "\nBody\n");
        assert_eq!(
            front.raw(text),
            "title: Hello\nlabels: [a, b]\nstate: open\n"
        );
    }

    #[test]
    fn a_file_without_front_matter_has_no_data_and_no_error() {
        let front = split("# Title\n\nBody\n");
        assert!(!front.present);
        assert!(front.data.is_empty());
        assert_eq!(front.error, None);
        assert_eq!(front.body_start, 0);
    }

    #[test]
    fn an_opening_line_without_a_closing_line_is_a_thematic_break() {
        assert!(!split("---\ntitle: never closed\n").present);
    }

    #[test]
    fn broken_yaml_is_reported_with_the_line_it_is_on() {
        let front = split("---\ntitle: ok\nlabels: [bug\n---\n");
        assert!(front.present);
        assert!(front.data.is_empty());
        assert!(front.error.unwrap().contains("line"));
    }

    #[test]
    fn a_key_written_twice_is_reported_rather_than_silently_dropped() {
        let front = split("---\ntitle: a\nstate: open\ntitle: b\n---\n");
        assert!(
            front
                .error
                .unwrap()
                .starts_with("the key “title” appears twice (line ")
        );
    }

    #[test]
    fn setting_a_key_replaces_only_its_line() {
        let raw = "title: Hello  # keep this comment\nlabels: [a]\nstate: open\nowner: someone\n";
        assert_eq!(
            set_key(raw, "state", Some("done")),
            "title: Hello  # keep this comment\nlabels: [a]\nstate: done\nowner: someone\n"
        );
    }

    #[test]
    fn setting_a_key_replaces_a_block_list_with_all_its_items() {
        let raw = "title: Hello\nlabels:\n  - a\n\n  - b\nstate: open\n";
        assert_eq!(
            set_key(raw, "labels", Some("[c]")),
            "title: Hello\nlabels: [c]\nstate: open\n"
        );
    }

    #[test]
    fn setting_a_key_replaces_a_list_written_at_the_same_indent() {
        let raw = "labels:\n- a\n- b\nstate: open\n";
        assert_eq!(
            set_key(raw, "labels", Some("[]")),
            "labels: []\nstate: open\n"
        );
    }

    #[test]
    fn a_missing_key_is_added_at_the_end() {
        assert_eq!(
            set_key("title: Hello", "state", Some("open")),
            "title: Hello\nstate: open\n"
        );
    }

    #[test]
    fn removing_a_key_takes_its_lines_out() {
        assert_eq!(
            set_key("a: 1\nb:\n  - x\nc: 3\n", "b", None),
            "a: 1\nc: 3\n"
        );
    }

    #[test]
    fn render_scalar_keeps_a_plain_title_plain_and_quotes_what_yaml_would_misread() {
        assert_eq!(
            render_scalar("Store notes in SQLite", Style::Plain),
            "Store notes in SQLite"
        );
        assert_eq!(
            render_scalar("Search: by title", Style::Plain),
            "\"Search: by title\""
        );
        assert_eq!(render_scalar("yes", Style::Plain), "\"yes\"");
        assert_eq!(render_scalar("2026", Style::Plain), "\"2026\"");
        assert_eq!(render_scalar("2026-09-20", Style::Plain), "\"2026-09-20\"");
        assert_eq!(
            render_scalar("# not a comment", Style::Plain),
            "\"# not a comment\""
        );
    }

    #[test]
    fn render_scalar_keeps_the_quote_style_the_file_already_uses() {
        assert_eq!(
            render_scalar("Plain words", Style::Double),
            "\"Plain words\""
        );
        assert_eq!(render_scalar("It's here", Style::Single), "'It''s here'");
        assert_eq!(
            style_of("title: \"x\"\nstate: open\n", "title"),
            Style::Double
        );
        assert_eq!(
            style_of("title: \"x\"\nstate: open\n", "state"),
            Style::Plain
        );
    }

    #[test]
    fn rendered_values_read_back_as_the_same_value() {
        for value in [
            "a: b",
            "say \"hi\"",
            "back\\slash",
            "naïve café, Straße",
            "- dash",
            "[x]",
        ] {
            for style in [Style::Plain, Style::Double, Style::Single] {
                let rendered = render_scalar(value, style);
                assert_eq!(
                    load(&format!("title: {rendered}\n")),
                    (hash(vec![("title", string(value))]), None),
                    "{value} as {style:?}"
                );
            }
        }
    }

    #[test]
    fn render_list_quotes_only_the_items_that_need_it() {
        assert_eq!(
            render_list(&["bug", "good first issue"]),
            "[bug, good first issue]"
        );
        assert_eq!(render_list(&["a,b", "yes", "ok"]), "[\"a,b\", \"yes\", ok]");
        assert_eq!(render_list::<&str>(&[]), "[]");
        let labels = ["x: y", "[z]", "true", "wiki-api"];
        let (data, error) = load(&format!("labels: {}\n", render_list(&labels)));
        assert_eq!(error, None);
        assert_eq!(
            data,
            hash(vec![(
                "labels",
                Yaml::Array(labels.iter().map(|l| string(l)).collect())
            )])
        );
    }
}
