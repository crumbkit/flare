//! Unit tests of Markdown rendering (threat 15, plan test 1).

use super::*;
use crate::test_support::{start_tags, text_of};

fn tags(markdown: &str) -> Vec<String> {
    start_tags(&to_html(markdown))
        .into_iter()
        .map(|(name, _)| name)
        .collect()
}

fn hrefs(markdown: &str) -> Vec<String> {
    start_tags(&to_html(markdown))
        .into_iter()
        .filter(|(name, _)| name == "a")
        .filter_map(|(_, attributes)| {
            attributes
                .into_iter()
                .find(|(name, _)| name == "href")
                .map(|(_, value)| value)
        })
        .collect()
}

#[test]
fn a_script_block_renders_as_escaped_text() {
    let html = to_html("<script>alert(1)</script>\n");
    assert!(!tags("<script>alert(1)</script>\n").contains(&"script".to_owned()));
    assert!(
        text_of(&html).contains("<script>alert(1)</script>"),
        "{html}"
    );
}

#[test]
fn inline_html_such_as_an_image_with_onerror_renders_as_escaped_text() {
    let markdown = "Look <img src=x onerror=alert(1)> here and <b>bold</b>.";
    assert_eq!(tags(markdown), vec!["p"]);
    assert!(text_of(&to_html(markdown)).contains("<img src=x onerror=alert(1)>"));
}

#[test]
fn a_javascript_link_keeps_its_text_and_loses_the_link() {
    let markdown = "[click](javascript:alert(1)) and [up](JavaScript:alert(1))";
    assert!(hrefs(markdown).is_empty());
    assert!(text_of(&to_html(markdown)).contains("click"));
}

#[test]
fn a_data_link_and_a_link_with_another_scheme_lose_the_link() {
    for markdown in [
        "[x](data:text/html,hi)",
        "[x](vbscript:msgbox)",
        "[x](file:///etc/passwd)",
        "[x](//evil.example/a)",
        "[x](<java\tscript:alert(1)>)",
    ] {
        assert!(hrefs(markdown).is_empty(), "{markdown}");
    }
}

#[test]
fn http_https_mailto_and_relative_links_keep_their_targets() {
    assert_eq!(
        hrefs(
            "[a](http://example.com/a) [b](https://example.com/b) [c](mailto:ana@example.com) [d](/pages/x) [e](notes.md#top) [f](#top)"
        ),
        vec![
            "http://example.com/a",
            "https://example.com/b",
            "mailto:ana@example.com",
            "/pages/x",
            "notes.md#top",
            "#top"
        ]
    );
}

#[test]
fn an_image_keeps_only_a_relative_source() {
    let html = to_html("![a chart](chart.svg) ![tracker](https://example.com/t.png)");
    let images: Vec<_> = start_tags(&html)
        .into_iter()
        .filter(|(name, _)| name == "img")
        .collect();
    assert_eq!(images.len(), 1, "{html}");
    assert!(
        images[0]
            .1
            .contains(&("src".to_owned(), "chart.svg".to_owned()))
    );
    assert!(
        images[0]
            .1
            .contains(&("alt".to_owned(), "a chart".to_owned()))
    );
    assert!(text_of(&html).contains("tracker"));
}

#[test]
fn mustache_tags_in_markdown_stay_text() {
    assert!(
        text_of(&to_html("Hello {{name}} and {{{raw}}}")).contains("Hello {{name}} and {{{raw}}}")
    );
}

#[test]
fn tables_strikethrough_and_task_lists_render() {
    let markdown = "| a | b |\n|---|--:|\n| 1 | 2 |\n\n~~gone~~\n\n- [x] done\n- [ ] open\n";
    let names = tags(markdown);
    for name in ["table", "thead", "th", "tbody", "td", "del", "ul", "li"] {
        assert!(names.contains(&name.to_owned()), "{name} in {names:?}");
    }
    assert!(!names.contains(&"input".to_owned()));
    let text = text_of(&to_html(markdown));
    assert!(
        text.contains('\u{2611}') && text.contains('\u{2610}'),
        "{text}"
    );
}

#[test]
fn headings_code_and_emphasis_render_as_their_elements() {
    let names = tags("# Title\n\nSome *em* and **strong** and `code`.\n\n```\n<pre>\n```\n");
    for name in ["h1", "p", "em", "strong", "code", "pre"] {
        assert!(names.contains(&name.to_owned()), "{name} in {names:?}");
    }
}

#[test]
fn a_link_title_is_escaped_into_its_attribute() {
    let html = to_html(r#"[a](/x "say \"hi\" <b>")"#);
    let anchors: Vec<_> = start_tags(&html)
        .into_iter()
        .filter(|(name, _)| name == "a")
        .collect();
    assert_eq!(anchors.len(), 1, "{html}");
    assert_eq!(anchors[0].1.len(), 2, "{html}");
}

#[test]
fn an_email_autolink_links_to_its_mailto_address() {
    assert_eq!(
        hrefs("<someone@example.com>"),
        vec!["mailto:someone@example.com".to_owned()]
    );
}
