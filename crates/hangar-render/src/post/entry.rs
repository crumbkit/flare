//! One field's entry read as the value its kind takes.

use crate::schema::FieldKind;
use serde_json::{Number, Value};

/// Why an entry could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Problem {
    /// The post has no entry for a field that is not a checkbox.
    Missing,
    /// The entry does not parse; the text says what the field takes.
    Bad(&'static str),
}

/// Reads the entry for a field of `kind`. `None` is no value: an emptied
/// optional field. `base` is the field's value when the form was drawn, which
/// stays a valid choice even when the schema's enum no longer lists it.
pub(super) fn read(
    kind: &FieldKind,
    entry: Option<&str>,
    required: bool,
    base: &Value,
) -> Result<Option<Value>, Problem> {
    if *kind == FieldKind::Boolean {
        return match entry {
            None | Some("false") => Ok(Some(Value::Bool(false))),
            Some("true") => Ok(Some(Value::Bool(true))),
            Some(_) => Err(Problem::Bad("a checked or unchecked box")),
        };
    }
    let entry = entry.ok_or(Problem::Missing)?;
    let text = match kind {
        FieldKind::Integer(_) | FieldKind::Number(_) | FieldKind::Date => entry.trim(),
        _ => entry,
    };
    if text.is_empty() && !required {
        return Ok(None);
    }
    match kind {
        FieldKind::Text | FieldKind::Prose => Ok(Some(Value::String(newlines(text)))),
        FieldKind::Date if is_date(text) => Ok(Some(Value::String(text.to_owned()))),
        FieldKind::Date => Err(Problem::Bad("a date such as 2026-11-20")),
        FieldKind::Choice(choices) => {
            if choices.iter().any(|choice| choice == text) || base.as_str() == Some(text) {
                Ok(Some(Value::String(text.to_owned())))
            } else {
                Err(Problem::Bad("one of the listed choices"))
            }
        }
        FieldKind::Integer(_) => integer(text)
            .map(|number| Some(Value::Number(number)))
            .ok_or(Problem::Bad("a whole number")),
        FieldKind::Number(_) => integer(text)
            .or_else(|| {
                text.parse::<f64>()
                    .ok()
                    .filter(|number| number.is_finite())
                    .and_then(Number::from_f64)
            })
            .map(|number| Some(Value::Number(number)))
            .ok_or(Problem::Bad("a number")),
        FieldKind::Boolean | FieldKind::Unsupported => Err(Problem::Bad("nothing")),
    }
}

/// `text` with CRLF and lone CR line endings made LF, as a browser's
/// textarea posts CRLF.
pub(crate) fn newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

fn integer(text: &str) -> Option<Number> {
    text.parse::<i64>()
        .map(Number::from)
        .or_else(|_| text.parse::<u64>().map(Number::from))
        .ok()
}

/// Whether `text` is a calendar date written as `YYYY-MM-DD`.
fn is_date(text: &str) -> bool {
    let mut parts = text.split('-');
    let (Some(year), Some(month), Some(day), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    let digits = |part: &str, length: usize| {
        part.len() == length && part.bytes().all(|byte| byte.is_ascii_digit())
    };
    if !(digits(year, 4) && digits(month, 2) && digits(day, 2)) {
        return false;
    }
    let (Ok(year), Ok(month), Ok(day)) = (
        year.parse::<u32>(),
        month.parse::<u32>(),
        day.parse::<u32>(),
    ) else {
        return false;
    };
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days).contains(&day)
}
