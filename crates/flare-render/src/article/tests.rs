//! Unit tests of the reading page's article.

use super::*;

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
