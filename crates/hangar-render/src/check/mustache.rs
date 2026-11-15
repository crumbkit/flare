//! Finds the Mustache tags of a template, as mustache2 reads them with its
//! default `{{` and `}}` delimiters.

use super::{FlawKind, Found};

/// The kinds of Mustache tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TagKind {
    /// `{{name}}`.
    Variable,
    /// `{{#name}}`.
    Section,
    /// `{{^name}}`.
    Inverted,
    /// `{{/name}}`.
    Close,
    /// `{{! text }}`.
    Comment,
    /// A tag the check refuses, kept so that the HTML pass skips it.
    Refused,
}

/// One Mustache tag: its byte range, its kind and the name it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tag {
    pub(crate) start: usize,
    pub(crate) end: usize,
    pub(crate) kind: TagKind,
    pub(crate) name: String,
}

/// Finds every Mustache tag of a template, and the flaw of each tag the
/// check refuses.
///
/// A tag runs from `{{` to the first `}}` after it. mustache2 counts a `{{`
/// inside a tag as a nested opener and steps through the tag byte by byte,
/// so a tag that holds `{{` or a character outside ASCII is refused: the
/// first would end elsewhere than this function says, and the second makes
/// mustache2 slice inside a character. A `{{` without a `}}` ends the search,
/// because mustache2 reads the rest of the template as that one tag.
pub(crate) fn lex(template: &str) -> (Vec<Tag>, Vec<Found>) {
    let mut tags = Vec::new();
    let mut found = Vec::new();
    let mut from = 0;
    while let Some(offset) = template.get(from..).and_then(|rest| rest.find("{{")) {
        let start = from + offset;
        let inner = start + 2;
        let Some(length) = template.get(inner..).and_then(|rest| rest.find("}}")) else {
            found.push(Found {
                kind: FlawKind::MalformedMustacheTag,
                at: start,
            });
            break;
        };
        let end = inner + length + 2;
        let content = template.get(inner..inner + length).unwrap_or_default();
        let (kind, name) = match classify(content) {
            Ok(tag) => tag,
            Err(kind) => {
                found.push(Found { kind, at: start });
                (TagKind::Refused, String::new())
            }
        };
        tags.push(Tag {
            start,
            end,
            kind,
            name,
        });
        from = end;
    }
    (tags, found)
}

/// Reads the text between `{{` and `}}` as mustache2 does.
fn classify(content: &str) -> Result<(TagKind, String), FlawKind> {
    if !content.is_ascii() || content.contains("{{") {
        return Err(FlawKind::MalformedMustacheTag);
    }
    let mut characters = content.chars();
    let kind = match characters.next() {
        Some('{' | '&' | '>' | '=' | '<' | '$') => return Err(FlawKind::ForbiddenMustacheTag),
        Some('!') => return Ok((TagKind::Comment, String::new())),
        Some('#') => TagKind::Section,
        Some('^') => TagKind::Inverted,
        Some('/') => TagKind::Close,
        _ => TagKind::Variable,
    };
    let name = if kind == TagKind::Variable {
        content.trim()
    } else {
        characters.as_str().trim()
    };
    if well_named(name) {
        Ok((kind, name.to_owned()))
    } else {
        Err(FlawKind::MalformedMustacheTag)
    }
}

/// Whether `name` is `.` or dot-separated segments of ASCII letters, digits,
/// `_` and `-`.
fn well_named(name: &str) -> bool {
    name == "."
        || name.split('.').all(|segment| {
            !segment.is_empty()
                && segment.chars().all(|character| {
                    character.is_ascii_alphanumeric() || matches!(character, '_' | '-')
                })
        })
}
