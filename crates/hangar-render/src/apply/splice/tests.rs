//! Unit tests of splicing an edit into a record's text.

use super::*;

#[test]
fn a_key_written_with_an_escape_is_the_field_it_names() {
    let text = r#"{"title": "a", "n": 1}"#;
    let changed = splice(text, &[("title", Some("\"b\"".to_owned()))]);
    assert_eq!(changed.as_deref(), Some(r#"{"title": "b", "n": 1}"#));
}

#[test]
fn an_escaped_quote_in_an_untouched_value_stays_as_it_is() {
    let text = r#"{"n": "say \"hi\", then go", "title": "a"}"#;
    let changed = splice(text, &[("title", Some("\"b\"".to_owned()))]);
    assert_eq!(
        changed.as_deref(),
        Some(r#"{"n": "say \"hi\", then go", "title": "b"}"#)
    );
}
