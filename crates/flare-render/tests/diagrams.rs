//! The reading page's diagrams: the server draws each one into its figure
//! as SVG, a diagram it cannot draw keeps its source and a message, and a
//! page whose only blocks are diagrams needs no script.
#![cfg(test)]

use flare_render::{ImageSources, article};

/// The reading page's HTML for `markdown`.
fn read(markdown: &str) -> flare_render::Article {
    article(markdown, &ImageSources::none()).unwrap()
}

/// `source` in a Mermaid fence.
fn fence(source: &str) -> String {
    format!("```mermaid\n{source}```\n")
}

const FIGURE_1: &str = include_str!("diagrams/01-figure-1.mmd");

#[test]
fn a_diagram_is_drawn_into_its_figure_with_its_caption_as_its_label() {
    let markdown = format!(
        "{}\nFigure 1. What happens to one sailing when a forecast arrives.\n",
        fence(FIGURE_1)
    );
    let page = read(&markdown);
    let html = page.html().as_str();
    assert!(
        html.starts_with("<figure class=\"diagram\" data-diagram=\"drawn\">\n<div class=\"scroll\" tabindex=\"0\">\n<div class=\"dg\" role=\"img\" aria-label=\"Figure 1. What happens to one sailing when a forecast arrives.\" style=\"min-width: 816px\"><svg "),
        "{html}"
    );
    assert!(
        html.ends_with("</svg></div>\n</div>\n<figcaption>Figure 1. What happens to one sailing when a forecast arrives.</figcaption>\n</figure>\n"),
        "{html}"
    );
    assert!(
        html.contains(include_str!("diagrams/01-figure-1.svg")),
        "{html}"
    );
    assert!(!html.contains("diagram-source"), "{html}");
}

#[test]
fn a_diagram_without_a_caption_is_labelled_by_its_type() {
    let page = read(&fence("pie\n  \"a\" : 1\n"));
    assert!(
        page.html()
            .as_str()
            .contains("<div class=\"dg\" role=\"img\" aria-label=\"pie diagram\""),
        "{}",
        page.html().as_str()
    );
}

#[test]
fn a_broken_diagram_keeps_its_source_and_limus_message_and_the_next_one_draws() {
    let markdown = format!(
        "{}\n{}",
        fence("flowchart LR\n  a --> --> b\n  ((\n"),
        fence("graph LR\n  a --> b\n")
    );
    let page = read(&markdown);
    let html = page.html().as_str();
    let (broken, next) = html.split_once("</figure>\n").unwrap();
    assert!(
        broken.starts_with("<figure class=\"diagram\" data-diagram=\"failed\">\n<pre class=\"diagram-source\" data-diagram=\"flowchart\">flowchart LR\n  a --&gt; --&gt; b\n  ((\n</pre>\n<p class=\"diagram-error\">Flare could not draw this diagram: "),
        "{broken}"
    );
    assert!(!broken.contains("<svg"), "{broken}");
    assert!(
        next.starts_with("<figure class=\"diagram\" data-diagram=\"drawn\">"),
        "{next}"
    );
    assert!(next.contains("<svg "), "{next}");
}

/// A flowchart of exactly `bytes` bytes, padded with a comment line.
fn flowchart_of(bytes: usize) -> String {
    let head = "flowchart LR\n  a --> b\n%% ";
    let mut source = String::from(head);
    source.push_str(&"x".repeat(bytes - head.len() - 1));
    source.push('\n');
    source
}

#[test]
fn a_source_over_50_000_bytes_shows_as_source_with_a_note_and_one_of_50_000_draws() {
    let long = read(&fence(&flowchart_of(50_001)));
    let html = long.html().as_str();
    assert!(
        html.starts_with("<figure class=\"diagram\" data-diagram=\"too-long\">\n<pre class=\"diagram-source\" data-diagram=\"flowchart\">flowchart LR"),
        "{}",
        html.get(..200).unwrap_or(html)
    );
    assert!(
        html.ends_with("</pre>\n<p class=\"diagram-error\">This diagram is too long to draw: Flare draws a diagram whose source is at most 50,000 bytes.</p>\n</figure>\n"),
        "{}",
        html.get(html.len() - 200..).unwrap_or(html)
    );
    let fits = read(&fence(&flowchart_of(50_000)));
    assert!(
        fits.html()
            .as_str()
            .starts_with("<figure class=\"diagram\" data-diagram=\"drawn\">"),
    );
}

#[test]
fn a_hostile_diagram_label_adds_no_element_or_handler_to_the_page() {
    let page = read(&fence(
        "flowchart LR\n  a[\"<img src=x onerror=alert(1)>\"] --> b[\"</pre><script>alert(1)</script>\"]\n",
    ));
    let html = page.html().as_str();
    for refused in [
        "<img",
        "<script",
        "<foreignObject",
        "href",
        "onerror=alert(1)>",
    ] {
        assert!(!html.contains(refused), "{refused}: {html}");
    }
    assert!(html.contains("data-diagram=\"drawn\""), "{html}");
}

#[test]
fn a_page_whose_only_blocks_are_diagrams_needs_no_script() {
    let page = read(&format!(
        "# Title\n\n{}\nFigure 1. x\n\n{}",
        fence(FIGURE_1),
        fence("pie\n  \"a\" : 1\n")
    ));
    assert!(page.has_diagrams() && !page.has_code() && !page.needs_script());
    let code = read(&format!("{}\n```rust\nfn x() {{}}\n```\n", fence(FIGURE_1)));
    assert!(code.has_diagrams() && code.has_code() && code.needs_script());
}
