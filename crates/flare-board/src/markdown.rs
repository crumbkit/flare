//! A typed Markdown record, such as a task, as one JSON object, and back.
//!
//! Board reads a Markdown record's front matter as its record and keeps the
//! text after the closing line as its body. flare sees the record as one
//! JSON object: each front-matter field is a key, and the text after the
//! closing line goes under the reserved key [`BODY`]. flare-core's
//! `FieldName` never starts with `$`, so no editable field collides with it.
//!
//! On a write the object becomes a file again: `---`, one `key: <JSON>`
//! line per field, `---`, then the prose unchanged. JSON values are YAML
//! 1.2 flow values, so board reads each line back as the value written.
//! `$type` comes first, then the fields in the order the current file has
//! them, then fields new to the file in sorted order.

use serde_json::{Map, Value};

/// The key that holds a Markdown record's prose in its JSON object.
pub(crate) const BODY: &str = "$body";

/// Why an object cannot be written as a Markdown record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum MarkdownError {
    /// The object has no [`BODY`] string.
    NoBody,
    /// The object has no `$type`.
    NoType,
}

impl MarkdownError {
    pub(crate) fn text(&self) -> &'static str {
        match self {
            Self::NoBody => "a Markdown record's body holds its prose as a string under $body",
            Self::NoType => "a Markdown record's body names its $type",
        }
    }
}

/// Splits a Markdown file into its front-matter block and the text after
/// the block's closing line, as board does: the first line is exactly `---`
/// and the block ends at the next line that is exactly `---` or `...`.
/// Answers `None` for a file with no such block.
pub(crate) fn split(text: &str) -> Option<(&str, &str)> {
    let rest = text.strip_prefix("---\n")?;
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        let bare = line.strip_suffix('\n').unwrap_or(line);
        if bare == "---" || bare == "..." {
            let block = rest.get(..offset)?;
            let after = rest.get(offset.checked_add(line.len())?..)?;
            return Some((block, after));
        }
        offset = offset.checked_add(line.len())?;
    }
    None
}

/// The top-level keys of a front-matter block, in the order the file has
/// them.
///
/// Shortcut: a key is the text before the first `:` of a line that starts
/// at the left margin and is not a comment or a list item, read as JSON
/// when it is double-quoted. That handles the keys board's own writers and
/// this module write. It misses single-quoted keys, keys with escapes and
/// complex keys, and a missed key is placed among the new fields in sorted
/// order, which changes only where the field's line sits.
pub(crate) fn field_order(block: &str) -> Vec<String> {
    let mut keys = Vec::new();
    for line in block.lines() {
        if line.is_empty() || line.starts_with([' ', '\t', '-', '#']) {
            continue;
        }
        let key = if line.starts_with('"') {
            line.find("\":")
                .and_then(|end| line.get(..=end))
                .and_then(|quoted| serde_json::from_str::<String>(quoted).ok())
        } else {
            line.split_once(':')
                .map(|(key, _)| key.trim_end().to_owned())
        };
        if let Some(key) = key {
            keys.push(key);
        }
    }
    keys
}

/// The Markdown file for `object`, with the fields of the current file in
/// `order` first, after `$type`.
///
/// # Errors
///
/// Returns [`MarkdownError::NoBody`] when [`BODY`] is not a string and
/// [`MarkdownError::NoType`] when `$type` is missing.
pub(crate) fn from_object(
    object: &Map<String, Value>,
    order: &[String],
) -> Result<String, MarkdownError> {
    let Some(Value::String(body)) = object.get(BODY) else {
        return Err(MarkdownError::NoBody);
    };
    let record_type = object.get("$type").ok_or(MarkdownError::NoType)?;
    let mut file = String::from("---\n");
    push_line(&mut file, "$type", record_type);
    let kept = order
        .iter()
        .filter(|key| key.as_str() != "$type" && key.as_str() != BODY);
    for key in kept {
        if let Some(value) = object.get(key) {
            push_line(&mut file, key, value);
        }
    }
    let mut new: Vec<(&String, &Value)> = object
        .iter()
        .filter(|(key, _)| key.as_str() != "$type" && key.as_str() != BODY)
        .filter(|(key, _)| !order.contains(key))
        .collect();
    new.sort_by(|left, right| left.0.cmp(right.0));
    for (key, value) in new {
        push_line(&mut file, key, value);
    }
    file.push_str("---\n");
    file.push_str(body);
    Ok(file)
}

/// The JSON object of a Markdown record whose front matter board read as
/// `record`, with `prose` under [`BODY`].
pub(crate) fn to_object(record: Map<String, Value>, prose: &str) -> Map<String, Value> {
    let mut object = record;
    object.insert(BODY.to_owned(), Value::String(prose.to_owned()));
    object
}

/// Appends `key: <JSON value>` and a newline. A key of letters, digits,
/// `_`, `-` and a leading `$` stays plain, and any other key is written as
/// a JSON string.
fn push_line(file: &mut String, key: &str, value: &Value) {
    let plain = key.char_indices().all(|(index, character)| {
        character.is_ascii_alphanumeric()
            || character == '_'
            || (index > 0 && character == '-')
            || (index == 0 && character == '$')
    });
    if plain && !key.is_empty() {
        file.push_str(key);
    } else {
        file.push_str(&Value::String(key.to_owned()).to_string());
    }
    file.push_str(": ");
    file.push_str(&value.to_string());
    file.push('\n');
}

#[cfg(test)]
mod tests;
