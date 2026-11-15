//! The control of each field kind, and the read-only row of a field no form
//! edits.

use crate::html::Builder;
use crate::schema::{FieldKind, NumberRange};

/// One field the form draws a control for.
#[derive(Debug)]
pub(super) struct Control {
    /// The input's name, `f.` and the field's name.
    pub(super) input: String,
    pub(super) kind: FieldKind,
    pub(super) label: String,
    pub(super) description: Option<String>,
    pub(super) required: bool,
    /// The value the control holds, as a browser would post it.
    pub(super) text: String,
}

/// The line under a field that no form edits.
const NOT_EDITABLE: &str = "This field cannot be edited here";

/// Writes the label and control of one field.
pub(super) fn control(builder: &mut Builder, control: &Control) {
    builder.open("label", &[("class", "field")]);
    builder.text(&control.label);
    let input = control.input.as_str();
    let text = control.text.as_str();
    match &control.kind {
        FieldKind::Prose => textarea(builder, input, text, control.required),
        FieldKind::Text if text.contains('\n') => textarea(builder, input, text, control.required),
        FieldKind::Text => line(builder, "text", input, text, control.required),
        FieldKind::Date => line(builder, "date", input, text, control.required),
        FieldKind::Choice(choices) => select(builder, input, text, choices, control.required),
        FieldKind::Boolean => {
            builder.open(
                "input",
                &[("type", "checkbox"), ("name", input), ("value", "true")],
            );
            if text == "true" {
                builder.flag("checked");
            }
        }
        FieldKind::Integer(range) => number(builder, input, text, range, "1", control.required),
        FieldKind::Number(range) => number(builder, input, text, range, "any", control.required),
        // A form never draws a control for an unsupported field; the caller
        // writes its read-only row instead.
        FieldKind::Unsupported => {}
    }
    if let Some(description) = &control.description {
        builder.element("span", &[("class", "hint")], description);
    }
    builder.close("label");
}

/// Writes the read-only row of a field that no form edits.
pub(super) fn read_only(builder: &mut Builder, label: &str, value: &str) {
    builder.open("div", &[("class", "field readonly")]);
    builder.element("span", &[], label);
    builder.element("span", &[("class", "value")], value);
    builder.element("span", &[("class", "hint")], NOT_EDITABLE);
    builder.close("div");
}

fn line(builder: &mut Builder, kind: &'static str, input: &str, text: &str, required: bool) {
    builder.open("input", &[("type", kind), ("name", input), ("value", text)]);
    if required {
        builder.flag("required");
    }
}

fn textarea(builder: &mut Builder, input: &str, text: &str, required: bool) {
    builder.open("textarea", &[("name", input), ("rows", "6")]);
    if required {
        builder.flag("required");
    }
    // A browser drops one newline that directly follows <textarea>, so one is
    // written first and a value that starts with a newline keeps it.
    builder.raw("\n");
    builder.text(text);
    builder.close("textarea");
}

fn select(builder: &mut Builder, input: &str, text: &str, choices: &[String], required: bool) {
    builder.open("select", &[("name", input)]);
    let mut option = |value: &str| {
        builder.open("option", &[("value", value)]);
        if value == text {
            builder.flag("selected");
        }
        builder.text(value);
        builder.close("option");
    };
    if !required {
        option("");
    }
    for choice in choices {
        option(choice);
    }
    // A current value outside the enum stays a choice, so the form does not
    // change it unless the person does.
    if !text.is_empty() && !choices.iter().any(|choice| choice == text) {
        option(text);
    }
    builder.close("select");
}

fn number(
    builder: &mut Builder,
    input: &str,
    text: &str,
    range: &NumberRange,
    step: &str,
    required: bool,
) {
    let step = range
        .step()
        .map_or_else(|| step.to_owned(), ToString::to_string);
    let mut attributes = vec![
        ("type", "number"),
        ("name", input),
        ("value", text),
        ("step", step.as_str()),
    ];
    let minimum = range.minimum().map(ToString::to_string);
    let maximum = range.maximum().map(ToString::to_string);
    if let Some(minimum) = &minimum {
        attributes.push(("min", minimum));
    }
    if let Some(maximum) = &maximum {
        attributes.push(("max", maximum));
    }
    builder.open("input", &attributes);
    if required {
        builder.flag("required");
    }
}
