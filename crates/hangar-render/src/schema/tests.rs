//! Unit tests of the schema subset (plan test 10).

use super::*;
use crate::test_support::{field, task_schema};

fn kind(property: &str) -> FieldKind {
    Schema::parse(&format!(r#"{{"properties": {{"x": {property}}}}}"#))
        .unwrap()
        .field(&field("x"))
}

#[test]
fn a_string_is_text() {
    assert_eq!(kind(r#"{"type": "string"}"#), FieldKind::Text);
    assert_eq!(
        kind(r#"{"type": "string", "format": "board-ref"}"#),
        FieldKind::Text
    );
}

#[test]
fn a_prose_string_is_prose() {
    assert_eq!(
        kind(r#"{"type": "string", "x-board-prose": true}"#),
        FieldKind::Prose
    );
    assert_eq!(
        kind(r#"{"type": "string", "x-board-prose": false}"#),
        FieldKind::Text
    );
}

#[test]
fn a_date_string_is_a_date() {
    assert_eq!(
        kind(r#"{"type": "string", "format": "date"}"#),
        FieldKind::Date
    );
}

#[test]
fn an_enum_of_strings_is_a_choice_in_schema_order() {
    let choices = FieldKind::Choice(vec!["open".to_owned(), "done".to_owned()]);
    assert_eq!(kind(r#"{"enum": ["open", "done"]}"#), choices);
    assert_eq!(
        kind(r#"{"type": "string", "enum": ["open", "done"]}"#),
        choices
    );
}

#[test]
fn an_enum_that_holds_anything_but_strings_is_unsupported() {
    assert_eq!(kind(r#"{"enum": ["open", 1]}"#), FieldKind::Unsupported);
    assert_eq!(kind(r#"{"enum": ["open", null]}"#), FieldKind::Unsupported);
    assert_eq!(
        kind(r#"{"type": "integer", "enum": [1, 2]}"#),
        FieldKind::Unsupported
    );
}

#[test]
fn a_boolean_is_a_checkbox() {
    assert_eq!(kind(r#"{"type": "boolean"}"#), FieldKind::Boolean);
}

#[test]
fn integers_and_numbers_carry_their_minimum_maximum_and_step() {
    let FieldKind::Integer(range) = kind(r#"{"type": "integer", "minimum": 0, "maximum": 10}"#)
    else {
        panic!("not an integer");
    };
    assert_eq!(
        range.minimum().map(ToString::to_string).as_deref(),
        Some("0")
    );
    assert_eq!(
        range.maximum().map(ToString::to_string).as_deref(),
        Some("10")
    );
    assert_eq!(range.step(), None);
    let FieldKind::Number(range) = kind(r#"{"type": "number", "multipleOf": 0.25}"#) else {
        panic!("not a number");
    };
    assert_eq!(
        range.step().map(ToString::to_string).as_deref(),
        Some("0.25")
    );
}

#[test]
fn every_shape_outside_the_subset_is_unsupported() {
    for property in [
        r#"{"type": "array", "items": {"type": "string"}}"#,
        r#"{"type": "object"}"#,
        r##"{"$ref": "#/$defs/Actor"}"##,
        r#"{"anyOf": [{"type": "string"}, {"type": "integer"}]}"#,
        r#"{"oneOf": [{"type": "string"}]}"#,
        r#"{"allOf": [{"type": "string"}]}"#,
        r#"{"type": "string", "const": "x"}"#,
        r#"{"type": ["string", "null"]}"#,
        r#"{"type": "null"}"#,
        r#"{}"#,
        r#"true"#,
    ] {
        assert_eq!(kind(property), FieldKind::Unsupported, "{property}");
    }
}

#[test]
fn a_field_the_schema_does_not_name_is_unsupported() {
    assert_eq!(
        task_schema().field(&field("missing")),
        FieldKind::Unsupported
    );
}

#[test]
fn title_description_and_required_are_read() {
    let schema = task_schema();
    let title = schema.spec(&field("title")).unwrap();
    assert_eq!(title.title.as_deref(), Some("Title"));
    assert_eq!(title.description.as_deref(), Some("What the task is"));
    assert!(title.required);
    assert!(!schema.spec(&field("notes")).unwrap().required);
}

#[test]
fn text_that_is_not_a_json_schema_object_is_refused() {
    assert!(matches!(
        Schema::parse("not json"),
        Err(SchemaError::Json(_))
    ));
    assert!(matches!(
        Schema::parse("[1]"),
        Err(SchemaError::NotAnObject)
    ));
    assert!(matches!(
        Schema::parse(r#"{"properties": []}"#),
        Err(SchemaError::Malformed {
            keyword: "properties"
        })
    ));
    assert!(matches!(
        Schema::parse(r#"{"required": "title"}"#),
        Err(SchemaError::Malformed {
            keyword: "required"
        })
    ));
    assert!(matches!(
        Schema::parse(&format!(
            "{{\"description\": \"{}\"}}",
            "a".repeat(MAX_RECORD_BYTES)
        )),
        Err(SchemaError::TooLong { .. })
    ));
}

#[test]
fn a_schema_without_properties_names_no_field() {
    let schema = Schema::parse(r#"{"type": "object"}"#).unwrap();
    assert_eq!(schema.field(&field("title")), FieldKind::Unsupported);
}
