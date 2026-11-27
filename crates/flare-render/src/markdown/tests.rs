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

/// Whether the element `name` carries `attribute` with `value`.
fn has(
    elements: &[(String, Vec<(String, String)>)],
    name: &str,
    attribute: &str,
    value: &str,
) -> bool {
    elements.iter().any(|(element, attributes)| {
        element == name
            && attributes
                .iter()
                .any(|(key, found)| key == attribute && found == value)
    })
}

/// The figcaptions' text in `html`.
fn captions(html: &str) -> Vec<String> {
    let mut found = Vec::new();
    let mut inside = None::<String>;
    for token in crate::test_support::tokens(html) {
        match token {
            crate::test_support::Tok::Start { name, .. } if name == "figcaption" => {
                inside = Some(String::new());
            }
            crate::test_support::Tok::End { name } if name == "figcaption" => {
                found.extend(inside.take());
            }
            crate::test_support::Tok::Text(text) => {
                if let Some(caption) = inside.as_mut() {
                    caption.push_str(&text);
                }
            }
            _ => {}
        }
    }
    found
}

const FLOWCHART: &str = "```mermaid\nflowchart LR\n  a[Forecast] --> b{Held?}\n```\n";

#[test]
fn each_allowed_mermaid_type_becomes_a_figure_that_holds_its_source() {
    for kind in [
        "flowchart",
        "graph",
        "sequenceDiagram",
        "stateDiagram",
        "classDiagram",
        "erDiagram",
        "gantt",
        "pie",
    ] {
        let markdown = format!("```mermaid\n{kind}\n  a --> b\n```\n");
        let rendered = render(&markdown);
        let found = start_tags(&rendered.html);
        assert!(
            has(&found, "figure", "class", "diagram"),
            "{kind}: {}",
            rendered.html
        );
        assert!(
            has(&found, "pre", "data-diagram", kind),
            "{kind}: {}",
            rendered.html
        );
        assert!(!found.iter().any(|(name, _)| name == "code"), "{kind}");
        assert!(rendered.diagrams && !rendered.code, "{kind}");
        assert!(text_of(&rendered.html).contains("a --> b"), "{kind}");
        // Only the reading page draws a diagram.
        assert!(!rendered.html.contains("<svg"), "{kind}");
    }
}

#[test]
fn a_mermaid_fence_of_another_type_renders_as_code_and_its_twin_as_a_figure() {
    for (refused, admitted) in [
        ("journey\n  title x", "flowchart LR\n  title x"),
        (
            "%%{init: {'securityLevel': 'loose'}}%%\nflowchart LR\n  a --> b",
            "flowchart LR\n  a --> b",
        ),
        (
            "---\nconfig:\n  securityLevel: loose\n---\nflowchart LR",
            "flowchart LR",
        ),
        ("", "pie"),
        ("Flowchart LR", "flowchart LR"),
    ] {
        let code = render(&format!("```mermaid\n{refused}\n```\n"));
        assert!(code.code && !code.diagrams, "{refused}: {}", code.html);
        assert!(
            !has(&start_tags(&code.html), "figure", "class", "diagram"),
            "{refused}"
        );
        let figure = render(&format!("```mermaid\n{admitted}\n```\n"));
        assert!(
            figure.diagrams && !figure.code,
            "{admitted}: {}",
            figure.html
        );
    }
}

#[test]
fn a_flowchart_under_another_fence_language_renders_as_code() {
    let rendered = render("```text\nflowchart LR\n  a --> b\n```\n");
    assert!(rendered.code && !rendered.diagrams, "{}", rendered.html);
    assert!(render(FLOWCHART).diagrams);
}

#[test]
fn a_diagram_source_is_escaped() {
    let markdown = "```mermaid\nflowchart LR\n  a[\"<img src=x onerror=alert(1)>\"] --> b[</pre><script>alert(1)</script>]\n```\n";
    let html = to_html(markdown);
    let names = tags(markdown);
    assert!(!names.contains(&"img".to_owned()), "{html}");
    assert!(!names.contains(&"script".to_owned()), "{html}");
    assert!(
        text_of(&html).contains("<img src=x onerror=alert(1)>"),
        "{html}"
    );
    assert!(
        text_of(&html).contains("</pre><script>alert(1)</script>"),
        "{html}"
    );
}

#[test]
fn a_fenced_block_shows_its_language_label_and_a_copy_button() {
    let html = to_html("```rust\nfn main() {}\n```\n");
    let found = start_tags(&html);
    assert!(has(&found, "div", "class", "code"), "{html}");
    assert!(has(&found, "code", "data-lang", "rust"), "{html}");
    assert!(has(&found, "button", "data-copy", ""), "{html}");
    assert!(has(&found, "button", "type", "button"), "{html}");
    assert!(text_of(&html).contains("rust"), "{html}");
    assert!(text_of(&html).contains("Copy"), "{html}");
    assert!(text_of(&html).contains("fn main() {}"), "{html}");
}

#[test]
fn a_code_language_is_escaped_into_its_label_and_attribute() {
    let html = to_html("```\"><script>x\nbody\n```\n");
    assert!(!tags("```\"><script>x\nbody\n```\n").contains(&"script".to_owned()));
    let found = start_tags(&html);
    assert!(has(&found, "code", "data-lang", "\"><script>x"), "{html}");
}

#[test]
fn an_indented_block_and_an_unlabelled_fence_are_plain_text_code() {
    for markdown in ["    let x = 1;\n", "```\nlet x = 1;\n```\n"] {
        let rendered = render(markdown);
        assert!(rendered.code, "{markdown}");
        assert!(
            has(&start_tags(&rendered.html), "code", "data-lang", "text"),
            "{}",
            rendered.html
        );
    }
}

#[test]
fn a_page_with_no_code_or_diagram_says_so() {
    let rendered = render("# Title\n\nText with `inline code`.\n\n| a |\n|---|\n| 1 |\n");
    assert!(!rendered.code && !rendered.diagrams);
}

#[test]
fn a_table_scrolls_in_its_own_box() {
    let html = to_html("| a | b |\n|---|---|\n| 1 | 2 |\n");
    let found = start_tags(&html);
    let scroll = found.iter().position(|(name, attributes)| {
        name == "div" && attributes.contains(&("class".to_owned(), "scroll".to_owned()))
    });
    let table = found.iter().position(|(name, _)| name == "table");
    assert!(
        matches!((scroll, table), (Some(s), Some(t)) if s < t),
        "{html}"
    );
}

#[test]
fn a_figure_paragraph_after_a_diagram_becomes_its_caption() {
    let html = to_html(&format!(
        "{FLOWCHART}\nFigure 1. What happens to one sailing.\n"
    ));
    assert_eq!(
        captions(&html),
        vec!["Figure 1. What happens to one sailing."]
    );
    assert!(
        !tags(&format!("{FLOWCHART}\nFigure 1. x\n")).contains(&"p".to_owned()),
        "{html}"
    );
    let figures = start_tags(&html)
        .into_iter()
        .filter(|(name, _)| name == "figure")
        .count();
    assert_eq!(figures, 1, "{html}");
}

#[test]
fn a_figure_paragraph_after_a_table_becomes_its_caption() {
    let markdown = "| a |\n|---|\n| 1 |\n\nFigure 2. The vessels.\n";
    let html = to_html(markdown);
    assert_eq!(captions(&html), vec!["Figure 2. The vessels."]);
    assert!(
        has(&start_tags(&html), "figure", "class", "table"),
        "{html}"
    );
    assert!(!tags(markdown).contains(&"p".to_owned()), "{html}");
}

#[test]
fn a_figure_paragraph_after_a_code_block_becomes_its_caption() {
    let markdown = "```rust\nfn decide() {}\n```\n\nFigure 3. The rule in *code*.\n";
    let html = to_html(markdown);
    assert_eq!(captions(&html), vec!["Figure 3. The rule in code."]);
    assert!(
        has(&start_tags(&html), "figure", "class", "listing"),
        "{html}"
    );
    assert!(tags(markdown).contains(&"em".to_owned()), "{html}");
}

#[test]
fn a_figure_paragraph_after_another_paragraph_stays_a_paragraph() {
    let html = to_html("Some text.\n\nFigure 1. Not a caption.\n");
    assert!(captions(&html).is_empty(), "{html}");
    assert!(!tags("Some text.\n\nFigure 1. x\n").contains(&"figure".to_owned()));
}

#[test]
fn a_figure_paragraph_with_no_number_stays_a_paragraph() {
    for caption in [
        "Figure. The rule.",
        "Figure one. The rule.",
        "Figure 1 The rule.",
        "Figures 1. x",
        "Figure 1.5 x",
    ] {
        let html = to_html(&format!("```rust\nfn x() {{}}\n```\n\n{caption}\n"));
        assert!(captions(&html).is_empty(), "{caption}: {html}");
    }
    let html = to_html("```rust\nfn x() {}\n```\n\nFigure 12.\n");
    assert_eq!(captions(&html), vec!["Figure 12."]);
}

#[test]
fn a_figure_paragraph_after_a_blank_block_stays_a_paragraph() {
    for between in ["<!-- -->", ">", "---"] {
        let html = to_html(&format!(
            "```rust\nfn x() {{}}\n```\n\n{between}\n\nFigure 1. The rule.\n"
        ));
        assert!(captions(&html).is_empty(), "{between}: {html}");
    }
}

#[test]
fn a_second_figure_paragraph_after_a_captioned_block_stays_a_paragraph() {
    let markdown = format!("{FLOWCHART}\nFigure 1. First.\n\nFigure 2. Second.\n");
    let html = to_html(&markdown);
    assert_eq!(captions(&html), vec!["Figure 1. First."]);
    assert!(tags(&markdown).contains(&"p".to_owned()), "{html}");
    assert!(text_of(&html).contains("Figure 2. Second."));
}

#[test]
fn caption_text_is_escaped() {
    let html = to_html(&format!(
        "{FLOWCHART}\nFigure 1. <script>alert(1)</script> & more\n"
    ));
    assert_eq!(
        captions(&html),
        vec!["Figure 1. <script>alert(1)</script> & more"]
    );
    assert!(
        !tags(&format!("{FLOWCHART}\nFigure 1. <script>x</script>\n"))
            .contains(&"script".to_owned())
    );
}

#[test]
fn a_figure_paragraph_inside_a_list_after_a_nested_code_block_stays_a_paragraph() {
    let html = to_html("- item\n\n  ```rust\n  fn x() {}\n  ```\n\n  Figure 1. Nested.\n");
    assert!(captions(&html).is_empty(), "{html}");
}
