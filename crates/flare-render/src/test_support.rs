//! Fixtures and an HTML reader that the unit tests share.

use crate::{FormTarget, FormToken, LocalPath, Schema};
use flare_core::{
    BindingName, Blob, ContentFormat, FieldName, Page, PageContent, PageName, Record, RecordBody,
    RecordPath, RecordType, Sample, Slot, SlotName, Slots,
};
use html5ever::tokenizer::states::RawKind;
use html5ever::tokenizer::{
    BufferQueue, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts,
};
use std::cell::RefCell;

/// The task type the fixtures use.
pub(crate) const TASK: &str = "tasks/task/v2";

/// A blob of 40 copies of `digit`.
pub(crate) fn blob(digit: char) -> Blob {
    Blob::try_from(digit.to_string().repeat(40).as_str()).unwrap()
}

/// A record of type `TASK` at `path` with blob `digit` and `body`.
pub(crate) fn record(path: &str, digit: char, body: &str) -> Record {
    Record::new(
        RecordPath::try_from(path).unwrap(),
        RecordType::try_from(TASK).unwrap(),
        blob(digit),
        RecordBody::try_from(body).unwrap(),
    )
}

/// A slot of type `TASK` that lets a form edit `editable`.
pub(crate) fn slot(name: &str, editable: &[&str], sample: Option<&str>) -> Slot {
    Slot::new(
        SlotName::try_from(name).unwrap(),
        RecordType::try_from(TASK).unwrap(),
        editable
            .iter()
            .map(|field| FieldName::try_from(*field).unwrap())
            .collect(),
        sample.map(|body| {
            Sample::new(
                RecordType::try_from(TASK).unwrap(),
                RecordBody::try_from(body).unwrap(),
            )
        }),
    )
    .unwrap()
}

/// An HTML page with `template` and `slots`.
pub(crate) fn html_page(template: &str, slots: Vec<Slot>) -> Page {
    Page::new(
        PageName::try_from("task-card").unwrap(),
        PageContent::new(ContentFormat::Html, template).unwrap(),
        Slots::new(slots).unwrap(),
    )
}

/// A Markdown page with `text` and no slot.
pub(crate) fn markdown_page(text: &str) -> Page {
    Page::new(
        PageName::try_from("notes").unwrap(),
        PageContent::new(ContentFormat::Markdown, text).unwrap(),
        Slots::new(Vec::new()).unwrap(),
    )
}

/// The slot name `name`.
pub(crate) fn slot_name(name: &str) -> SlotName {
    SlotName::try_from(name).unwrap()
}

/// The field name `name`.
pub(crate) fn field(name: &str) -> FieldName {
    FieldName::try_from(name).unwrap()
}

/// A form target that posts to `/b/flare-plan/edit` with `token`.
pub(crate) fn target(token: &str) -> FormTarget {
    FormTarget::new(
        LocalPath::try_from("/b/flare-plan/edit").unwrap(),
        FormToken::new(token),
        BindingName::try_from("flare-plan").unwrap(),
    )
}

/// The schema of a task with a title, a status, a body, a done flag, an
/// estimate and a due date, and some shapes no form edits.
pub(crate) fn task_schema() -> Schema {
    Schema::parse(
        r##"{
          "type": "object",
          "required": ["title", "status"],
          "properties": {
            "$type": {"const": "tasks/task/v2", "type": "string"},
            "title": {"type": "string", "title": "Title", "description": "What the task is"},
            "status": {"type": "string", "title": "Status", "enum": ["open", "done", "deferred", "dropped"]},
            "body": {"type": "string", "x-board-prose": true},
            "notes": {"type": "string"},
            "flagged": {"type": "boolean", "title": "Flagged"},
            "estimate": {"type": "integer", "minimum": 0, "maximum": 100, "title": "Estimate"},
            "weight": {"type": "number", "multipleOf": 0.5},
            "due": {"type": "string", "format": "date", "title": "Due"},
            "tags": {"type": "array", "items": {"type": "string"}},
            "owner": {"$ref": "#/$defs/Actor"}
          }
        }"##,
    )
    .unwrap()
}

/// One token of HTML as a browser's tokenizer reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Tok {
    Start {
        name: String,
        attributes: Vec<(String, String)>,
    },
    End {
        name: String,
    },
    Text(String),
    Comment(String),
    Doctype,
}

struct Sink(RefCell<Vec<Tok>>);

impl TokenSink for Sink {
    type Handle = ();

    fn process_token(&self, token: Token, _line: u64) -> TokenSinkResult<()> {
        let mut tokens = self.0.borrow_mut();
        match token {
            Token::TagToken(tag) => {
                let name = tag.name.to_string();
                if tag.kind == TagKind::EndTag {
                    tokens.push(Tok::End { name });
                    return TokenSinkResult::Continue;
                }
                let attributes = tag
                    .attrs
                    .iter()
                    .map(|attribute| {
                        (
                            attribute.name.local.to_string(),
                            attribute.value.to_string(),
                        )
                    })
                    .collect();
                tokens.push(Tok::Start {
                    name: name.clone(),
                    attributes,
                });
                // The tree builder switches the tokenizer for these elements
                // in HTML content; the sink does the same.
                match name.as_str() {
                    "style" | "xmp" | "iframe" | "noembed" | "noframes" => {
                        return TokenSinkResult::RawData(RawKind::Rawtext);
                    }
                    "title" | "textarea" => return TokenSinkResult::RawData(RawKind::Rcdata),
                    "script" => return TokenSinkResult::RawData(RawKind::ScriptData),
                    "plaintext" => return TokenSinkResult::Plaintext,
                    _ => {}
                }
            }
            Token::CharacterTokens(text) => match tokens.last_mut() {
                Some(Tok::Text(previous)) => previous.push_str(&text),
                _ => tokens.push(Tok::Text(text.to_string())),
            },
            Token::NullCharacterToken => tokens.push(Tok::Text("\0".to_owned())),
            Token::CommentToken(text) => tokens.push(Tok::Comment(text.to_string())),
            Token::DoctypeToken(_) => tokens.push(Tok::Doctype),
            Token::EOFToken | Token::ParseError(_) => {}
        }
        TokenSinkResult::Continue
    }
}

/// The tokens of `html`, with adjacent text joined.
pub(crate) fn tokens(html: &str) -> Vec<Tok> {
    let tokenizer = Tokenizer::new(Sink(RefCell::new(Vec::new())), TokenizerOpts::default());
    let input = BufferQueue::default();
    input.push_back(html.into());
    let _ = tokenizer.feed(&input);
    tokenizer.end();
    tokenizer.sink.0.take()
}

/// The start tags of `html`, with their attributes.
pub(crate) fn start_tags(html: &str) -> Vec<(String, Vec<(String, String)>)> {
    tokens(html)
        .into_iter()
        .filter_map(|token| match token {
            Tok::Start { name, attributes } => Some((name, attributes)),
            _ => None,
        })
        .collect()
}

/// The text of `html`, joined.
pub(crate) fn text_of(html: &str) -> String {
    tokens(html)
        .into_iter()
        .filter_map(|token| match token {
            Tok::Text(text) => Some(text),
            _ => None,
        })
        .collect()
}

/// The pairs a browser would post from the first form in `html`, with no
/// field changed: each named input's value (a checkbox only when checked),
/// each textarea's text and each select's selected option. As a browser
/// does, it turns every LF and lone CR in a value into CRLF.
pub(crate) fn submit(html: &str) -> Vec<(String, String)> {
    drawn_pairs(html)
        .into_iter()
        .map(|(name, value)| (name, crlf(&value)))
        .collect()
}

/// `text` with each LF, CR and CRLF line break made CRLF.
pub(crate) fn crlf(text: &str) -> String {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
}

/// The bytes of `pairs` as an `application/x-www-form-urlencoded` body:
/// `*`, `-`, `.`, `_`, ASCII letters and digits stay one byte, a space
/// becomes `+`, and every other byte becomes `%XX`.
pub(crate) fn urlencoded_len(pairs: &[(String, String)]) -> usize {
    let encoded = |text: &str| -> usize {
        text.bytes()
            .map(|byte| {
                if byte.is_ascii_alphanumeric() || b"*-._ ".contains(&byte) {
                    1
                } else {
                    3
                }
            })
            .sum()
    };
    let joins = pairs.len().saturating_sub(1);
    pairs
        .iter()
        .map(|(name, value)| encoded(name) + 1 + encoded(value))
        .sum::<usize>()
        + joins
}

fn drawn_pairs(html: &str) -> Vec<(String, String)> {
    let mut pairs = Vec::new();
    let mut open: Option<(String, String)> = None;
    let mut select: Option<String> = None;
    for token in tokens(html) {
        match token {
            Tok::Start { name, attributes } => {
                let get = |key: &str| {
                    attributes
                        .iter()
                        .find(|(name, _)| name == key)
                        .map(|(_, value)| value.clone())
                };
                match name.as_str() {
                    "input" => {
                        let checkbox = get("type").as_deref() == Some("checkbox");
                        if let Some(input) = get("name")
                            && (!checkbox || get("checked").is_some())
                        {
                            pairs.push((input, get("value").unwrap_or_default()));
                        }
                    }
                    "textarea" => open = get("name").map(|name| (name, String::new())),
                    "select" => select = get("name"),
                    "option" if get("selected").is_some() => {
                        if let Some(name) = &select {
                            pairs.push((name.clone(), get("value").unwrap_or_default()));
                        }
                    }
                    _ => {}
                }
            }
            Tok::Text(text) => {
                if let Some((_, value)) = &mut open {
                    value.push_str(&text);
                }
            }
            Tok::End { name } if name == "textarea" => {
                // A browser's tree builder drops one newline that directly
                // follows <textarea>.
                if let Some((name, value)) = open.take() {
                    let value = value
                        .strip_prefix('\n')
                        .map_or(value.clone(), str::to_owned);
                    pairs.push((name, value));
                }
            }
            Tok::End { name } if name == "select" => select = None,
            _ => {}
        }
    }
    pairs
}

/// `pairs` with the value of `key` replaced by `value`, or added when absent.
pub(crate) fn with(
    mut pairs: Vec<(String, String)>,
    key: &str,
    value: &str,
) -> Vec<(String, String)> {
    match pairs.iter_mut().find(|(name, _)| name == key) {
        Some(pair) => pair.1 = value.to_owned(),
        None => pairs.push((key.to_owned(), value.to_owned())),
    }
    pairs
}

/// `pairs` without `key`.
pub(crate) fn without(pairs: Vec<(String, String)>, key: &str) -> Vec<(String, String)> {
    pairs.into_iter().filter(|(name, _)| name != key).collect()
}
