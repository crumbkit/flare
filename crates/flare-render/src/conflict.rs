//! The conflict view (decisions 25dd and d340).

use crate::apply::{Conflict, same};
use crate::body::Fields;
use crate::form::{self, FormTarget, LocalPath, MAX_VALUE_BYTES, PostBudget, hidden_inputs};
use crate::html::{Builder, Html};
use crate::values;
use serde_json::Value;

/// Draws a conflict: each drawn field now beside the person's edit, what a
/// save would write, a form whose "Save my edit" posts the merged values
/// against the newer record with a fresh token, and a "Discard my edit"
/// link to `discard`. Every value is escaped.
///
/// The form is an ordinary post of the edit form's shape: `based_on` is the
/// newer blob, `base` holds the newer values, and each field's input holds
/// the person's entry where they changed the field and the newer value
/// where they did not. Board refuses it again if the record changes once
/// more.
#[must_use]
pub fn conflict_view(conflict: &Conflict, target: &FormTarget, discard: &LocalPath) -> Html {
    let edit = &conflict.edit;
    let noun = edit.noun.as_str();
    let mut builder = Builder::default();
    builder.open("div", &[("class", "conflict")]);
    builder.open("div", &[("class", "msg bad")]);
    builder.element(
        "b",
        &[],
        &format!("Your edit was not saved, because someone changed this {noun} since you opened this form."),
    );
    builder.element("span", &[], "Nothing you typed is lost.");
    builder.close("div");

    builder.open("div", &[("class", "cf")]);
    builder.open("div", &[("class", "r hd")]);
    builder.element("span", &[], "");
    header(
        &mut builder,
        &format!("The {noun} now"),
        "as it is saved now",
    );
    header(&mut builder, "Your edit", "from the version you opened");
    builder.close("div");
    let mut merged = Vec::new();
    let mut base = Fields::new();
    for (field, now) in edit.fields.iter().zip(&conflict.now) {
        let changed = !same(field.entered.as_ref(), field.base.as_ref());
        builder.open("div", &[("class", "r")]);
        builder.element("span", &[("class", "f")], &field.label);
        if same(now.as_ref(), field.base.as_ref()) {
            cell(
                &mut builder,
                "same",
                &values::shown(now.as_ref()),
                false,
                "not changed",
            );
        } else {
            let was = format!("was {}", values::shown(field.base.as_ref()));
            cell(
                &mut builder,
                "keep",
                &values::shown(now.as_ref()),
                true,
                &was,
            );
        }
        if changed {
            cell(
                &mut builder,
                "keep",
                &values::shown(field.entered.as_ref()),
                true,
                "",
            );
        } else {
            let base = values::shown(field.base.as_ref());
            cell(&mut builder, "same", &base, false, "you did not change it");
        }
        builder.close("div");
        let value = if changed {
            field.entered.clone()
        } else {
            now.clone()
        };
        merged.push((field, value, changed, now));
    }
    // The form carries each changed field, and each unchanged field whose
    // newer value an edit form would draw a control for and that fits in
    // the post. A field it leaves out stays as it is now.
    let mut budget = PostBudget::new(
        target.token(),
        &conflict.newer,
        target.binding(),
        &edit.slot,
    );
    let mut posted = Vec::new();
    for pass_changed in [true, false] {
        for (field, value, changed, now) in &merged {
            if *changed != pass_changed {
                continue;
            }
            let text = values::control_text(value.as_ref());
            let loaded = Option::clone(now).unwrap_or(Value::Null);
            let drawable = values::fits(&field.kind, now.as_ref())
                && form::size(now.as_ref()) <= MAX_VALUE_BYTES;
            // A changed field is carried even when it no longer fits, so
            // the edit is never dropped.
            let fits = (*changed || drawable) && budget.take(&field.name, &text, &loaded);
            if *changed || fits {
                base.insert(field.name.as_str().to_owned(), loaded);
                posted.push((field.name.clone(), text));
            }
        }
    }
    builder.element(
        "div",
        &[("class", "rest")],
        "Fields this page does not edit stay as they are now.",
    );
    builder.close("div");

    builder.open(
        "form",
        &[
            ("class", "result"),
            ("method", "post"),
            ("action", target.action().as_str()),
        ],
    );
    hidden_inputs(&mut builder, target, &conflict.newer, &edit.slot, &base);
    for (name, text) in &posted {
        let input = format!("f.{name}");
        builder.open(
            "input",
            &[("type", "hidden"), ("name", &input), ("value", text)],
        );
    }
    builder.element(
        "h3",
        &[],
        &format!("Save your edit onto the {noun} as it is now?"),
    );
    builder.open("dl", &[]);
    for (field, value, ..) in &merged {
        builder.element("dt", &[], &field.label);
        builder.element("dd", &[], &values::shown(value.as_ref()));
    }
    builder.element("dt", &[], "Other fields");
    builder.element("dd", &[], "as they are now");
    builder.close("dl");
    builder.open("div", &[("class", "go")]);
    builder.element(
        "button",
        &[("class", "btn"), ("type", "submit")],
        "Save my edit",
    );
    builder.element(
        "a",
        &[("class", "btn quiet"), ("href", discard.as_str())],
        "Discard my edit",
    );
    builder.element(
        "span",
        &[("class", "hint")],
        &format!("The save is refused again if the {noun} changes once more."),
    );
    builder.close("div");
    builder.close("form");
    builder.close("div");
    builder.finish()
}

fn header(builder: &mut Builder, title: &str, line: &str) {
    builder.open("span", &[]);
    builder.element("b", &[], title);
    builder.text(line);
    builder.close("span");
}

/// One value cell: the value, marked when it differs from the version the
/// form was drawn from, and a line under it.
fn cell(builder: &mut Builder, class: &'static str, value: &str, marked: bool, line: &str) {
    builder.open("span", &[("class", class)]);
    if marked {
        builder.element("mark", &[], value);
    } else {
        builder.text(value);
    }
    if !line.is_empty() {
        builder.element("span", &[("class", "sub")], line);
    }
    builder.close("span");
}

#[cfg(test)]
mod tests;
