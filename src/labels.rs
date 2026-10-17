//! Colours for labels, which in a file are only names.
//!
//! GitHub's default labels keep their usual colours; any other name gets one from a fixed palette,
//! chosen by a hash of the name, so a label looks the same on every page and after every restart.

use sha2::{Digest, Sha256};

use crate::text::casefold;

const KNOWN: [(&str, &str); 10] = [
    ("bug", "d73a4a"),
    ("documentation", "0075ca"),
    ("duplicate", "cfd3d7"),
    ("enhancement", "a2eeef"),
    ("good first issue", "7057ff"),
    ("help wanted", "008672"),
    ("invalid", "e4e669"),
    ("question", "d876e3"),
    ("wontfix", "ffffff"),
    ("backlog", "ededed"),
];
pub const PALETTE: [&str; 20] = [
    "0e8a16", "1d76db", "5319e7", "b60205", "d93f0b", "fbca04", "006b75", "0052cc", "c5def5",
    "bfd4f2", "d4c5f9", "f9d0c4", "fef2c0", "c2e0c6", "bfdadc", "e99695", "84b6eb", "fad8c7",
    "bfe5bf", "c9a8ff",
];
const AVATARS: [&str; 8] = [
    "1f883d", "0969da", "8250df", "bf3989", "cf222e", "bc4c00", "1b7c83", "4d2d8f",
];

/// The digest of the folded name, read as one big-endian number, modulo the number of choices.
fn pick<'a>(name: &str, choices: &[&'a str]) -> &'a str {
    let digest = Sha256::digest(casefold(name).as_bytes());
    let count = choices.len() as u64;
    let index = digest
        .iter()
        .fold(0u64, |acc, byte| (acc * 256 + u64::from(*byte)) % count);
    choices[index as usize]
}

pub fn color(name: &str) -> &'static str {
    let folded = casefold(name);
    KNOWN
        .iter()
        .find(|(known, _)| *known == folded)
        .map_or_else(|| pick(name, &PALETTE), |(_, color)| color)
}

pub fn text_color(background: &str) -> &'static str {
    let channel = |at: usize| {
        f64::from(u8::from_str_radix(background.get(at..at + 2).unwrap_or("00"), 16).unwrap_or(0))
    };
    let lightness = (channel(0) * 0.2126 + channel(2) * 0.7152 + channel(4) * 0.0722) / 255.0;
    if lightness > 0.453 {
        "#1f2328"
    } else {
        "#ffffff"
    }
}

pub fn style(name: &str) -> String {
    let background = color(name);
    format!(
        "background-color:#{background};color:{}",
        text_color(background)
    )
}

pub fn avatar_style(name: &str) -> String {
    format!("background-color:#{}", pick(name, &AVATARS))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_labels_keep_the_usual_colours_and_others_get_a_stable_one() {
        assert_eq!(color("bug"), "d73a4a");
        assert_eq!(color("Enhancement"), "a2eeef");
        assert_eq!(color("wiki-api"), color("wiki-api"));
        assert!(PALETTE.contains(&color("wiki-api")));
    }

    #[test]
    fn label_text_is_dark_on_light_colours_and_white_on_dark_ones() {
        assert_eq!(text_color("a2eeef"), "#1f2328");
        assert_eq!(text_color("0052cc"), "#ffffff");
        assert_eq!(style("bug"), "background-color:#d73a4a;color:#ffffff");
    }

    #[test]
    fn colours_match_the_ones_the_earlier_version_picked() {
        assert_eq!(color("wiki-api"), "0e8a16");
        assert_eq!(color("needs design"), "d93f0b");
        assert_eq!(avatar_style("someone"), "background-color:#bf3989");
    }
}
