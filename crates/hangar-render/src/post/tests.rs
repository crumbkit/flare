//! Unit tests of reading and checking a form post (plan test 12).

use super::*;
use crate::test_support::{
    field, record, slot, slot_name, submit, target, task_schema, with, without,
};
use crate::{Notice, edit_form};
use secrecy::ExposeSecret;

const BODY: &str = r#"{"title": "Ship it", "status": "open", "flagged": true, "estimate": 3, "due": "2026-11-20", "tags": ["a"]}"#;
const EDITABLE: &[&str] = &["title", "status", "flagged", "estimate", "due"];

/// The pairs a browser posts from the form drawn for `BODY`.
fn drawn() -> Vec<(String, String)> {
    let html = edit_form(
        &slot("task", EDITABLE, None),
        &record("/tasks/a.md", '1', BODY),
        &task_schema(),
        &target("tok-1"),
        Notice::Fresh,
    )
    .unwrap();
    submit(html.as_str())
}

fn edit(pairs: Vec<(String, String)>) -> Result<Edit, PostError> {
    FormPost::from_pairs(pairs)?.into_edit(&slot("task", EDITABLE, None), &task_schema())
}

fn entered(edit: &Edit, name: &str) -> Option<Value> {
    edit.fields
        .iter()
        .find(|entry| entry.name.as_str() == name)
        .unwrap()
        .entered
        .clone()
}

#[test]
fn a_post_of_the_form_as_drawn_reads_back_its_claims() {
    let post = FormPost::from_pairs(drawn()).unwrap();
    assert_eq!(post.token().expose_secret(), "tok-1");
    assert_eq!(post.based_on().as_str(), "1".repeat(40));
    assert_eq!(post.through().as_str(), "flare-plan");
    assert_eq!(post.slot(), &slot_name("task"));
    assert!(edit(drawn()).is_ok());
}

#[test]
fn each_missing_hidden_input_is_refused() {
    for key in ["token", "based_on", "through", "slot", "base"] {
        assert_eq!(
            FormPost::from_pairs(without(drawn(), key)).unwrap_err(),
            PostError::Missing { key },
            "{key}"
        );
    }
}

#[test]
fn an_unknown_key_is_refused_and_the_form_as_drawn_is_admitted() {
    let mut pairs = drawn();
    pairs.push(("path".to_owned(), "/tasks/other.md".to_owned()));
    assert_eq!(
        FormPost::from_pairs(pairs).unwrap_err(),
        PostError::Unknown {
            key: "path".to_owned()
        }
    );
    assert!(FormPost::from_pairs(drawn()).is_ok());
}

#[test]
fn a_repeated_key_is_refused_and_the_single_key_is_admitted() {
    let mut pairs = drawn();
    pairs.push(("f.title".to_owned(), "Again".to_owned()));
    assert_eq!(
        FormPost::from_pairs(pairs).unwrap_err(),
        PostError::Repeated {
            key: "f.title".to_owned()
        }
    );
    let mut pairs = drawn();
    pairs.push(("token".to_owned(), "tok-2".to_owned()));
    assert!(matches!(
        FormPost::from_pairs(pairs),
        Err(PostError::Repeated { .. })
    ));
    assert!(FormPost::from_pairs(with(drawn(), "f.title", "Again")).is_ok());
}

#[test]
fn malformed_hidden_inputs_are_refused() {
    assert_eq!(
        FormPost::from_pairs(with(drawn(), "based_on", "HEAD")).unwrap_err(),
        PostError::Malformed { key: "based_on" }
    );
    assert_eq!(
        FormPost::from_pairs(with(drawn(), "base", "[1]")).unwrap_err(),
        PostError::Malformed { key: "base" }
    );
    assert_eq!(
        FormPost::from_pairs(with(drawn(), "token", "")).unwrap_err(),
        PostError::Malformed { key: "token" }
    );
    assert!(matches!(
        FormPost::from_pairs(with(drawn(), "slot", "Bad Slot")),
        Err(PostError::Name(_))
    ));
    assert!(matches!(
        FormPost::from_pairs(with(drawn(), "through", "../x")),
        Err(PostError::Name(_))
    ));
    assert!(matches!(
        FormPost::from_pairs(with(drawn(), "base", r#"{"$type": "x"}"#)),
        Err(PostError::Name(_))
    ));
}

#[test]
fn a_post_larger_than_any_form_is_refused() {
    let long = "x".repeat(crate::form::MAX_VALUE_BYTES + 1);
    assert_eq!(
        FormPost::from_pairs(with(drawn(), "f.title", &long)).unwrap_err(),
        PostError::TooLarge
    );
    let mut pairs = drawn();
    for index in 0..64 {
        pairs.push((format!("f.extra{index}"), String::new()));
    }
    assert_eq!(
        FormPost::from_pairs(pairs).unwrap_err(),
        PostError::TooLarge
    );
}

/// The pairs a browser posts from the form drawn for a record whose body
/// field holds `prose`.
fn drawn_with_prose(prose: &str) -> Vec<(String, String)> {
    let body = serde_json::json!({"title": "Ship it", "status": "open", "body": prose}).to_string();
    let html = edit_form(
        &slot("task", &["title", "body"], None),
        &record("/tasks/a.md", '1', &body),
        &task_schema(),
        &target("tok-1"),
        Notice::Fresh,
    )
    .unwrap();
    submit(html.as_str())
}

#[test]
fn a_textarea_at_the_size_limit_posts_back_although_the_browser_sends_crlf() {
    let line = "x".repeat(12) + "\n";
    let mut prose = line.repeat(crate::form::MAX_VALUE_BYTES / line.len());
    prose.push_str(&"y".repeat(crate::form::MAX_VALUE_BYTES - prose.len()));
    assert_eq!(prose.len(), crate::form::MAX_VALUE_BYTES);
    let pairs = with(drawn_with_prose(&prose), "f.title", "Ship it today");
    let posted = &pairs.iter().find(|(key, _)| key == "f.body").unwrap().1;
    assert!(posted.len() > crate::form::MAX_VALUE_BYTES);
    let edit = FormPost::from_pairs(pairs)
        .unwrap()
        .into_edit(&slot("task", &["title", "body"], None), &task_schema())
        .unwrap();
    assert_eq!(
        edit.fields
            .iter()
            .find(|field| field.name.as_str() == "body")
            .unwrap()
            .entered,
        Some(Value::String(prose))
    );
}

#[test]
fn an_entry_over_the_size_limit_with_lf_line_endings_is_refused() {
    let over = crate::test_support::crlf(&("x\n".repeat(crate::form::MAX_VALUE_BYTES / 2) + "y"));
    assert_eq!(
        FormPost::from_pairs(with(drawn(), "f.title", &over)).unwrap_err(),
        PostError::TooLarge
    );
}

#[test]
fn a_post_for_another_slot_is_refused() {
    let result = FormPost::from_pairs(with(drawn(), "slot", "other"))
        .unwrap()
        .into_edit(&slot("task", EDITABLE, None), &task_schema());
    assert_eq!(
        result.unwrap_err(),
        PostError::WrongSlot {
            expected: slot_name("task"),
            found: slot_name("other")
        }
    );
}

#[test]
fn a_field_the_form_did_not_draw_is_refused_and_a_drawn_one_is_admitted() {
    assert_eq!(
        edit(with(drawn(), "f.notes", "sneaky")).unwrap_err(),
        PostError::NotDrawn {
            field: field("notes")
        }
    );
    assert!(edit(with(drawn(), "f.title", "fine")).is_ok());
}

#[test]
fn a_field_the_slot_does_not_list_is_refused_even_when_base_claims_it() {
    let base = r#"{"title": "Ship it", "status": "open", "flagged": true, "estimate": 3, "due": "2026-11-20", "notes": null}"#;
    let pairs = with(with(drawn(), "base", base), "f.notes", "sneaky");
    assert_eq!(
        edit(pairs).unwrap_err(),
        PostError::NotEditable {
            field: field("notes")
        }
    );
}

#[test]
fn a_field_whose_schema_shape_no_form_edits_is_refused_even_when_listed() {
    let editable = &["title", "tags"];
    let base = r#"{"title": "Ship it", "tags": ["a"]}"#;
    let pairs = with(with(drawn(), "base", base), "f.tags", "b");
    let pairs = without(
        without(
            without(without(pairs, "f.status"), "f.flagged"),
            "f.estimate",
        ),
        "f.due",
    );
    let result = FormPost::from_pairs(pairs)
        .unwrap()
        .into_edit(&slot("task", editable, None), &task_schema());
    assert_eq!(
        result.unwrap_err(),
        PostError::Unsupported {
            field: field("tags")
        }
    );
}

#[test]
fn a_bad_number_is_refused_and_a_whole_number_is_admitted() {
    for bad in ["lots", "1.5", "1e400", "0x10"] {
        assert!(
            matches!(
                edit(with(drawn(), "f.estimate", bad)),
                Err(PostError::BadValue {
                    expected: "a whole number",
                    ..
                })
            ),
            "{bad:?}"
        );
    }
    let edit = edit(with(drawn(), "f.estimate", " 4 ")).unwrap();
    assert_eq!(entered(&edit, "estimate"), Some(serde_json::json!(4)));
}

#[test]
fn a_bad_date_choice_or_flag_is_refused() {
    assert!(matches!(
        edit(with(drawn(), "f.due", "tomorrow")),
        Err(PostError::BadValue {
            expected: "a date such as 2026-11-20",
            ..
        })
    ));
    assert!(matches!(
        edit(with(drawn(), "f.due", "2026-02-30")),
        Err(PostError::BadValue { .. })
    ));
    assert!(matches!(
        edit(with(drawn(), "f.status", "abandoned")),
        Err(PostError::BadValue {
            expected: "one of the listed choices",
            ..
        })
    ));
    assert!(matches!(
        edit(with(drawn(), "f.flagged", "yes")),
        Err(PostError::BadValue { .. })
    ));
    assert!(edit(with(drawn(), "f.status", "done")).is_ok());
}

#[test]
fn an_unchecked_checkbox_reads_as_false() {
    let edit = edit(without(drawn(), "f.flagged")).unwrap();
    assert_eq!(entered(&edit, "flagged"), Some(Value::Bool(false)));
}

#[test]
fn an_empty_entry_for_an_optional_field_reads_as_no_value() {
    let edit = edit(with(drawn(), "f.due", "")).unwrap();
    assert_eq!(entered(&edit, "due"), None);
}

#[test]
fn a_missing_entry_for_a_drawn_field_other_than_a_checkbox_is_refused() {
    assert!(matches!(
        edit(without(drawn(), "f.title")),
        Err(PostError::MissingField { field: missing }) if missing == field("title")
    ));
}

#[test]
fn a_form_tokens_debug_output_hides_the_token() {
    let token = crate::FormToken::new("s3cret-token");
    assert!(!format!("{token:?}").contains("s3cret"));
    let post = FormPost::from_pairs(with(drawn(), "token", "s3cret-token")).unwrap();
    assert!(!format!("{post:?}").contains("s3cret"));
}
