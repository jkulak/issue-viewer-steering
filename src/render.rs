//! Markdown to HTML the way GitHub renders an issue, safe to put straight into a page.
//!
//! Raw HTML is escaped, apart from a short list of tags without attributes (details, summary, kbd,
//! sub, sup, …) that can neither run a script nor load anything; HTML comments disappear, as they
//! do on GitHub. Task list items become checkboxes that know the source line they came from, so
//! ticking one in the browser changes exactly that line. `#N` and links to `NNN-slug.md` open that
//! issue.

use std::collections::HashMap;
use std::sync::LazyLock;

use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, html};
use pulldown_cmark_escape::{escape_href, escape_html};
use regex::Regex;

use crate::text::issue_references;

static ALLOWED_TAG: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^</?(?:b|i|u|s|em|strong|code|kbd|sub|sup|br|details|summary|ins|del|mark|small)\s*/?>$")
        .unwrap()
});
static TAG: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"<[^<>]*>").unwrap());
static HTML_COMMENT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?s)<!--.*?-->").unwrap());
static ISSUE_FILE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(?:\./)?([0-9]+)(?:-[^/?#]*)?\.md(?:#.*)?$").unwrap());
static SAFE_DATA: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^data:image/(?:gif|png|jpeg|webp);").unwrap());

/// Titles and states of the issues a text may refer to: number → (title, open).
pub type References = HashMap<u64, (String, bool)>;

#[derive(Debug, Clone, Copy, Default)]
pub struct Context<'a> {
    pub project: Option<&'a str>,
    pub issues: Option<&'a References>,
    /// "description" or a comment index, for the checkboxes.
    pub part: Option<&'a str>,
    /// The checkboxes can be ticked.
    pub interactive: bool,
}

fn escaped(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    escape_html(&mut out, text).expect("writing to a String cannot fail");
    out
}

fn escaped_href(href: &str) -> String {
    let mut out = String::with_capacity(href.len());
    escape_href(&mut out, href).expect("writing to a String cannot fail");
    out
}

/// Raw HTML with comments removed and every tag escaped that is not on the short list.
fn sanitise(content: &str) -> String {
    let content = HTML_COMMENT.replace_all(content, "");
    let mut out = String::with_capacity(content.len());
    let mut position = 0;
    for tag in TAG.find_iter(&content) {
        out += &escaped(&content[position..tag.start()]);
        if ALLOWED_TAG.is_match(tag.as_str()) {
            out += tag.as_str();
        } else {
            out += &escaped(tag.as_str());
        }
        position = tag.end();
    }
    out += &escaped(&content[position..]);
    out
}

/// False for the schemes a browser would run or open as a document instead of navigating to.
fn is_safe_url(url: &str) -> bool {
    let cleaned: String = url
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    let lowered = cleaned.trim_matches(|c: char| c <= ' ').to_lowercase();
    let dangerous = ["javascript:", "vbscript:", "file:", "data:"]
        .iter()
        .any(|scheme| lowered.starts_with(scheme));
    !dangerous || SAFE_DATA.is_match(&lowered)
}

fn checkbox(checked: bool, line: usize, context: &Context) -> String {
    let mut attributes = vec![
        r#"type="checkbox""#.to_string(),
        r#"class="task-list-item-checkbox""#.into(),
    ];
    if checked {
        attributes.push("checked".into());
    }
    match context.part {
        Some(part) if context.interactive => {
            attributes.push(format!(
                r#"data-part="{}" data-line="{line}""#,
                escaped(part)
            ));
        }
        _ => attributes.push("disabled".into()),
    }
    format!("<input {}> ", attributes.join(" "))
}

fn link_open(dest: &str, title: &str, link_type: LinkType, context: &Context) -> String {
    let mut href = dest.to_string();
    let mut rel = false;
    if link_type == LinkType::Email {
        href = format!("mailto:{dest}");
    } else if let (Some(found), Some(project)) = (ISSUE_FILE.captures(dest), context.project) {
        let number: u64 = found[1].parse().unwrap_or_default();
        href = format!("/{project}/issues/{number}");
    } else if ["http://", "https://", "//"]
        .iter()
        .any(|prefix| dest.starts_with(prefix))
    {
        rel = true;
    }
    let mut tag = format!(r#"<a href="{}""#, escaped_href(&href));
    if !title.is_empty() {
        tag += &format!(r#" title="{}""#, escaped(title));
    }
    if rel {
        tag += r#" rel="nofollow noopener noreferrer""#;
    }
    tag + ">"
}

struct Writer<'a, 'c> {
    out: Vec<Event<'a>>,
    context: &'c Context<'c>,
    text: String,
    html_block: Option<String>,
    /// For each open link or image: whether it was written as one.
    links: Vec<bool>,
    images: usize,
    code_block: bool,
    items: Vec<usize>,
    lists: Vec<(usize, Option<u64>)>,
}

impl<'a> Writer<'a, '_> {
    fn html(&mut self, html: String) {
        self.out.push(Event::Html(CowStr::from(html)));
    }

    /// Text is split into several events around `_` and the like; it is collected first so that a
    /// reference is found whole.
    fn flush_text(&mut self) {
        if self.text.is_empty() {
            return;
        }
        let text = std::mem::take(&mut self.text);
        let linkable =
            !self.code_block && self.images == 0 && !self.links.iter().any(|written| *written);
        let (Some(project), Some(issues), true) =
            (self.context.project, self.context.issues, linkable)
        else {
            self.out.push(Event::Text(CowStr::from(text)));
            return;
        };
        let mut position = 0;
        for (range, number) in issue_references(&text) {
            let Some((title, open)) = issues.get(&number) else {
                continue;
            };
            if range.start > position {
                self.out.push(Event::Text(CowStr::from(
                    text[position..range.start].to_string(),
                )));
            }
            let state = if *open { "open" } else { "closed" };
            self.html(format!(
                r#"<a href="/{project}/issues/{number}" class="issue-link" title="{} ({state})">{}</a>"#,
                escaped(title),
                escaped(&text[range.clone()]),
            ));
            position = range.end;
        }
        if position < text.len() {
            self.out
                .push(Event::Text(CowStr::from(text[position..].to_string())));
        }
    }

    fn event(&mut self, event: Event<'a>, line: impl Fn() -> usize) {
        if let Event::Text(text) = &event {
            self.text += text;
            return;
        }
        self.flush_text();
        if let Some(block) = &mut self.html_block {
            match event {
                Event::Html(html) => *block += &html,
                Event::End(TagEnd::HtmlBlock) => {
                    let block = self.html_block.take().unwrap_or_default();
                    self.html(sanitise(&block));
                }
                other => self.out.push(other),
            }
            return;
        }
        if self.images > 0 {
            // Everything inside an image becomes its alt text; leave it to the HTML writer.
            match &event {
                Event::Start(Tag::Image { .. }) => self.images += 1,
                Event::End(TagEnd::Image) => self.images -= 1,
                _ => {}
            }
            self.out.push(event);
            return;
        }
        match event {
            Event::Start(Tag::HtmlBlock) => self.html_block = Some(String::new()),
            Event::Html(html) | Event::InlineHtml(html) => self.html(sanitise(&html)),
            Event::Start(Tag::CodeBlock(kind)) => {
                self.code_block = true;
                self.out.push(Event::Start(Tag::CodeBlock(kind)));
            }
            Event::End(TagEnd::CodeBlock) => {
                self.code_block = false;
                self.out.push(Event::End(TagEnd::CodeBlock));
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                ..
            }) => {
                let safe = is_safe_url(&dest_url);
                if safe {
                    let tag = link_open(&dest_url, &title, link_type, self.context);
                    self.html(tag);
                }
                self.links.push(safe);
            }
            Event::End(TagEnd::Link) => {
                if self.links.pop().unwrap_or(false) {
                    self.html("</a>".into());
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if is_safe_url(&dest_url) {
                    self.images += 1;
                    self.out.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url,
                        title,
                        id,
                    }));
                } else {
                    // Shown as its description, like a refused link.
                    self.links.push(false);
                }
            }
            Event::End(TagEnd::Image) => {
                self.links.pop();
            }
            Event::Start(Tag::Strikethrough) => self.html("<s>".into()),
            Event::End(TagEnd::Strikethrough) => self.html("</s>".into()),
            Event::Start(Tag::List(start)) => {
                self.lists.push((self.out.len(), start));
                self.out.push(Event::Start(Tag::List(start)));
            }
            Event::End(TagEnd::List(ordered)) => {
                self.lists.pop();
                self.out.push(Event::End(TagEnd::List(ordered)));
            }
            Event::Start(Tag::Item) => {
                self.items.push(self.out.len());
                self.out.push(Event::Start(Tag::Item));
            }
            Event::End(TagEnd::Item) => {
                self.items.pop();
                self.out.push(Event::End(TagEnd::Item));
            }
            Event::TaskListMarker(checked) => {
                if let Some(item) = self.items.last().copied() {
                    self.out[item] = Event::Html(CowStr::from(r#"<li class="task-list-item">"#));
                }
                if let Some((list, start)) = self.lists.last().copied() {
                    let tag = match start {
                        None => r#"<ul class="contains-task-list">"#.to_string(),
                        Some(1) => r#"<ol class="contains-task-list">"#.to_string(),
                        Some(start) => {
                            format!(r#"<ol start="{start}" class="contains-task-list">"#)
                        }
                    };
                    self.out[list] = Event::Html(CowStr::from(format!("{tag}\n")));
                }
                let html = checkbox(checked, line(), self.context);
                self.html(html);
            }
            other => self.out.push(other),
        }
    }
}

pub fn render(text: &str, context: &Context) -> String {
    let options =
        Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS;
    let mut writer = Writer {
        out: Vec::new(),
        context,
        text: String::new(),
        html_block: None,
        links: Vec::new(),
        images: 0,
        code_block: false,
        items: Vec::new(),
        lists: Vec::new(),
    };
    for (event, range) in Parser::new_ext(text, options).into_offset_iter() {
        writer.event(event, || text[..range.start].matches('\n').count());
    }
    writer.flush_text();
    let mut out = String::with_capacity(text.len() * 3 / 2);
    html::push_html(&mut out, writer.out.into_iter());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn issues() -> References {
        HashMap::from([
            (2, ("Store notes".to_string(), false)),
            (4, ("Crash on save".to_string(), true)),
        ])
    }

    fn demo(text: &str) -> String {
        render(
            text,
            &Context {
                project: Some("demo"),
                ..Context::default()
            },
        )
    }

    fn with_issues(text: &str) -> String {
        let issues = issues();
        render(
            text,
            &Context {
                project: Some("demo"),
                issues: Some(&issues),
                ..Context::default()
            },
        )
    }

    #[test]
    fn task_list_items_become_checkboxes_that_know_their_line() {
        let context = Context {
            project: Some("demo"),
            part: Some("description"),
            interactive: true,
            ..Context::default()
        };
        let html = render("Intro\n\n- [x] done\n- [ ] todo\n", &context);
        assert!(
            html.contains(r#"<ul class="contains-task-list">"#),
            "{html}"
        );
        assert!(html.contains(
            r#"<li class="task-list-item"><input type="checkbox" class="task-list-item-checkbox" checked data-part="description" data-line="2"> done</li>"#
        ), "{html}");
        assert!(html.contains(r#"data-line="3"> todo"#), "{html}");
    }

    #[test]
    fn a_task_in_a_loose_or_ordered_list_knows_its_line_too() {
        let context = Context {
            project: Some("demo"),
            part: Some("0"),
            interactive: true,
            ..Context::default()
        };
        let html = render("3. [ ] one\n\n4. [x] two\n", &context);
        assert!(
            html.contains(r#"<ol start="3" class="contains-task-list">"#),
            "{html}"
        );
        assert!(
            html.contains(r#"data-part="0" data-line="2"> two"#),
            "{html}"
        );
    }

    #[test]
    fn checkboxes_are_disabled_when_the_page_cannot_write() {
        let context = Context {
            project: Some("demo"),
            part: Some("description"),
            ..Context::default()
        };
        let html = render("- [ ] todo\n", &context);
        assert!(html.contains("disabled"));
        assert!(!html.contains("data-line"));
    }

    #[test]
    fn brackets_that_are_not_a_task_stay_text() {
        assert!(demo("- [link] text\n- [ ]\n").contains("[link] text"));
    }

    #[test]
    fn issue_numbers_link_to_issues_that_exist_and_only_those() {
        let html = with_issues("See #2, #4 and #99. Also [#2](https://example.com/x#2).");
        assert!(html.contains(
            r#"<a href="/demo/issues/2" class="issue-link" title="Store notes (closed)">#2</a>"#
        ));
        assert!(html.contains(
            r#"<a href="/demo/issues/4" class="issue-link" title="Crash on save (open)">#4</a>"#
        ));
        assert!(html.contains("#99") && !html.contains("/issues/99"));
        assert_eq!(html.matches("/demo/issues/2").count(), 1);
    }

    #[test]
    fn a_reference_split_by_the_parser_is_still_found() {
        let html = with_issues("snake_case #2_x and a_b #4");
        assert!(html.contains(r#"href="/demo/issues/4""#), "{html}");
        assert!(!html.contains(r#"href="/demo/issues/2""#), "{html}");
    }

    #[test]
    fn references_inside_code_are_not_linked() {
        assert!(!with_issues("`#2` and\n\n```\n#4\n```\n").contains("issue-link"));
    }

    #[test]
    fn links_to_issue_files_open_the_issue_page() {
        let html = with_issues("[storage](002-store-notes.md) and [readme](../README.md)");
        assert!(html.contains(r#"href="/demo/issues/2""#));
        assert!(html.contains(r#"href="../README.md""#));
    }

    #[test]
    fn raw_html_is_escaped_but_harmless_tags_survive() {
        let html = demo(
            "<script>alert(1)</script>\n\n<img src=x onerror=alert(1)>\n\n\
             <details>\n<summary>More</summary>\n\nHidden\n</details>\n\nPress <kbd>Ctrl</kbd>",
        );
        assert!(!html.contains("<script>"));
        assert!(html.contains("&lt;script&gt;"));
        assert!(!html.contains("<img"));
        assert!(html.contains("<details>") && html.contains("<summary>More</summary>"));
        assert!(html.contains("<kbd>Ctrl</kbd>"));
    }

    #[test]
    fn tags_with_attributes_are_escaped_even_when_the_tag_is_allowed() {
        let html = demo(r#"<details open onclick="x()">text</details>"#);
        assert!(!html.contains("<details open"));
        assert!(html.contains("&lt;details open"));
    }

    #[test]
    fn html_comments_are_hidden() {
        let html = demo("Before\n\n<!-- a note to self -->\n\nAfter <!-- inline --> end");
        assert!(!html.contains("note to self"));
        assert!(!html.contains("inline"));
        assert!(html.contains("After  end"), "{html}");
    }

    #[test]
    fn script_links_are_not_rendered_as_links() {
        let html = demo(
            "[click](javascript:alert(1)) [data](data:text/html,x) [tab](java\tscript:x) ![i](data:image/png;base64,AA)",
        );
        assert!(!html.contains(r#"href="javascript"#));
        assert!(!html.contains(r#"href="data:text"#));
        assert!(!html.contains("script:x\""));
        assert!(html.contains("click"));
        assert!(html.contains(r#"src="data:image/png;base64,AA""#), "{html}");
    }

    #[test]
    fn external_links_do_not_leak_the_page() {
        assert!(demo("<https://example.com/a>").contains(r#"rel="nofollow noopener noreferrer""#));
        assert!(demo("<me@example.com>").contains(r#"href="mailto:me@example.com""#));
    }

    #[test]
    fn tables_and_strikethrough_render() {
        let html = demo("| a | b |\n|---|---|\n| 1 | 2 |\n\n~~gone~~");
        assert!(html.contains("<table>") && html.contains("<td>2</td>"));
        assert!(html.contains("<s>gone</s>"));
    }

    #[test]
    fn image_descriptions_are_not_turned_into_links() {
        let html = with_issues("![see #2](pic.png)");
        assert!(html.contains(r##"alt="see #2""##), "{html}");
        assert!(!html.contains("issue-link"));
    }
}
