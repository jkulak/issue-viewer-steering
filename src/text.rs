//! String helpers shared by the modules that read and write issue files.

use yaml_rust2::Yaml;

/// Case folding for comparisons that should not care about case: lower case, plus the folds that
/// lower casing alone misses.
pub fn casefold(text: &str) -> String {
    let lower = text.to_lowercase();
    if lower.contains(['ß', 'ſ', 'ς']) {
        lower.replace('ß', "ss").replace('ſ', "s").replace('ς', "σ")
    } else {
        lower
    }
}

/// Runs of whitespace as one space, without whitespace at either end.
pub fn collapse(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn is_blank(text: &str) -> bool {
    text.trim().is_empty()
}

/// A string quoted for an error message, so an empty value or one with spaces is visible.
pub fn quoted(text: &str) -> String {
    let quote = if text.contains('\'') && !text.contains('"') {
        '"'
    } else {
        '\''
    };
    let mut out = String::with_capacity(text.len() + 2);
    out.push(quote);
    for char in text.chars() {
        match char {
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c == quote => {
                out.push('\\');
                out.push(c);
            }
            c if c.is_control() => out.push_str(&format!("\\x{:02x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push(quote);
    out
}

/// Whether a character counts as part of a word, for the `#12` reference rules.
pub fn is_word_char(char: char) -> bool {
    char.is_alphanumeric() || char == '_'
}

/// `#N` references in `text`: the byte range of each and the number it names.
///
/// A `#` right after a word character, `&`, `/` or another `#` is part of something else (an
/// HTML entity, a URL fragment, a heading), and a number running into letters is not a reference.
pub fn issue_references(text: &str) -> Vec<(std::ops::Range<usize>, u64)> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut position = 0;
    while let Some(offset) = text[position..].find('#') {
        let start = position + offset;
        let digits = bytes[start + 1..]
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count();
        position = start + 1;
        if digits == 0 {
            continue;
        }
        let end = start + 1 + digits;
        position = end;
        let before = text[..start].chars().next_back();
        if before.is_some_and(|c| is_word_char(c) || c == '&' || c == '/' || c == '#') {
            continue;
        }
        if text[end..].chars().next().is_some_and(is_word_char) {
            continue;
        }
        if let Ok(number) = text[start + 1..end].parse() {
            found.push((start..end, number));
        }
    }
    found
}

/// A YAML value as text, the way it is shown where a string is expected: a title, a state, a
/// label.
pub fn yaml_text(value: &Yaml) -> String {
    match value {
        Yaml::String(text) => text.clone(),
        Yaml::Integer(number) => number.to_string(),
        Yaml::Real(text) => real_text(text),
        Yaml::Boolean(true) => "True".into(),
        Yaml::Boolean(false) => "False".into(),
        Yaml::Null | Yaml::BadValue | Yaml::Alias(_) => String::new(),
        Yaml::Array(items) => {
            let items: Vec<String> = items.iter().map(yaml_repr).collect();
            format!("[{}]", items.join(", "))
        }
        Yaml::Hash(hash) => {
            let pairs: Vec<String> = hash
                .iter()
                .map(|(key, value)| format!("{}: {}", yaml_repr(key), yaml_repr(value)))
                .collect();
            format!("{{{}}}", pairs.join(", "))
        }
    }
}

fn yaml_repr(value: &Yaml) -> String {
    match value {
        Yaml::String(text) => quoted(text),
        Yaml::Null => "None".into(),
        other => yaml_text(other),
    }
}

/// A float as written, unless it is one of the spellings YAML 1.2 and 1.1 agree on.
fn real_text(text: &str) -> String {
    match text.parse::<f64>() {
        Ok(number) if number.is_finite() && text.contains('.') && !text.contains(['e', 'E']) => {
            if number.fract() == 0.0 && number.abs() < 1e16 {
                format!("{number:.1}")
            } else {
                number.to_string()
            }
        }
        _ => text.to_string(),
    }
}

/// Whether a YAML value counts as set: not null, not false, not zero, not empty.
pub fn yaml_truthy(value: &Yaml) -> bool {
    match value {
        Yaml::String(text) => !text.is_empty(),
        Yaml::Integer(number) => *number != 0,
        Yaml::Real(text) => text.parse::<f64>().map(|n| n != 0.0).unwrap_or(true),
        Yaml::Boolean(flag) => *flag,
        Yaml::Array(items) => !items.is_empty(),
        Yaml::Hash(hash) => !hash.is_empty(),
        Yaml::Null | Yaml::BadValue | Yaml::Alias(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn casefolding_folds_the_sharp_s() {
        assert_eq!(casefold("Straße"), "strasse");
    }

    #[test]
    fn references_skip_entities_fragments_and_words() {
        let found: Vec<u64> = issue_references("#1 a#2 &#3; /#4 ##5 #6x (#7) #8")
            .into_iter()
            .map(|(_, n)| n)
            .collect();
        assert_eq!(found, vec![1, 7, 8]);
    }

    #[test]
    fn quoted_values_look_like_they_do_in_the_old_messages() {
        assert_eq!(quoted("notes"), "'notes'");
        assert_eq!(quoted("it's"), "\"it's\"");
    }
}
