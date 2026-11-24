//! Unit tests of the reading page's article.

use super::*;
use crate::markdown::image_paths;

fn article(markdown: &str) -> Result<Article, RenderError> {
    super::article(markdown, &ImageSources::none())
}

#[test]
fn an_article_with_code_needs_the_script() {
    let article = article("```rust\nfn x() {}\n```\n").unwrap();
    assert!(article.has_code() && !article.has_diagrams() && article.needs_script());
    assert!(article.html().as_str().contains("data-lang=\"rust\""));
}

#[test]
fn an_article_with_a_diagram_needs_the_script() {
    let article = article("```mermaid\npie\n  \"a\" : 1\n```\n").unwrap();
    assert!(article.has_diagrams() && !article.has_code() && article.needs_script());
}

#[test]
fn an_article_of_prose_and_tables_needs_no_script() {
    let article = article("# Title\n\n| a |\n|---|\n| 1 |\n\nText.\n").unwrap();
    assert!(!article.needs_script());
}

#[test]
fn an_article_over_4_mib_is_refused() {
    let markdown = "<".repeat(MAX_OUTPUT_BYTES / 4 + 1);
    assert!(matches!(
        article(&markdown),
        Err(RenderError::TooLarge { .. })
    ));
}

#[test]
fn an_image_its_version_lists_shows_from_its_route_and_an_unlisted_one_shows_its_alt_text() {
    let sources = ImageSources::new([(
        "img/chart.png".to_owned(),
        "/api/shares/ab/versions/1/files/cd".to_owned(),
    )]);
    let markdown = "![The chart](img/chart.png) and ![Missing](img/gone.png) and ![Remote](https://x.example/a.png)\n";
    let html = super::article(markdown, &sources).unwrap();
    let html = html.html().as_str();
    assert!(
        html.contains("<img src=\"/api/shares/ab/versions/1/files/cd\" alt=\"The chart\">"),
        "{html}"
    );
    assert!(
        !html.contains("img/gone.png") && html.contains("Missing"),
        "{html}"
    );
    assert!(
        !html.contains("x.example") && html.contains("Remote"),
        "{html}"
    );
    assert_eq!(html.matches("<img").count(), 1, "{html}");
}

#[test]
fn an_article_with_no_listed_images_shows_every_image_as_its_alt_text() {
    let html = article("![A](a.png)\n").unwrap();
    assert!(
        !html.html().as_str().contains("<img"),
        "{}",
        html.html().as_str()
    );
    assert!(html.html().as_str().contains('A'));
}

#[test]
fn the_image_paths_of_a_document_are_its_relative_sources_in_order_each_once() {
    let markdown = "![a](img/a.png) ![b](https://x.example/b.png) ![c](c.gif)\n\n![a again](img/a.png) ![d](//host/d.png) ![e](data:image/png;base64,AA)\n";
    assert_eq!(image_paths(markdown), ["img/a.png", "c.gif"]);
    assert!(image_paths("No images.\n").is_empty());
}
