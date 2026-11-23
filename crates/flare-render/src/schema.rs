//! The JSON Schema of a record type, read as the subset that an edit form
//! can draw.

use flare_core::{FieldName, MAX_RECORD_BYTES};
use serde_json::{Map, Number, Value};
use std::collections::BTreeMap;

/// The JSON Schema of one record type, read from its text.
///
/// Only the top-level `properties` and `required` matter. Each property is
/// read as a [`FieldKind`]; a shape outside the subset an edit form can draw
/// is [`FieldKind::Unsupported`].
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    fields: BTreeMap<String, FieldSpec>,
}

/// What the schema says about one field.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FieldSpec {
    pub(crate) kind: FieldKind,
    pub(crate) title: Option<String>,
    pub(crate) description: Option<String>,
    pub(crate) required: bool,
}

/// The control that an edit form draws for a field.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum FieldKind {
    /// `type: string`: a one-line text input.
    Text,
    /// `type: string` with `x-board-prose: true`: a textarea.
    Prose,
    /// `type: string` with `format: date`: a date input.
    Date,
    /// `enum` of strings: a select with these choices, in schema order.
    Choice(Vec<String>),
    /// `type: boolean`: a checkbox.
    Boolean,
    /// `type: integer`: a number input with a whole-number step.
    Integer(NumberRange),
    /// `type: number`: a number input.
    Number(NumberRange),
    /// Any other shape, such as an array, an object, `$ref`, `anyOf`,
    /// `oneOf`, `const`, a list of types, or a field the schema does not
    /// name. A form shows such a field's value as read-only text.
    Unsupported,
}

/// The `minimum`, `maximum` and `multipleOf` of a number field, as the
/// schema states them.
#[derive(Debug, Clone, PartialEq, Default)]
#[non_exhaustive]
pub struct NumberRange {
    minimum: Option<Number>,
    maximum: Option<Number>,
    step: Option<Number>,
}

impl NumberRange {
    /// The smallest value the schema allows, from `minimum`.
    #[must_use]
    pub fn minimum(&self) -> Option<&Number> {
        self.minimum.as_ref()
    }

    /// The largest value the schema allows, from `maximum`.
    #[must_use]
    pub fn maximum(&self) -> Option<&Number> {
        self.maximum.as_ref()
    }

    /// The step between allowed values, from `multipleOf`.
    #[must_use]
    pub fn step(&self) -> Option<&Number> {
        self.step.as_ref()
    }
}

/// Why schema text could not be read.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum SchemaError {
    /// The text is not JSON.
    #[error("the schema is not JSON")]
    Json(#[source] serde_json::Error),
    /// The text is JSON but not one object.
    #[error("the schema is not a JSON object")]
    NotAnObject,
    /// `properties` is not an object, or `required` is not a list of
    /// strings.
    #[error("the schema's {keyword} has the wrong shape")]
    Malformed {
        /// The keyword whose value has the wrong shape.
        keyword: &'static str,
    },
    /// The text is longer than 1 MiB.
    #[error("the schema is longer than {max} bytes")]
    TooLong {
        /// The longest schema text in bytes.
        max: usize,
    },
}

impl Schema {
    /// Reads the JSON Schema text of one record type.
    ///
    /// # Errors
    ///
    /// Returns [`SchemaError::TooLong`] for text over 1 MiB,
    /// [`SchemaError::Json`] for text that is not JSON,
    /// [`SchemaError::NotAnObject`] for JSON that is not an object, and
    /// [`SchemaError::Malformed`] when `properties` is not an object or
    /// `required` is not a list of strings.
    pub fn parse(text: &str) -> Result<Self, SchemaError> {
        if text.len() > MAX_RECORD_BYTES {
            return Err(SchemaError::TooLong {
                max: MAX_RECORD_BYTES,
            });
        }
        let Value::Object(schema) = serde_json::from_str(text).map_err(SchemaError::Json)? else {
            return Err(SchemaError::NotAnObject);
        };
        let required = match schema.get("required") {
            None => Vec::new(),
            Some(Value::Array(names)) => names
                .iter()
                .map(|name| name.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
                .ok_or(SchemaError::Malformed {
                    keyword: "required",
                })?,
            Some(_) => {
                return Err(SchemaError::Malformed {
                    keyword: "required",
                });
            }
        };
        let properties = match schema.get("properties") {
            None => &Map::new(),
            Some(Value::Object(properties)) => properties,
            Some(_) => {
                return Err(SchemaError::Malformed {
                    keyword: "properties",
                });
            }
        };
        let fields = properties
            .iter()
            .map(|(name, property)| (name.clone(), read_field(property, required.contains(name))))
            .collect();
        Ok(Self { fields })
    }

    /// The control an edit form draws for `field`.
    #[must_use]
    pub fn field(&self, field: &FieldName) -> FieldKind {
        self.spec(field)
            .map_or(FieldKind::Unsupported, |spec| spec.kind.clone())
    }

    /// What the schema says about `field`, when it names it.
    pub(crate) fn spec(&self, field: &FieldName) -> Option<&FieldSpec> {
        self.fields.get(field.as_str())
    }
}

/// Keywords that take a field outside the subset a form draws.
const UNSUPPORTED_KEYWORDS: &[&str] = &["$ref", "anyOf", "oneOf", "allOf", "not", "const", "if"];

/// Reads one property of the schema.
fn read_field(property: &Value, required: bool) -> FieldSpec {
    let text = |key: &str| property.get(key).and_then(Value::as_str).map(str::to_owned);
    FieldSpec {
        kind: read_kind(property),
        title: text("title"),
        description: text("description"),
        required,
    }
}

fn read_kind(property: &Value) -> FieldKind {
    let Value::Object(keywords) = property else {
        return FieldKind::Unsupported;
    };
    if UNSUPPORTED_KEYWORDS
        .iter()
        .any(|keyword| keywords.contains_key(*keyword))
    {
        return FieldKind::Unsupported;
    }
    let kind = keywords.get("type");
    if let Some(choices) = keywords.get("enum") {
        let strings = choices.as_array().and_then(|choices| {
            choices
                .iter()
                .map(|choice| choice.as_str().map(str::to_owned))
                .collect::<Option<Vec<_>>>()
        });
        let string_typed = kind.is_none_or(|kind| kind.as_str() == Some("string"));
        return match strings {
            Some(choices) if string_typed => FieldKind::Choice(choices),
            _ => FieldKind::Unsupported,
        };
    }
    let range = || NumberRange {
        minimum: number(keywords, "minimum"),
        maximum: number(keywords, "maximum"),
        step: number(keywords, "multipleOf"),
    };
    match kind.and_then(Value::as_str) {
        Some("string") if keywords.get("format").and_then(Value::as_str) == Some("date") => {
            FieldKind::Date
        }
        Some("string") if keywords.get("x-board-prose") == Some(&Value::Bool(true)) => {
            FieldKind::Prose
        }
        Some("string") => FieldKind::Text,
        Some("boolean") => FieldKind::Boolean,
        Some("integer") => FieldKind::Integer(range()),
        Some("number") => FieldKind::Number(range()),
        _ => FieldKind::Unsupported,
    }
}

fn number(keywords: &Map<String, Value>, key: &str) -> Option<Number> {
    match keywords.get(key) {
        Some(Value::Number(number)) => Some(number.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests;
