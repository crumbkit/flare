//! Unit tests of the edit form (decision 13ee, plan test 11).

use super::*;
use crate::test_support::{
    TASK, Tok, record, slot, start_tags, submit, target, task_schema, text_of, tokens,
};
use crate::{FormPost, Schema};
use hangar_core::{RecordBody, RecordPath, RecordType};

const BODY: &str = r#"{"$type": "tasks/task/v2", "title": "Ship it", "status": "open", "body": "Long\nprose", "flagged": true, "estimate": 3, "weight": 1.5, "due": "2026-11-20", "tags": ["a"]}"#;

fn form(editable: &[&str], body: &str) -> String {
    edit_form(
        &slot("task", editable, None),
        &record("/tasks/a.md", '1', body),
        &task_schema(),
        &target("tok-1"),
        Notice::Fresh,
    )
    .unwrap()
    .into_string()
}

fn attribute<'a>(attributes: &'a [(String, String)], name: &str) -> Option<&'a str> {
    attributes
        .iter()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.as_str())
}

/// The start tag of the control named `name`.
fn control(html: &str, name: &str) -> (String, Vec<(String, String)>) {
    start_tags(html)
        .into_iter()
        .find(|(_, attributes)| attribute(attributes, "name") == Some(name))
        .unwrap_or_else(|| panic!("no control {name} in {html}"))
}

fn control_names(html: &str) -> Vec<String> {
    start_tags(html)
        .into_iter()
        .filter_map(|(_, attributes)| attribute(&attributes, "name").map(str::to_owned))
        .collect()
}

#[test]
fn the_form_draws_only_the_editable_fields_in_the_slots_order() {
    let html = form(&["status", "title"], BODY);
    let names: Vec<String> = control_names(&html)
        .into_iter()
        .filter(|name| name.starts_with("f."))
        .collect();
    assert_eq!(names, vec!["f.status", "f.title"]);
}

#[test]
fn the_form_posts_to_the_targets_action() {
    let html = form(&["title"], BODY);
    let (name, attributes) = &start_tags(&html)[0];
    assert_eq!(name, "form");
    assert_eq!(attribute(attributes, "method"), Some("post"));
    assert_eq!(attribute(attributes, "action"), Some("/b/hangar-plan/edit"));
}

#[test]
fn the_hidden_inputs_carry_the_token_blob_binding_slot_and_base() {
    let html = form(&["title", "status", "notes"], BODY);
    let hidden = |name: &str| {
        let (_, attributes) = control(&html, name);
        assert_eq!(attribute(&attributes, "type"), Some("hidden"), "{name}");
        attribute(&attributes, "value").unwrap().to_owned()
    };
    assert_eq!(hidden("token"), "tok-1");
    assert_eq!(hidden("based_on"), "1".repeat(40));
    assert_eq!(hidden("through"), "hangar-plan");
    assert_eq!(hidden("slot"), "task");
    let base: serde_json::Value = serde_json::from_str(&hidden("base")).unwrap();
    assert_eq!(
        base,
        serde_json::json!({"title": "Ship it", "status": "open", "notes": null})
    );
}

#[test]
fn each_kind_gets_its_control() {
    let html = form(
        &[
            "title", "body", "due", "status", "flagged", "estimate", "weight",
        ],
        BODY,
    );
    let (name, attributes) = control(&html, "f.title");
    assert_eq!(
        (name.as_str(), attribute(&attributes, "type")),
        ("input", Some("text"))
    );
    assert_eq!(attribute(&attributes, "value"), Some("Ship it"));
    assert_eq!(control(&html, "f.body").0, "textarea");
    assert_eq!(attribute(&control(&html, "f.due").1, "type"), Some("date"));
    assert_eq!(control(&html, "f.status").0, "select");
    let (_, flagged) = control(&html, "f.flagged");
    assert_eq!(attribute(&flagged, "type"), Some("checkbox"));
    assert!(attribute(&flagged, "checked").is_some());
    let (_, estimate) = control(&html, "f.estimate");
    assert_eq!(attribute(&estimate, "type"), Some("number"));
    assert_eq!(attribute(&estimate, "step"), Some("1"));
    assert_eq!(attribute(&estimate, "min"), Some("0"));
    assert_eq!(attribute(&estimate, "max"), Some("100"));
    assert_eq!(attribute(&estimate, "value"), Some("3"));
    assert_eq!(
        attribute(&control(&html, "f.weight").1, "step"),
        Some("0.5")
    );
}

#[test]
fn a_text_field_whose_value_holds_a_newline_gets_a_textarea_that_keeps_a_leading_newline() {
    let html = form(
        &["title"],
        r#"{"title": "\nLine one\nLine two", "status": "open"}"#,
    );
    assert_eq!(control(&html, "f.title").0, "textarea");
    let pairs = submit(&html);
    let title = pairs.iter().find(|(name, _)| name == "f.title").unwrap();
    assert_eq!(title.1, "\r\nLine one\r\nLine two");
}

#[test]
fn a_select_marks_the_current_choice_and_an_optional_one_offers_an_empty_choice() {
    let schema = Schema::parse(
        r#"{"required": ["status"], "properties": {"status": {"enum": ["open", "done"]}, "size": {"enum": ["s", "m"]}}}"#,
    )
    .unwrap();
    let html = edit_form(
        &slot("task", &["status", "size"], None),
        &record("/tasks/a.md", '1', r#"{"status": "done", "size": "xl"}"#),
        &schema,
        &target("t"),
        Notice::Fresh,
    )
    .unwrap()
    .into_string();
    let options: Vec<(String, bool)> = start_tags(&html)
        .into_iter()
        .filter(|(name, _)| name == "option")
        .map(|(_, attributes)| {
            (
                attribute(&attributes, "value").unwrap().to_owned(),
                attribute(&attributes, "selected").is_some(),
            )
        })
        .collect();
    assert_eq!(
        options,
        vec![
            ("open".to_owned(), false),
            ("done".to_owned(), true),
            (String::new(), false),
            ("s".to_owned(), false),
            ("m".to_owned(), false),
            ("xl".to_owned(), true),
        ]
    );
}

#[test]
fn an_unsupported_or_oversized_field_shows_its_value_as_read_only_text() {
    let long = "x".repeat(MAX_VALUE_BYTES + 1);
    let body = format!(r#"{{"title": "{long}", "status": "open", "tags": ["alpha", "beta"]}}"#);
    let html = form(&["tags", "title", "status"], &body);
    let names = control_names(&html);
    assert!(!names.contains(&"f.tags".to_owned()));
    assert!(!names.contains(&"f.title".to_owned()));
    assert!(names.contains(&"f.status".to_owned()));
    let text = text_of(&html);
    assert_eq!(
        text.matches("This field cannot be edited here").count(),
        2,
        "{text}"
    );
    assert!(text.contains("alpha"));
    let (_, base) = control(&html, "base");
    let base: serde_json::Value = serde_json::from_str(attribute(&base, "value").unwrap()).unwrap();
    assert_eq!(base, serde_json::json!({"status": "open"}));
}

/// The schema, slot and record of a task with 32 text fields `f0` to
/// `f31`, each holding `value`, with `f0` to `f9` as prose.
fn wide(value: &str) -> (Schema, hangar_core::Slot, hangar_core::Record) {
    let names: Vec<String> = (0..MAX_FIELDS).map(|index| format!("f{index}")).collect();
    let properties: serde_json::Map<String, serde_json::Value> = names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let spec = if index < 10 {
                serde_json::json!({"type": "string", "x-board-prose": true})
            } else {
                serde_json::json!({"type": "string"})
            };
            (name.clone(), spec)
        })
        .collect();
    let schema =
        Schema::parse(&serde_json::json!({"type": "object", "properties": properties}).to_string())
            .unwrap();
    let body: serde_json::Map<String, serde_json::Value> = names
        .iter()
        .map(|name| (name.clone(), serde_json::json!(value)))
        .collect();
    let names: Vec<&str> = names.iter().map(String::as_str).collect();
    (
        schema,
        slot("task", &names, None),
        record(
            "/tasks/a.md",
            '1',
            &serde_json::Value::Object(body).to_string(),
        ),
    )
}

#[test]
fn the_largest_form_posts_within_the_servers_one_mib_body_limit() {
    // Each record stays under Board's 1 MiB record limit: a control
    // character, a line break and a quote take more than one byte in JSON.
    let cases = [
        "\u{65e5}".repeat(5000),
        "\u{1}".repeat(5000),
        "\n".repeat(15_000),
        "\"".repeat(15_000),
        "x".repeat(MAX_VALUE_BYTES),
    ];
    for value in cases {
        assert!(value.len() <= MAX_VALUE_BYTES);
        let (schema, slot, record) = wide(&value);
        let html = edit_form(&slot, &record, &schema, &target("tok-1"), Notice::Fresh)
            .unwrap()
            .into_string();
        let pairs = submit(&html);
        let length = crate::test_support::urlencoded_len(&pairs);
        assert!(length <= 1024 * 1024, "{length}");
        let drawn = control_names(&html)
            .iter()
            .filter(|name| name.starts_with("f."))
            .count();
        assert!(drawn >= 1, "{drawn}");
        assert_eq!(
            text_of(&html)
                .matches("This field cannot be edited here")
                .count(),
            MAX_FIELDS - drawn
        );
        assert!(
            FormPost::from_pairs(pairs)
                .unwrap()
                .into_edit(&slot, &schema)
                .is_ok()
        );
    }
}

#[test]
fn a_form_of_32_short_fields_draws_every_field() {
    let (schema, slot, record) = wide("short value");
    let html = edit_form(&slot, &record, &schema, &target("tok-1"), Notice::Fresh)
        .unwrap()
        .into_string();
    let drawn = control_names(&html)
        .iter()
        .filter(|name| name.starts_with("f."))
        .count();
    assert_eq!(drawn, MAX_FIELDS);
}

#[test]
fn a_field_whose_value_has_the_wrong_type_for_its_schema_is_read_only() {
    let html = form(&["title", "status"], r#"{"title": 7, "status": "open"}"#);
    assert!(!control_names(&html).contains(&"f.title".to_owned()));
}

#[test]
fn a_hostile_value_label_or_description_adds_no_markup() {
    let schema = Schema::parse(
        r#"{"properties": {"title": {"type": "string", "title": "<b onmouseover=x>T</b>", "description": "</label><input name=\"f.body\">"}}}"#,
    )
    .unwrap();
    let html = edit_form(
        &slot("task", &["title"], None),
        &record(
            "/tasks/a.md",
            '1',
            r#"{"title": "\"><script>alert(1)</script><input name='f.body'>"}"#,
        ),
        &schema,
        &target("\"><script>"),
        Notice::Fresh,
    )
    .unwrap()
    .into_string();
    let elements: Vec<String> = start_tags(&html)
        .into_iter()
        .map(|(name, _)| name)
        .collect();
    assert!(!elements.contains(&"script".to_owned()));
    assert!(!elements.contains(&"b".to_owned()));
    let names = control_names(&html);
    assert_eq!(
        names,
        vec!["token", "based_on", "through", "slot", "base", "f.title"]
    );
}

#[test]
fn the_form_has_no_script_no_event_handler_and_no_style_attribute() {
    let html = form(&["title", "status", "body", "flagged", "tags"], BODY);
    for token in tokens(&html) {
        if let Tok::Start { name, attributes } = token {
            assert_ne!(name, "script");
            for (key, _) in attributes {
                assert!(!key.starts_with("on") && key != "style", "{key}");
            }
        }
    }
}

#[test]
fn the_hint_names_the_fields_a_save_writes_and_says_since_you_opened_this_form() {
    let text = text_of(&form(&["title", "status"], BODY));
    assert!(text.contains("Edit this task"), "{text}");
    assert!(
        text.contains(
            "Save writes Title and Status to this one task, if nobody has changed it since you opened this form. This page does not let you edit the body."
        ),
        "{text}"
    );
}

#[test]
fn the_hint_names_no_field_when_the_page_edits_every_prose_field() {
    let text = text_of(&form(&["title", "status", "body"], BODY));
    assert!(
        text.contains(
            "Save writes Title, Status and body to this one task, if nobody has changed it since you opened this form. This page does not let you edit the other fields."
        ),
        "{text}"
    );
    let text = text_of(&form(
        &["title", "status"],
        r#"{"title": "Ship it", "status": "open"}"#,
    ));
    assert!(text.contains("since you opened this form."), "{text}");
    assert!(!text.contains("This page does not let you edit"), "{text}");
}

#[test]
fn a_refused_post_is_drawn_again_with_its_entries_its_base_and_the_message() {
    let first = form(&["title", "estimate"], BODY);
    let pairs: Vec<(String, String)> = submit(&first)
        .into_iter()
        .map(|(name, value)| match name.as_str() {
            "f.estimate" => (name, "lots".to_owned()),
            "f.title" => (name, "New title".to_owned()),
            _ => (name, value),
        })
        .collect();
    let post = FormPost::from_pairs(pairs).unwrap();
    let html = edit_form(
        &slot("task", &["title", "estimate"], None),
        &record(
            "/tasks/a.md",
            '2',
            r#"{"title": "Moved on", "estimate": 9}"#,
        ),
        &task_schema(),
        &target("tok-2"),
        Notice::Refused {
            post: &post,
            message: "the field Estimate needs a whole number",
        },
    )
    .unwrap()
    .into_string();
    assert!(text_of(&html).contains("the field Estimate needs a whole number"));
    let again = submit(&html);
    let value = |key: &str| {
        again
            .iter()
            .find(|(name, _)| name == key)
            .unwrap()
            .1
            .clone()
    };
    assert_eq!(value("f.title"), "New title");
    assert_eq!(value("f.estimate"), "lots");
    assert_eq!(value("based_on"), "1".repeat(40));
    assert_eq!(value("token"), "tok-2");
}

#[test]
fn a_saved_or_unchanged_notice_is_stated() {
    let draw = |notice| {
        text_of(
            edit_form(
                &slot("task", &["title"], None),
                &record("/tasks/a.md", '1', BODY),
                &task_schema(),
                &target("t"),
                notice,
            )
            .unwrap()
            .as_str(),
        )
    };
    assert!(draw(Notice::Saved).contains("Saved."));
    assert!(draw(Notice::Unchanged).contains("Nothing to save."));
}

#[test]
fn a_record_that_is_not_an_object_or_of_another_type_gets_no_form() {
    let draw = |record: &Record| {
        edit_form(
            &slot("task", &["title"], None),
            record,
            &task_schema(),
            &target("t"),
            Notice::Fresh,
        )
    };
    assert_eq!(
        draw(&record("/tasks/a.md", '1', "[1]")),
        Err(FormError::NotAnObject)
    );
    let other = Record::new(
        RecordPath::try_from("/notes/a").unwrap(),
        RecordType::try_from("notes/note/v1").unwrap(),
        crate::test_support::blob('1'),
        RecordBody::try_from("{}").unwrap(),
    );
    assert!(matches!(draw(&other), Err(FormError::Misfit(_))));
    assert_eq!(TASK, "tasks/task/v2");
}

#[test]
fn a_slot_with_more_than_32_editable_fields_gets_no_form() {
    let names: Vec<String> = (0..33).map(|index| format!("f{index}")).collect();
    let editable: Vec<&str> = names.iter().map(String::as_str).collect();
    let result = edit_form(
        &slot("task", &editable, None),
        &record("/tasks/a.md", '1', "{}"),
        &task_schema(),
        &target("t"),
        Notice::Fresh,
    );
    assert_eq!(result, Err(FormError::TooManyFields { max: 32 }));
}

#[test]
fn a_local_path_starts_with_exactly_one_slash_and_holds_nothing_a_browser_reads_as_a_host() {
    for good in ["/", "/b/x/edit", "/b/x?y=1#z"] {
        assert!(LocalPath::try_from(good).is_ok(), "{good}");
    }
    assert_eq!(LocalPath::try_from(""), Err(LocalPathError::NotLocal));
    assert_eq!(LocalPath::try_from("b/x"), Err(LocalPathError::NotLocal));
    assert_eq!(
        LocalPath::try_from("//evil.example"),
        Err(LocalPathError::NotLocal)
    );
    assert_eq!(
        LocalPath::try_from("https://evil.example/"),
        Err(LocalPathError::NotLocal)
    );
    for bad in ["/\\evil.example", "/a b", "/a\tb", "/a\nb", "/\0"] {
        assert_eq!(
            LocalPath::try_from(bad),
            Err(LocalPathError::Malformed),
            "{bad:?}"
        );
    }
}
