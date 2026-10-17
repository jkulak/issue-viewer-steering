//! Small SVG icons, drawn for this app on a 16x16 grid, in the current text colour.

const SHAPES: &[(&str, &str)] = &[
    (
        "issue-opened",
        r##"<circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="8" cy="8" r="1.6" fill="currentColor"/>"##,
    ),
    (
        "issue-closed",
        r##"<circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.4 8.3 7.2 10.1 10.7 6.4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "issue-not-planned",
        r##"<circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M3.9 12.1 12.1 3.9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "blocked",
        r##"<circle cx="8" cy="8" r="6.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.9 8h6.2" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "comment",
        r##"<path d="M2.75 2.5h10.5c.69 0 1.25.56 1.25 1.25v6.5c0 .69-.56 1.25-1.25 1.25H8.6L5.5 14v-2.5H2.75c-.69 0-1.25-.56-1.25-1.25v-6.5c0-.69.56-1.25 1.25-1.25Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"##,
    ),
    (
        "tag",
        r##"<path d="M1.75 2.75v4.5c0 .27.11.52.29.71l6.25 6.25a1 1 0 0 0 1.42 0l4.5-4.5a1 1 0 0 0 0-1.42L7.96 2.04a1 1 0 0 0-.71-.29h-4.5a1 1 0 0 0-1 1Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><circle cx="5" cy="5" r="1.1" fill="currentColor"/>"##,
    ),
    (
        "search",
        r##"<circle cx="7" cy="7" r="4.75" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="m10.5 10.5 3.75 3.75" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "gear",
        r##"<path d="M13.14,6.72 L15.13,7.00 L15.13,9.00 L13.14,9.28 L12.54,10.73 L13.75,12.33 L12.33,13.75 L10.73,12.54 L9.28,13.14 L9.00,15.13 L7.00,15.13 L6.72,13.14 L5.27,12.54 L3.67,13.75 L2.25,12.33 L3.46,10.73 L2.86,9.28 L0.87,9.00 L0.87,7.00 L2.86,6.72 L3.46,5.27 L2.25,3.67 L3.67,2.25 L5.27,3.46 L6.72,2.86 L7.00,0.87 L9.00,0.87 L9.28,2.86 L10.73,3.46 L12.33,2.25 L13.75,3.67 L12.54,5.27 Z" fill="none" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round"/><circle cx="8" cy="8" r="2.1" fill="none" stroke="currentColor" stroke-width="1.3"/>"##,
    ),
    (
        "triangle-down",
        r##"<path d="M4.4 6.2h7.2a.4.4 0 0 1 .3.66L8.3 10.6a.4.4 0 0 1-.6 0L4.1 6.86a.4.4 0 0 1 .3-.66Z" fill="currentColor"/>"##,
    ),
    (
        "check",
        r##"<path d="m3 8.5 3 3 7-7" fill="none" stroke="currentColor" stroke-width="1.75" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "x",
        r##"<path d="m4 4 8 8M12 4l-8 8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "kebab",
        r##"<circle cx="3" cy="8" r="1.3" fill="currentColor"/><circle cx="8" cy="8" r="1.3" fill="currentColor"/><circle cx="13" cy="8" r="1.3" fill="currentColor"/>"##,
    ),
    (
        "repo",
        r##"<path d="M3.25 13V3c0-.69.56-1.25 1.25-1.25h8.25v9.5H4.5c-.69 0-1.25.56-1.25 1.25S3.81 14.25 4.5 14.25h8.25" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M6 4.75h4" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "lock",
        r##"<rect x="3" y="7" width="10" height="7.25" rx="1.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5.25 7V5a2.75 2.75 0 0 1 5.5 0v2" fill="none" stroke="currentColor" stroke-width="1.5"/>"##,
    ),
    (
        "folder",
        r##"<path d="M1.75 3.5c0-.41.34-.75.75-.75h3.4l1.5 1.75h6.1c.41 0 .75.34.75.75v7.25c0 .41-.34.75-.75.75H2.5a.75.75 0 0 1-.75-.75Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"##,
    ),
    (
        "file",
        r##"<path d="M3.75 1.75h5.5l3 3v9.25a.25.25 0 0 1-.25.25h-8.25a.25.25 0 0 1-.25-.25V2a.25.25 0 0 1 .25-.25Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M9 1.75V5h3.25" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/>"##,
    ),
    (
        "copy",
        r##"<rect x="5.25" y="5.25" width="8.5" height="8.5" rx="1.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M10.75 5.25V3.5c0-.69-.56-1.25-1.25-1.25H3.5c-.69 0-1.25.56-1.25 1.25v6c0 .69.56 1.25 1.25 1.25h1.75" fill="none" stroke="currentColor" stroke-width="1.5"/>"##,
    ),
    (
        "pencil",
        r##"<path d="M10.9 2.6a1.5 1.5 0 0 1 2.1 0l.4.4a1.5 1.5 0 0 1 0 2.1L6 12.5l-3.25.75.75-3.25Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="m9.75 3.75 2.5 2.5" fill="none" stroke="currentColor" stroke-width="1.5"/>"##,
    ),
    (
        "trash",
        r##"<path d="M2.75 4.25h10.5M6.25 4.25V2.75h3.5v1.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M4 4.25l.7 9a1 1 0 0 0 1 .95h4.6a1 1 0 0 0 1-.95l.7-9" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "link",
        r##"<path d="M6.9 9.1a2.6 2.6 0 0 0 3.7 0l2.2-2.2a2.6 2.6 0 0 0-3.7-3.7l-.8.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M9.1 6.9a2.6 2.6 0 0 0-3.7 0L3.2 9.1a2.6 2.6 0 0 0 3.7 3.7l.8-.8" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "markdown",
        r##"<rect x="1" y="3.25" width="14" height="9.5" rx="1.5" fill="none" stroke="currentColor" stroke-width="1.25"/><path d="M3.5 10.5v-5l2 2.25 2-2.25v5" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linejoin="round"/><path d="M11 5.5v5m-1.75-1.75L11 10.5l1.75-1.75" fill="none" stroke="currentColor" stroke-width="1.25" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "alert",
        r##"<path d="M7.13 2.5a1 1 0 0 1 1.74 0l5.5 9.75a1 1 0 0 1-.87 1.5H2.5a1 1 0 0 1-.87-1.5Z" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linejoin="round"/><path d="M8 6.25v3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><circle cx="8" cy="11.4" r=".9" fill="currentColor"/>"##,
    ),
    (
        "chevron-left",
        r##"<path d="M9.75 3.5 5.25 8l4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "chevron-right",
        r##"<path d="m6.25 3.5 4.5 4.5-4.5 4.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "tasklist",
        r##"<path d="m1.75 3.75 1.25 1.25 2.25-2.5M1.75 10.25l1.25 1.25 2.25-2.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><path d="M8 4.25h6.25M8 10.75h6.25" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "git-branch",
        r##"<circle cx="4.5" cy="3.25" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="4.5" cy="12.75" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="11.5" cy="4.75" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 4.75v6.5M11.5 6.25c0 3-7 2.25-7 5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "project",
        r##"<rect x="1.75" y="1.75" width="12.5" height="12.5" rx="1.25" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M5 4.75v6.5M8 4.75v3.5M11 4.75v5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round"/>"##,
    ),
    (
        "git-pull-request",
        r##"<circle cx="4.5" cy="3.25" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="4.5" cy="12.75" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="11.5" cy="12.75" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 4.75v6.5M11.5 11.25V6.75c0-1.1-.9-2-2-2H7M8.75 3 7 4.75 8.75 6.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "git-pull-request-draft",
        r##"<circle cx="4.5" cy="3.25" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="4.5" cy="12.75" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><circle cx="11.5" cy="12.75" r="1.5" fill="none" stroke="currentColor" stroke-width="1.5"/><path d="M4.5 4.75v6.5" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/><circle cx="11.5" cy="8.25" r="1" fill="currentColor"/><circle cx="11.5" cy="4" r="1" fill="currentColor"/>"##,
    ),
    (
        "link-external",
        r##"<path d="M9.5 2.25h4.25V6.5M13.5 2.5 8 8M11.75 9.5v3c0 .69-.56 1.25-1.25 1.25h-7c-.69 0-1.25-.56-1.25-1.25v-7c0-.69.56-1.25 1.25-1.25h3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
    (
        "sync",
        r##"<path d="M2.75 8a5.25 5.25 0 0 1 9.2-3.45M13.25 8a5.25 5.25 0 0 1-9.2 3.45M12.25 1.75v3h-3M3.75 14.25v-3h3" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"/>"##,
    ),
];

const LOGO: &str = r##"<svg class="icon logo" viewBox="0 0 32 32" width="{size}" height="{size}" aria-hidden="true"><rect class="logo-tile" width="32" height="32" rx="8"/><circle cx="16" cy="16" r="8.5" fill="none" stroke="#fff" stroke-width="2.75"/><circle cx="16" cy="16" r="2.75" fill="#fff"/></svg>"##;

pub fn exists(name: &str) -> bool {
    name == "logo" || SHAPES.iter().any(|(known, _)| *known == name)
}

/// The SVG for an icon, or None for a name there is no icon for.
pub fn icon(name: &str, size: u32, class: &str) -> Option<String> {
    if name == "logo" {
        return Some(LOGO.replace("{size}", &size.to_string()));
    }
    let shape = SHAPES.iter().find(|(known, _)| *known == name)?.1;
    let classes = format!("icon icon-{name} {class}");
    Some(format!(
        r#"<svg class="{}" viewBox="0 0 16 16" width="{size}" height="{size}" aria-hidden="true" focusable="false">{shape}</svg>"#,
        classes.trim()
    ))
}

/// A ring that fills up as the tasks get done, like the one in GitHub's issue list.
pub fn progress(done: usize, total: usize) -> String {
    let circumference = 2.0 * std::f64::consts::PI * 6.0;
    let filled = if total == 0 {
        0.0
    } else {
        circumference * done as f64 / total as f64
    };
    let state = if total > 0 && done == total {
        "complete"
    } else {
        "partial"
    };
    format!(
        concat!(
            r#"<svg class="icon progress-ring progress-{}" viewBox="0 0 16 16" width="16" height="16" "#,
            r#"aria-hidden="true"><circle class="progress-track" cx="8" cy="8" r="6" fill="none" "#,
            r#"stroke-width="2"/><circle class="progress-value" cx="8" cy="8" r="6" fill="none" "#,
            r#"stroke-width="2" stroke-dasharray="{:.2} {:.2}" transform="rotate(-90 8 8)"/></svg>"#
        ),
        state, filled, circumference
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_icon_is_an_svg_with_its_name_as_a_class() {
        let svg = icon("tag", 16, "").unwrap();
        assert!(svg.starts_with(
            r#"<svg class="icon icon-tag" viewBox="0 0 16 16" width="16" height="16""#
        ));
        assert!(
            icon("logo", 24, "")
                .unwrap()
                .contains(r#"width="24" height="24""#)
        );
        assert_eq!(icon("nope", 16, ""), None);
    }

    #[test]
    fn the_progress_ring_fills_with_the_share_of_tasks_done() {
        assert!(progress(1, 2).contains(r#"stroke-dasharray="18.85 37.70""#));
        assert!(progress(2, 2).contains("progress-complete"));
        assert!(progress(0, 0).contains(r#"progress-partial"#));
    }
}
