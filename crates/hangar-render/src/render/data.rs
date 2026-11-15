//! The data a template renders: each slot's record as a mustache2 value.

use super::RenderError;
use crate::body;
use hangar_core::{Page, Record, Sample, SlotName};
use mustache2::Data;
use serde_json::Value;

/// The data of a page: one map whose keys are slot names and whose values
/// are the objects of the records that fill them, or of their samples.
///
/// # Errors
///
/// Returns [`RenderError::Misfit`] for a fill of an undeclared slot or of
/// the wrong type, and [`RenderError::NotAnObject`] for a body that is not
/// one JSON object.
pub(crate) fn page_data(page: &Page, fills: &[(SlotName, Record)]) -> Result<Data, RenderError> {
    page.check_fit(
        fills
            .iter()
            .map(|(slot, record)| (slot, record.record_type())),
    )
    .map_err(RenderError::Misfit)?;
    let mut slots = std::collections::HashMap::new();
    for slot in page.slots().iter() {
        let filled = fills
            .iter()
            .find(|(name, _)| name == slot.name())
            .map(|(_, record)| record.body());
        let Some(text) = filled.or_else(|| slot.sample().map(Sample::body)) else {
            continue;
        };
        let fields = body::fields(text).ok_or_else(|| RenderError::NotAnObject {
            slot: slot.name().clone(),
        })?;
        let value = Data::Map(
            fields
                .iter()
                .map(|(key, value)| (clean(key), to_data(value)))
                .collect(),
        );
        slots.insert(slot.name().as_str().to_owned(), value);
    }
    Ok(Data::Map(slots))
}

/// A serde_json value as mustache2 data. A number becomes a string that
/// holds serde_json's text of it, so mustache2 does not reformat it, and a
/// number is truthy in a section even when it is 0.
fn to_data(value: &Value) -> Data {
    match value {
        Value::Null => Data::Null,
        Value::Bool(flag) => Data::Bool(*flag),
        Value::Number(number) => Data::String(number.to_string()),
        Value::String(text) => Data::String(clean(text)),
        Value::Array(items) => Data::List(items.iter().map(to_data).collect()),
        Value::Object(fields) => Data::Map(
            fields
                .iter()
                .map(|(key, value)| (clean(key), to_data(value)))
                .collect(),
        ),
    }
}

/// `text` with NUL replaced by U+FFFD, because mustache2 writes NUL as it is.
fn clean(text: &str) -> String {
    text.replace('\0', "\u{FFFD}")
}
