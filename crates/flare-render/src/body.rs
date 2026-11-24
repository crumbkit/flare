//! Record bodies read as JSON objects.

use flare_core::RecordBody;
use serde_json::{Map, Value};

/// A record's fields: the body read as one JSON object.
pub(crate) type Fields = Map<String, Value>;

/// Reads `body` as the text of one JSON object, or `None` when it is any
/// other text.
pub(crate) fn fields(body: &RecordBody) -> Option<Fields> {
    match serde_json::from_str::<Value>(body.as_str()) {
        Ok(Value::Object(fields)) => Some(fields),
        _ => None,
    }
}
