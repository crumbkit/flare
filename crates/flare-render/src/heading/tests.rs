//! Unit tests of [`first_heading`].

use super::*;

#[test]
fn the_first_level_one_heading_is_the_title() {
    assert_eq!(
        first_heading("# Quarterly plan\n\nSome text.\n\n# Later\n").as_deref(),
        Some("Quarterly plan")
    );
}

#[test]
fn a_document_with_no_level_one_heading_has_no_title() {
    assert_eq!(first_heading("Just text.\n"), None);
    assert_eq!(first_heading("## Background\n\nText.\n"), None);
}

#[test]
fn a_level_one_heading_after_a_level_two_heading_is_still_the_title() {
    assert_eq!(
        first_heading("## Background\n\n# The plan\n").as_deref(),
        Some("The plan")
    );
}

#[test]
fn a_first_level_one_heading_with_no_text_gives_no_title_and_one_with_text_does() {
    assert_eq!(first_heading("#\n"), None);
    assert_eq!(first_heading("#   \n\n# Named\n"), None);
    assert_eq!(first_heading("# ![only a picture](x.png)\n"), None);
    assert_eq!(first_heading("# Named\n\n#\n").as_deref(), Some("Named"));
}

#[test]
fn a_hash_line_inside_a_fenced_code_block_is_not_a_heading() {
    assert_eq!(
        first_heading("```\n# Not a title\n```\n\n# Real title\n").as_deref(),
        Some("Real title")
    );
    assert_eq!(first_heading("```sh\n# Not a title\n```\n"), None);
}

#[test]
fn a_hash_line_inside_an_html_block_is_not_a_heading() {
    assert_eq!(first_heading("<div>\n# Not a title\n</div>\n"), None);
}

#[test]
fn a_setext_heading_is_a_title() {
    assert_eq!(
        first_heading("Setext title\n============\n\nText.\n").as_deref(),
        Some("Setext title")
    );
}

#[test]
fn the_title_keeps_the_text_of_emphasis_code_and_links_without_their_markup() {
    assert_eq!(
        first_heading("# The *new* `api` [guide](x)\n").as_deref(),
        Some("The new api guide")
    );
}

#[test]
fn the_title_leaves_out_image_alternative_text() {
    assert_eq!(
        first_heading("# Logo ![a picture](logo.png) plan\n").as_deref(),
        Some("Logo plan")
    );
}

#[test]
fn runs_of_whitespace_become_one_space_and_the_ends_are_trimmed() {
    assert_eq!(
        first_heading("#   Wide \t  gaps   here  \n").as_deref(),
        Some("Wide gaps here")
    );
    assert_eq!(
        first_heading("Two\nlines\n===\n").as_deref(),
        Some("Two lines")
    );
}

#[test]
fn control_characters_are_removed_from_the_title() {
    assert_eq!(
        first_heading("# Bell\u{7}ed\u{1b} title\n").as_deref(),
        Some("Belled title")
    );
}

#[test]
fn a_long_heading_is_cut_at_a_character_boundary_within_the_title_limit() {
    let heading = "é".repeat(150);
    assert_eq!(heading.len(), 300);
    let title = first_heading(&format!("# {heading}\n")).unwrap();
    assert!(title.len() <= MAX_TITLE, "{}", title.len());
    assert_eq!(title, "é".repeat(100));
    let three = "€".repeat(100);
    let title = first_heading(&format!("# {three}\n")).unwrap();
    assert_eq!(title, "€".repeat(66));
    assert_eq!(title.len(), 198);
}

#[test]
fn a_heading_within_the_title_limit_is_kept_whole() {
    let heading = "a".repeat(MAX_TITLE);
    assert_eq!(first_heading(&format!("# {heading}\n")), Some(heading));
}
