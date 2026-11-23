//! Field values as the form, the post and the conflict view spell them.

use crate::schema::{FieldKind, Schema};
use flare_core::{FieldName, SlotName};
use serde_json::Value;

/// The label of a field: its schema `title`, or else its name.
pub(crate) fn label(schema: &Schema, field: &FieldName) -> String {
    schema
        .spec(field)
        .and_then(|spec| spec.title.clone())
        .unwrap_or_else(|| field.as_str().to_owned())
}

/// What the texts call a slot's record: the slot's name, with `_` read as a
/// space, such as "task" for the slot `task`.
pub(crate) fn noun(slot: &SlotName) -> String {
    slot.as_str().replace('_', " ")
}

/// Whether a field's current value has the JSON type its kind edits. An
/// absent value and null fit every kind.
pub(crate) fn fits(kind: &FieldKind, value: Option<&Value>) -> bool {
    match (kind, value) {
        (FieldKind::Unsupported, _) => false,
        (_, None | Some(Value::Null)) => true,
        (
            FieldKind::Text | FieldKind::Prose | FieldKind::Date | FieldKind::Choice(_),
            Some(value),
        ) => value.is_string(),
        (FieldKind::Boolean, Some(value)) => value.is_boolean(),
        (FieldKind::Integer(_), Some(value)) => value.is_i64() || value.is_u64(),
        (FieldKind::Number(_), Some(value)) => value.is_number(),
    }
}

/// The text a control holds for `value`: a string as it is, a number as its
/// JSON text, a boolean as `true` or `false`, and nothing for no value.
pub(crate) fn control_text(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// The text that shows `value` to a person: a string as it is, any other
/// value as its JSON text, and "empty" for no value.
pub(crate) fn shown(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => "empty".to_owned(),
        Some(Value::String(text)) if text.is_empty() => "empty".to_owned(),
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
    }
}

/// `labels` joined as a sentence lists them: "A", "A and B", "A, B and C".
pub(crate) fn joined(labels: &[String]) -> String {
    match labels {
        [] => String::new(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}
