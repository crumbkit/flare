//! Unit tests of applying an edit (plan tests 13, 14 and 16).

use super::*;
use crate::test_support::{blob, record, slot, submit, target, task_schema, with};
use crate::{FormPost, Notice, edit_form};
use hangar_core::{BindingName, PrincipalId, RecordType};
use serde_json::json;

const BODY: &str = r#"{"$type": "tasks/task/v2", "title": "Ship it", "status": "open", "notes": "keep", "body": "Line one\nLine two", "estimate": 3, "weight": 1.50, "owner": {"person": "ana", "list": [1, 2.0, "x"]}, "tags": ["a"]}"#;
const EDITABLE: &[&str] = &["title", "status", "notes", "body", "estimate"];

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

fn edit(pairs: Vec<(String, String)>) -> Edit {
    FormPost::from_pairs(pairs)
        .unwrap()
        .into_edit(&slot("task", EDITABLE, None), &task_schema())
        .unwrap()
}

fn write(outcome: Outcome) -> RecordWrite {
    match outcome {
        Outcome::Write(write) => write,
        other => panic!("expected a write, got {other:?}"),
    }
}

fn fields(body: &RecordBody) -> serde_json::Map<String, Value> {
    match serde_json::from_str(body.as_str()).unwrap() {
        Value::Object(fields) => fields,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn a_changed_field_is_written_and_every_other_field_stays_as_it_was_parsed() {
    let current = record("/tasks/a.md", '1', BODY);
    let write = write(
        edit(with(drawn(), "f.title", "Ship it today"))
            .apply(&current)
            .unwrap(),
    );
    let before = fields(current.body());
    let after = fields(write.body());
    assert_eq!(after.get("title"), Some(&json!("Ship it today")));
    assert_eq!(after.len(), before.len());
    for (key, value) in &before {
        if key != "title" {
            assert_eq!(after.get(key), Some(value), "{key}");
        }
    }
    assert_eq!(write.path().as_str(), "/tasks/a.md");
    assert_eq!(write.based_on(), &blob('1'));
    assert_eq!(write.through().as_str(), "hangar-plan");
}

/// A body whose untouched fields a parse and a rewrite would change: keys
/// out of order, an integer beyond u64, numbers in exponent and trailing
/// zero spellings, an escaped string and uneven spacing.
const ODD: &str = "{\n  \"$type\": \"tasks/task/v2\",\n  \"zeta\":   123456789012345678901234567890,\n  \"title\": \"Ship it\",\n  \"e\": 1e2,\n  \"status\" : \"open\",\n  \"weight\": 1.50,\n  \"esc\": \"\\u00e9\\/\",\n  \"notes\": \"keep\",\n  \"alpha\": {\"b\": 1, \"a\": [1.0, 2]}\n}\n";

/// The write that setting `field` to `value` in `ODD` makes.
fn odd_write(field: &str, value: &str) -> String {
    let editable = &["title", "notes", "body"];
    let html = edit_form(
        &slot("task", editable, None),
        &record("/tasks/a.md", '1', ODD),
        &task_schema(),
        &target("tok-1"),
        Notice::Fresh,
    )
    .unwrap();
    let edit = FormPost::from_pairs(with(submit(html.as_str()), field, value))
        .unwrap()
        .into_edit(&slot("task", editable, None), &task_schema())
        .unwrap();
    write(edit.apply(&record("/tasks/a.md", '1', ODD)).unwrap())
        .body()
        .as_str()
        .to_owned()
}

#[test]
fn a_save_keeps_every_byte_of_the_fields_the_person_did_not_touch() {
    assert_eq!(
        odd_write("f.title", "Ship it \"today\""),
        ODD.replace("\"Ship it\"", "\"Ship it \\\"today\\\"\"")
    );
}

#[test]
fn an_emptied_field_is_removed_and_every_other_byte_stays() {
    assert_eq!(
        odd_write("f.notes", ""),
        ODD.replace(",\n  \"notes\": \"keep\"", "")
    );
}

#[test]
fn a_field_the_record_lacks_is_added_after_the_others_and_every_other_byte_stays() {
    assert_eq!(
        odd_write("f.body", "New prose"),
        ODD.replace("[1.0, 2]}\n}", "[1.0, 2]},\n  \"body\": \"New prose\"\n}")
    );
}

#[test]
fn removing_the_last_field_keeps_the_others_and_their_separators() {
    let body = r#"{"title": "Ship it", "status": "open", "notes": "keep"}"#;
    let html = edit_form(
        &slot("task", &["notes"], None),
        &record("/tasks/a.md", '1', body),
        &task_schema(),
        &target("tok-1"),
        Notice::Fresh,
    )
    .unwrap();
    let edit = FormPost::from_pairs(with(submit(html.as_str()), "f.notes", ""))
        .unwrap()
        .into_edit(&slot("task", &["notes"], None), &task_schema())
        .unwrap();
    let written = write(edit.apply(&record("/tasks/a.md", '1', body)).unwrap());
    assert_eq!(
        written.body().as_str(),
        r#"{"title": "Ship it", "status": "open"}"#
    );
}

#[test]
fn a_post_that_changes_nothing_is_unchanged() {
    let current = record("/tasks/a.md", '1', BODY);
    assert!(matches!(
        edit(drawn()).apply(&current).unwrap(),
        Outcome::Unchanged
    ));
}

#[test]
fn an_emptied_optional_field_is_removed() {
    let current = record("/tasks/a.md", '1', BODY);
    let write = write(edit(with(drawn(), "f.notes", "")).apply(&current).unwrap());
    assert!(!fields(write.body()).contains_key("notes"));
}

#[test]
fn an_emptied_required_text_field_is_written_as_an_empty_string() {
    let current = record("/tasks/a.md", '1', BODY);
    let write = write(edit(with(drawn(), "f.title", "")).apply(&current).unwrap());
    assert_eq!(fields(write.body()).get("title"), Some(&json!("")));
}

#[test]
fn crlf_line_endings_from_a_textarea_do_not_count_as_a_change() {
    let current = record("/tasks/a.md", '1', BODY);
    let outcome = edit(with(drawn(), "f.body", "Line one\r\nLine two")).apply(&current);
    assert!(matches!(outcome.unwrap(), Outcome::Unchanged));
}

#[test]
fn a_changed_textarea_value_is_written_with_newline_line_endings() {
    let current = record("/tasks/a.md", '1', BODY);
    let write = write(
        edit(with(drawn(), "f.body", "One\r\nTwo\rThree"))
            .apply(&current)
            .unwrap(),
    );
    assert_eq!(
        fields(write.body()).get("body"),
        Some(&json!("One\nTwo\nThree"))
    );
}

#[test]
fn a_record_with_another_blob_gives_a_conflict_and_no_write() {
    let newer = record(
        "/tasks/a.md",
        '2',
        &BODY.replace("\"status\": \"open\"", "\"status\": \"done\""),
    );
    let outcome = edit(with(drawn(), "f.title", "Ship it today"))
        .apply(&newer)
        .unwrap();
    let Outcome::Conflict(conflict) = outcome else {
        panic!("expected a conflict, got {outcome:?}");
    };
    assert_eq!(conflict.newer(), &blob('2'));
    assert_eq!(conflict.path().as_str(), "/tasks/a.md");
}

#[test]
fn a_conflict_whose_edit_the_newer_record_already_holds_is_unchanged() {
    let newer = record(
        "/tasks/a.md",
        '2',
        &BODY.replace("\"Ship it\"", "\"Ship it today\""),
    );
    let outcome = edit(with(drawn(), "f.title", "Ship it today"))
        .apply(&newer)
        .unwrap();
    assert!(matches!(outcome, Outcome::Unchanged), "{outcome:?}");
}

#[test]
fn an_edit_that_changes_nothing_is_unchanged_against_a_newer_record() {
    let newer = record("/tasks/a.md", '2', &BODY.replace("open", "done"));
    assert!(matches!(
        edit(drawn()).apply(&newer).unwrap(),
        Outcome::Unchanged
    ));
}

#[test]
fn a_record_that_is_not_an_object_or_of_another_type_is_refused() {
    let edit = edit(with(drawn(), "f.title", "x"));
    assert!(matches!(
        edit.apply(&record("/tasks/a.md", '1', "[]")),
        Err(ApplyError::NotAnObject { .. })
    ));
    let other = Record::new(
        RecordPath::try_from("/notes/a").unwrap(),
        RecordType::try_from("notes/note/v1").unwrap(),
        blob('1'),
        RecordBody::try_from(BODY).unwrap(),
    );
    assert!(matches!(
        edit.apply(&other),
        Err(ApplyError::WrongType { .. })
    ));
}

#[test]
fn the_provenance_of_a_write_carries_the_blob_and_the_binding_the_post_named() {
    let current = record("/tasks/a.md", '1', BODY);
    let write = write(edit(with(drawn(), "f.title", "x")).apply(&current).unwrap());
    let ana = Principal::new(PrincipalId::try_from("ana@example.com").unwrap());
    let provenance = write.provenance(ana.clone());
    assert_eq!(provenance.based_on(), &blob('1'));
    assert_eq!(
        provenance.through(),
        &BindingName::try_from("hangar-plan").unwrap()
    );
    assert_eq!(provenance.made_by(), &ana);
}
