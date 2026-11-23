//! Unit tests of escaping, the markup builder and the stylesheet.

use super::*;

#[test]
fn text_escapes_the_five_characters_that_can_end_text_or_a_quoted_value() {
    assert_eq!(
        Html::text(r#"a&b<c>d"e'f"#).as_str(),
        "a&amp;b&lt;c&gt;d&quot;e&#39;f"
    );
}

#[test]
fn text_leaves_every_other_character_as_it_is() {
    let text = "plain text, `=/ é 漢字 {{x}}";
    assert_eq!(Html::text(text).as_str(), text);
}

#[test]
fn text_turns_nul_into_the_replacement_character() {
    assert_eq!(Html::text("a\0b").as_str(), "a\u{FFFD}b");
}

#[test]
fn the_builder_escapes_attribute_values_and_text() {
    let mut builder = Builder::default();
    builder.element("p", &[("title", "\" onclick=\"x")], "<b>");
    assert_eq!(
        builder.finish().as_str(),
        "<p title=\"&quot; onclick=&quot;x\">&lt;b&gt;</p>"
    );
}

#[test]
fn the_builder_adds_a_flag_to_the_element_it_opened_last() {
    let mut builder = Builder::default();
    builder.open("input", &[("type", "checkbox")]);
    builder.flag("checked");
    assert_eq!(
        builder.finish().as_str(),
        "<input type=\"checkbox\" checked>"
    );
}

#[test]
fn the_stylesheet_holds_no_less_than_sign_and_no_outside_url() {
    let css = stylesheet();
    assert!(!css.contains('<'));
    assert!(!css.contains("url("));
    assert!(!css.contains("@import"));
    assert!(!css.contains("//"));
}

#[test]
fn the_stylesheet_has_board_ui_tokens_for_light_and_dark() {
    let css = stylesheet();
    for token in ["--paper:", "--ink:", "--accent:", "--h-act:", "--h-bad:"] {
        assert!(css.contains(token), "{token}");
    }
    assert!(css.contains("prefers-color-scheme: dark"));
}

#[test]
fn the_stylesheet_styles_each_class_the_crate_writes() {
    let css = stylesheet();
    for class in [
        ".flare-form",
        ".fields",
        ".field",
        ".hint",
        ".btn",
        ".go",
        ".msg",
        ".readonly",
        ".conflict",
        ".cf",
        ".result",
        ".prose-board",
    ] {
        assert!(css.contains(class), "{class}");
    }
}

#[test]
fn the_stylesheet_has_the_mockup_tokens_for_light_and_dark() {
    let css = stylesheet();
    let (light, dark) = css
        .split_once("@media (prefers-color-scheme: dark)")
        .unwrap();
    for token in [
        "--canvas:",
        "--app:",
        "--surface-2:",
        "--card:",
        "--line:",
        "--fg:",
        "--muted:",
        "--accent:",
        "--accent-tint:",
        "--accent-ink:",
    ] {
        assert!(light.contains(token), "light {token}");
        assert!(dark.contains(token), "dark {token}");
    }
    assert!(light.contains("--fg: oklch(18% .015 240)"));
    assert!(dark.contains("--fg: oklch(93% .008 240)"));
}

#[test]
fn the_stylesheet_styles_the_reading_page_article() {
    let css = stylesheet();
    for class in [
        ".doc",
        ".scroll",
        ".code-head",
        ".text-btn",
        ".diagram-source",
        ".mm-light",
        ".mm-dark",
        "figcaption",
        "--shiki-dark",
    ] {
        assert!(css.contains(class), "{class}");
    }
}
