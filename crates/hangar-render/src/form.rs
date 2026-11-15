//! The edit form that hangar draws under a page for one filled slot
//! (decision 13ee).

mod budget;
mod controls;

pub(crate) use budget::{PostBudget, encoded};

use crate::body::{self, Fields};
use crate::html::{Builder, Html};
use crate::post::FormPost;
use crate::schema::{FieldKind, Schema};
use crate::token::FormToken;
use crate::values;
use controls::Control;
use hangar_core::{BindingName, Blob, FieldName, Misfit, Record, Slot, SlotName};
use serde_json::Value;
use std::fmt;

/// The largest number of fields one form draws.
pub(crate) const MAX_FIELDS: usize = 32;
/// The largest field value in bytes that a form draws a control for.
pub(crate) const MAX_VALUE_BYTES: usize = 16 * 1024;

/// The longest local path in bytes.
const MAX_PATH_BYTES: usize = 2048;

/// A path on the server, such as `/b/hangar-plan/edit`: it starts with one
/// `/` and holds no backslash, whitespace or control character, so a browser
/// cannot read it as another host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalPath(String);

impl LocalPath {
    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why text is not a [`LocalPath`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum LocalPathError {
    /// The text does not start with exactly one `/`.
    #[error("a local path starts with exactly one /")]
    NotLocal,
    /// The text holds a backslash, whitespace or a control character, or is
    /// longer than 2,048 bytes.
    #[error(
        "a local path holds no backslash, whitespace or control character and is at most 2048 bytes"
    )]
    Malformed,
}

impl TryFrom<&str> for LocalPath {
    type Error = LocalPathError;

    fn try_from(text: &str) -> Result<Self, LocalPathError> {
        if !text.starts_with('/') || text.starts_with("//") {
            return Err(LocalPathError::NotLocal);
        }
        let unsafe_character = |character: char| {
            character == '\\' || character.is_whitespace() || character.is_control()
        };
        if text.len() > MAX_PATH_BYTES || text.chars().any(unsafe_character) {
            return Err(LocalPathError::Malformed);
        }
        Ok(Self(text.to_owned()))
    }
}

impl fmt::Display for LocalPath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Where a form posts and what it carries for the server to check: the
/// action path, the single-use token and the binding the edit goes through.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct FormTarget {
    action: LocalPath,
    token: FormToken,
    binding: BindingName,
}

impl FormTarget {
    /// Builds a target.
    #[must_use]
    pub fn new(action: LocalPath, token: FormToken, binding: BindingName) -> Self {
        Self {
            action,
            token,
            binding,
        }
    }

    /// The path the form posts to.
    #[must_use]
    pub fn action(&self) -> &LocalPath {
        &self.action
    }

    /// The token the form carries.
    #[must_use]
    pub fn token(&self) -> &FormToken {
        &self.token
    }

    /// The binding the edit goes through.
    #[must_use]
    pub fn binding(&self) -> &BindingName {
        &self.binding
    }
}

/// What the form says above its fields.
#[derive(Debug, Clone, Copy)]
#[non_exhaustive]
pub enum Notice<'a> {
    /// Nothing: a form opened fresh.
    Fresh,
    /// The last post was saved.
    Saved,
    /// The last post changed nothing.
    Unchanged,
    /// The last post was refused. The form shows the post's entries again,
    /// keeps the post's `based_on` and `base`, and states `message`.
    Refused {
        /// The refused post.
        post: &'a FormPost,
        /// Why it was refused, such as a `PostError`'s text or Board's
        /// rejection.
        message: &'a str,
    },
}

/// Why a form could not be drawn.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum FormError {
    /// The record's body is not one JSON object.
    #[error("the record is not a JSON object")]
    NotAnObject,
    /// The record's type differs from the slot's type.
    #[error("the record does not fit the slot")]
    Misfit(#[source] Misfit),
    /// The slot lists more editable fields than one form draws.
    #[error("the slot lists more than {max} editable fields")]
    TooManyFields {
        /// The largest number of fields one form draws.
        max: usize,
    },
}

/// Draws one `<form method="post">` that edits `record` in `slot`.
///
/// The form lists the slot's editable fields in the slot's order, each with
/// the control its [`FieldKind`](crate::FieldKind) gives. A field the form
/// cannot edit shows its value as read-only text. Hidden inputs carry the
/// token, the blob the form was drawn from, the binding, the slot and the
/// loaded values of the drawn fields.
///
/// # Errors
///
/// Returns [`FormError::NotAnObject`] for a body that is not one JSON
/// object, [`FormError::Misfit`] when the record's type is not the slot's,
/// and [`FormError::TooManyFields`] when the slot lists more than 32
/// editable fields.
pub fn edit_form(
    slot: &Slot,
    record: &Record,
    schema: &Schema,
    target: &FormTarget,
    notice: Notice<'_>,
) -> Result<Html, FormError> {
    if record.record_type() != slot.record_type() {
        return Err(FormError::Misfit(Misfit::WrongType {
            slot: slot.name().clone(),
            expected: slot.record_type().clone(),
            found: record.record_type().clone(),
        }));
    }
    let fields = body::fields(record.body()).ok_or(FormError::NotAnObject)?;
    let mut editable: Vec<&FieldName> = Vec::new();
    for field in slot.editable_fields() {
        if !editable.contains(&field) {
            editable.push(field);
        }
    }
    if editable.len() > MAX_FIELDS {
        return Err(FormError::TooManyFields { max: MAX_FIELDS });
    }
    let refused = match notice {
        Notice::Refused { post, .. } => Some(post),
        _ => None,
    };
    let based_on = refused.map_or_else(|| record.blob(), FormPost::based_on);
    let mut budget = PostBudget::new(target.token(), based_on, target.binding(), slot.name());
    let mut rows = Vec::new();
    let mut base = Fields::new();
    for field in editable {
        let kind = schema.field(field);
        let value = fields.get(field.as_str());
        let label = values::label(schema, field);
        let drawn = match refused {
            Some(post) => {
                kind != FieldKind::Unsupported && post.base().contains_key(field.as_str())
            }
            None => values::fits(&kind, value) && size(value) <= MAX_VALUE_BYTES,
        };
        if !drawn {
            rows.push(Row::ReadOnly {
                label,
                value: values::shown(value),
            });
            continue;
        }
        let (loaded, text) = match refused {
            Some(post) => (
                post.base()
                    .get(field.as_str())
                    .cloned()
                    .unwrap_or(Value::Null),
                post.entry(field).unwrap_or_default().to_owned(),
            ),
            None => (
                value.cloned().unwrap_or(Value::Null),
                values::control_text(value),
            ),
        };
        if !budget.take(field, &text, &loaded) {
            rows.push(Row::ReadOnly {
                label,
                value: values::shown(value),
            });
            continue;
        }
        base.insert(field.as_str().to_owned(), loaded);
        let spec = schema.spec(field);
        rows.push(Row::Control(Control {
            input: format!("f.{field}"),
            kind,
            label,
            description: spec.and_then(|spec| spec.description.clone()),
            required: spec.is_some_and(|spec| spec.required),
            text,
        }));
    }
    let untouched: Vec<FieldName> = fields
        .keys()
        .filter(|key| !base.contains_key(key.as_str()))
        .filter_map(|key| FieldName::try_from(key.as_str()).ok())
        .collect();
    let mut builder = Builder::default();
    builder.open(
        "form",
        &[
            ("class", "hangar-form"),
            ("method", "post"),
            ("action", target.action().as_str()),
        ],
    );
    hidden_inputs(&mut builder, target, based_on, slot.name(), &base);
    notice_message(&mut builder, notice);
    let noun = values::noun(slot.name());
    builder.element("h3", &[], &format!("Edit this {noun}"));
    builder.open("div", &[("class", "fields")]);
    let mut written = Vec::new();
    for row in &rows {
        match row {
            Row::Control(control) => {
                controls::control(&mut builder, control);
                written.push(control.label.clone());
            }
            Row::ReadOnly { label, value } => controls::read_only(&mut builder, label, value),
        }
    }
    builder.close("div");
    builder.open("div", &[("class", "go")]);
    let hint = if written.is_empty() {
        format!("This page lets you edit no field of this {noun}.")
    } else {
        builder.element("button", &[("class", "btn"), ("type", "submit")], "Save");
        format!(
            "Save writes {} to this one {noun}, if nobody has changed it since you opened this form.{}",
            values::joined(&written),
            not_edited(schema, &untouched)
        )
    };
    builder.element("span", &[("class", "hint")], &hint);
    builder.close("div");
    builder.close("form");
    Ok(builder.finish())
}

/// The hint's sentence about the fields the form does not edit: it names
/// the prose fields among `untouched`, such as "the body", and otherwise
/// speaks of the other fields. It is empty when `untouched` is.
fn not_edited(schema: &Schema, untouched: &[FieldName]) -> String {
    if untouched.is_empty() {
        return String::new();
    }
    let prose: Vec<String> = untouched
        .iter()
        .filter(|field| schema.field(field) == FieldKind::Prose)
        .map(|field| values::label(schema, field))
        .collect();
    if prose.is_empty() {
        " This page does not let you edit the other fields.".to_owned()
    } else {
        format!(
            " This page does not let you edit the {}.",
            values::joined(&prose)
        )
    }
}

/// One row of the form.
#[derive(Debug)]
enum Row {
    Control(Control),
    ReadOnly { label: String, value: String },
}

/// The size of a value as JSON text, in bytes.
pub(crate) fn size(value: Option<&Value>) -> usize {
    value.map_or(0, |value| match value {
        Value::String(text) => text.len(),
        other => other.to_string().len(),
    })
}

/// Writes the hidden inputs that carry what the server checks and the
/// loaded values that tell a change from a field left alone.
pub(crate) fn hidden_inputs(
    builder: &mut Builder,
    target: &FormTarget,
    based_on: &Blob,
    slot: &SlotName,
    base: &Fields,
) {
    target.token().hidden_input(builder);
    let base = Value::Object(base.clone()).to_string();
    for (name, value) in [
        ("based_on", based_on.as_str()),
        ("through", target.binding().as_str()),
        ("slot", slot.as_str()),
        ("base", base.as_str()),
    ] {
        builder.open(
            "input",
            &[("type", "hidden"), ("name", name), ("value", value)],
        );
    }
}

fn notice_message(builder: &mut Builder, notice: Notice<'_>) {
    let (class, head, line) = match notice {
        Notice::Fresh => return,
        Notice::Saved => ("msg good", "Saved.", None),
        Notice::Unchanged => (
            "msg good",
            "Nothing to save.",
            Some("The record already holds these values."),
        ),
        Notice::Refused { message, .. } => ("msg bad", "Your edit was not saved.", Some(message)),
    };
    builder.open("div", &[("class", class)]);
    builder.element("b", &[], head);
    if let Some(line) = line {
        builder.element("span", &[], line);
    }
    builder.close("div");
}

#[cfg(test)]
mod tests;
