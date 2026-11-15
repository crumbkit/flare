//! Unit tests of the conflict view (plan test 15).

use super::*;
use crate::test_support::{
    Tok, blob, record, slot, submit, target, task_schema, text_of, tokens, with,
};
use crate::{FormPost, Notice, Outcome, edit_form};
use secrecy::ExposeSecret;
use serde_json::{Value, json};

const BODY: &str = r#"{"title": "Artifact server", "status": "open", "body": "Prose"}"#;
const NEWER: &str = r#"{"title": "Artifact server", "status": "done", "body": "Prose and more"}"#;
const EDITABLE: &[&str] = &["title", "status"];

fn conflict(title: &str, newer: &str) -> Conflict {
    conflict_on("f.title", title, newer)
}

/// The conflict of an edit that sets `field` to `value` with the record
/// `newer`.
fn conflict_on(field: &str, value: &str, newer: &str) -> Conflict {
    let html = edit_form(
        &slot("task", EDITABLE, None),
        &record("/tasks/a.md", '1', BODY),
        &task_schema(),
        &target("tok-1"),
        Notice::Fresh,
    )
    .unwrap();
    let edit = FormPost::from_pairs(with(submit(html.as_str()), field, value))
        .unwrap()
        .into_edit(&slot("task", EDITABLE, None), &task_schema())
        .unwrap();
    match edit.apply(&record("/tasks/a.md", '2', newer)).unwrap() {
        Outcome::Conflict(conflict) => conflict,
        other => panic!("expected a conflict, got {other:?}"),
    }
}

fn view(conflict: &Conflict) -> String {
    conflict_view(
        conflict,
        &target("tok-2"),
        &LocalPath::try_from("/b/hangar-plan").unwrap(),
    )
    .into_string()
}

#[test]
fn the_view_shows_each_field_now_beside_the_edit() {
    let text = text_of(&view(&conflict("hangar: share pages", NEWER)));
    assert!(
        text.contains(
            "Your edit was not saved, because someone changed this task since you opened this form."
        ),
        "{text}"
    );
    assert!(text.contains("Nothing you typed is lost."));
    assert!(text.contains("hangar: share pages"));
    assert!(text.contains("done"));
    assert!(text.contains("was open"));
    assert!(text.contains("you did not change it"));
    assert!(text.contains("Fields this page does not edit stay as they are now."));
    assert!(text.contains("Save my edit"));
    assert!(text.contains("Discard my edit"));
    assert!(text.contains("The save is refused again if the task changes once more."));
}

#[test]
fn discard_is_a_link_and_save_is_a_post_to_the_targets_action() {
    let html = view(&conflict("x", NEWER));
    let mut saw_link = false;
    let mut saw_form = false;
    for token in tokens(&html) {
        if let Tok::Start { name, attributes } = token {
            let has =
                |key: &str, value: &str| attributes.iter().any(|(k, v)| k == key && v == value);
            saw_link |= name == "a" && has("href", "/b/hangar-plan");
            saw_form |=
                name == "form" && has("action", "/b/hangar-plan/edit") && has("method", "post");
        }
    }
    assert!(saw_link && saw_form, "{html}");
}

#[test]
fn saving_from_the_view_writes_the_edit_onto_the_newer_record() {
    let conflict = conflict("hangar: share pages", NEWER);
    let pairs = submit(&view(&conflict));
    let post = FormPost::from_pairs(pairs).unwrap();
    assert_eq!(post.token().expose_secret(), "tok-2");
    assert_eq!(post.based_on(), &blob('2'));
    let edit = post
        .into_edit(&slot("task", EDITABLE, None), &task_schema())
        .unwrap();
    let newer = record("/tasks/a.md", '2', NEWER);
    let Outcome::Write(write) = edit.apply(&newer).unwrap() else {
        panic!("expected a write");
    };
    assert_eq!(write.based_on(), &blob('2'));
    let body: Value = serde_json::from_str(write.body().as_str()).unwrap();
    assert_eq!(
        body,
        json!({"title": "hangar: share pages", "status": "done", "body": "Prose and more"})
    );
}

#[test]
fn a_save_from_the_view_is_refused_again_when_the_record_changed_once_more() {
    let conflict = conflict("hangar: share pages", NEWER);
    let edit = FormPost::from_pairs(submit(&view(&conflict)))
        .unwrap()
        .into_edit(&slot("task", EDITABLE, None), &task_schema())
        .unwrap();
    let newest = record("/tasks/a.md", '3', NEWER);
    assert!(matches!(edit.apply(&newest).unwrap(), Outcome::Conflict(_)));
}

#[test]
fn every_value_in_the_view_is_escaped() {
    let newer = r#"{"title": "<script>alert(1)</script>", "status": "<b>done</b>"}"#;
    let html = view(&conflict("\"><img src=x onerror=alert(1)>", newer));
    for token in tokens(&html) {
        if let Tok::Start { name, attributes } = token {
            assert!(!["script", "img"].contains(&name.as_str()), "{name}");
            for (key, _) in attributes {
                assert!(!key.starts_with("on") && key != "style", "{key}");
            }
        }
    }
    let text = text_of(&html);
    assert!(text.contains("<script>alert(1)</script>"));
    assert!(text.contains("<b>done</b>"));
}

/// The body that "Save my edit" writes onto `newer`.
fn saved_from_view(conflict: &Conflict, newer: &str) -> Value {
    let edit = FormPost::from_pairs(submit(&view(conflict)))
        .unwrap()
        .into_edit(&slot("task", EDITABLE, None), &task_schema())
        .unwrap();
    let Outcome::Write(write) = edit.apply(&record("/tasks/a.md", '2', newer)).unwrap() else {
        panic!("expected a write");
    };
    serde_json::from_str(write.body().as_str()).unwrap()
}

#[test]
fn an_unchanged_field_whose_newer_value_no_control_edits_stays_as_it_is_now() {
    let newer = r#"{"title": "Artifact server", "status": 5, "body": "Prose"}"#;
    let conflict = conflict("hangar: share pages", newer);
    assert_eq!(
        saved_from_view(&conflict, newer),
        json!({"title": "hangar: share pages", "status": 5, "body": "Prose"})
    );
}

#[test]
fn an_unchanged_field_whose_newer_value_is_over_the_size_limit_stays_as_it_is_now() {
    let long = "x".repeat(crate::form::MAX_VALUE_BYTES + 1);
    let newer = json!({"title": long, "status": "open", "body": "Prose"}).to_string();
    let conflict = conflict_on("f.status", "done", &newer);
    let html = view(&conflict);
    assert!(text_of(&html).contains(&long));
    assert_eq!(
        saved_from_view(&conflict, &newer),
        json!({"title": long, "status": "done", "body": "Prose"})
    );
}
