//! Walks a template's HTML and records where each Mustache tag sits.
//!
//! This scanner handles only HTML whose tokens do not depend on the parser's
//! tree state: text, start and end tags with their attributes, comments,
//! doctypes, bogus comments, and the content of `<style>` and `<title>` (and
//! of the refused elements whose content is not markup). It follows the
//! WHATWG tokenizer's states for those, byte by byte. It does not model what
//! needs the tree builder: CDATA in foreign content, `<plaintext>`, script
//! data escapes and the switches that depend on scripting. The check refuses
//! each of those constructs instead (`CData`, `ForbiddenElement`), so it
//! fails closed. No maintained tokenizer reports whether an attribute value
//! was quoted (html5ever and html5gum were read), and the check needs that; a
//! tokenizer that reports quoting and source offsets would replace this
//! scanner.

use super::mustache::Tag;
use super::{FlawKind, Found, Place, Site};

/// Elements a template may not hold.
const FORBIDDEN: &[&str] = &[
    "script",
    "base",
    "link",
    "iframe",
    "frame",
    "object",
    "embed",
    "form",
    "input",
    "button",
    "select",
    "textarea",
    "xmp",
    "plaintext",
    "noscript",
    "noembed",
    "noframes",
];

/// Elements whose content the tokenizer reads as text up to their end tag.
const RAW_TEXT: &[&str] = &[
    "style", "title", "script", "xmp", "iframe", "noembed", "noframes", "noscript", "textarea",
];

/// Whether `byte` is whitespace inside a tag.
fn is_space(byte: u8) -> bool {
    matches!(byte, b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
}

/// One tag as the scanner reads it.
#[derive(Debug, Default)]
struct TagReading {
    name: String,
    end_tag: bool,
    attributes: Vec<String>,
}

/// Walks a template's HTML.
struct Scanner<'a> {
    bytes: &'a [u8],
    tags: &'a [Tag],
    next_tag: usize,
    position: usize,
    sites: Vec<Site>,
    found: Vec<Found>,
}

/// Records where each Mustache tag in `tags` sits in `template`, and finds
/// the template's HTML flaws.
pub(crate) fn scan(template: &str, tags: &[Tag]) -> (Vec<Site>, Vec<Found>) {
    let mut scanner = Scanner {
        bytes: template.as_bytes(),
        tags,
        next_tag: 0,
        position: 0,
        sites: Vec::new(),
        found: Vec::new(),
    };
    if let Some(at) = template.find('\0') {
        scanner.flaw(FlawKind::Nul, at);
    }
    scanner.data();
    (scanner.sites, scanner.found)
}

impl Scanner<'_> {
    fn byte(&self, at: usize) -> Option<u8> {
        self.bytes.get(at).copied()
    }

    /// Whether the bytes at `at` are `text`, with ASCII letters compared
    /// without case.
    fn starts_with(&self, at: usize, text: &[u8]) -> bool {
        self.bytes
            .get(at..at + text.len())
            .is_some_and(|slice| slice.eq_ignore_ascii_case(text))
    }

    fn flaw(&mut self, kind: FlawKind, at: usize) {
        self.found.push(Found { kind, at });
    }

    /// When a Mustache tag starts at the current position, records it at
    /// `place`, steps past it and returns true.
    fn mustache(&mut self, place: &Place) -> bool {
        let Some(tag) = self.tags.get(self.next_tag) else {
            return false;
        };
        if tag.start != self.position {
            return false;
        }
        self.sites.push(Site {
            at: tag.start,
            tag: tag.kind,
            place: place.clone(),
        });
        self.position = tag.end;
        self.next_tag += 1;
        true
    }

    /// The data state, up to the end of the template.
    fn data(&mut self) {
        loop {
            if self.mustache(&Place::Text) {
                continue;
            }
            match self.byte(self.position) {
                None => break,
                Some(b'<') => self.less_than(),
                Some(_) => self.position += 1,
            }
        }
        // A Mustache tag that the walk stepped over unseen sits where the
        // scanner cannot say, so it is refused as markup.
        while let Some(tag) = self.tags.get(self.next_tag) {
            self.sites.push(Site {
                at: tag.start,
                tag: tag.kind,
                place: Place::Markup,
            });
            self.next_tag += 1;
        }
    }

    /// The tag open state, at a `<`.
    fn less_than(&mut self) {
        let open = self.position;
        self.position += 1;
        match self.byte(self.position) {
            Some(b'!') => self.declaration(open),
            Some(b'/') => {
                self.position += 1;
                match self.byte(self.position) {
                    Some(byte) if byte.is_ascii_alphabetic() => self.tag(open, true),
                    Some(b'>') => self.position += 1,
                    None => {}
                    Some(_) if self.mustache(&Place::AfterLessThan) => {}
                    Some(_) => self.bogus_comment(),
                }
            }
            Some(b'?') => {
                self.flaw(FlawKind::ProcessingInstruction, open);
                self.bogus_comment();
            }
            Some(byte) if byte.is_ascii_alphabetic() => self.tag(open, false),
            // The `<` is text; a Mustache tag right after it could start a
            // tag once filled.
            _ => {
                self.mustache(&Place::AfterLessThan);
            }
        }
    }

    /// The markup declaration open state, after `<!`.
    fn declaration(&mut self, open: usize) {
        self.position += 1;
        if self.starts_with(self.position, b"--") {
            self.position += 2;
            self.comment(open);
        } else {
            if self.starts_with(self.position, b"[CDATA[") {
                self.flaw(FlawKind::CData, open);
            }
            // A doctype, CDATA outside foreign content and a bogus comment
            // all end at the first `>`.
            self.bogus_comment();
        }
    }

    /// Everything up to the next `>`.
    fn bogus_comment(&mut self) {
        self.until(|byte| byte == b'>', &Place::Comment);
        if self.byte(self.position).is_some() {
            self.position += 1;
        }
    }

    /// A comment, after `<!--`.
    fn comment(&mut self, open: usize) {
        if self.starts_with(self.position, b">") {
            self.flaw(FlawKind::MalformedComment, open);
            self.position += 1;
            return;
        }
        if self.starts_with(self.position, b"->") {
            self.flaw(FlawKind::MalformedComment, open);
            self.position += 2;
            return;
        }
        loop {
            if self.mustache(&Place::Comment) {
                continue;
            }
            if self.byte(self.position).is_none() {
                self.flaw(FlawKind::Unterminated, open);
                return;
            }
            if self.starts_with(self.position, b"-->") {
                self.position += 3;
                return;
            }
            if self.starts_with(self.position, b"--!>") {
                self.flaw(FlawKind::MalformedComment, self.position);
                self.position += 4;
                return;
            }
            if self.starts_with(self.position, b"<!--") {
                self.flaw(FlawKind::MalformedComment, self.position);
                self.position += 4;
                continue;
            }
            self.position += 1;
        }
    }

    /// A start or end tag, from its name to its `>`.
    fn tag(&mut self, open: usize, end_tag: bool) {
        let mut reading = TagReading {
            end_tag,
            ..TagReading::default()
        };
        let name_start = self.position;
        self.until(
            |byte| is_space(byte) || matches!(byte, b'/' | b'>'),
            &Place::Markup,
        );
        reading.name = self.lower(name_start, self.position);
        loop {
            self.until(|byte| !is_space(byte), &Place::Markup);
            match self.byte(self.position) {
                None => {
                    self.flaw(FlawKind::Unterminated, open);
                    return;
                }
                Some(b'>') => {
                    self.position += 1;
                    self.emit(open, &reading);
                    return;
                }
                Some(b'/') => self.position += 1,
                Some(_) => self.attribute(&mut reading),
            }
        }
    }

    /// One attribute: its name and, when it has one, its value.
    fn attribute(&mut self, reading: &mut TagReading) {
        let start = self.position;
        // The first character belongs to the name even when it is `=`.
        if !self.mustache(&Place::Markup) {
            self.position += 1;
        }
        self.until(
            |byte| is_space(byte) || matches!(byte, b'/' | b'>' | b'='),
            &Place::Markup,
        );
        let name = self.lower(start, self.position);
        if reading.attributes.contains(&name) {
            self.flaw(
                FlawKind::DuplicateAttribute {
                    attribute: name.clone(),
                },
                start,
            );
        }
        if name.starts_with("on") {
            self.flaw(
                FlawKind::EventHandler {
                    attribute: name.clone(),
                },
                start,
            );
        }
        reading.attributes.push(name.clone());
        self.until(|byte| !is_space(byte), &Place::Markup);
        if self.byte(self.position) != Some(b'=') {
            return;
        }
        self.position += 1;
        while self.byte(self.position).is_some_and(is_space) {
            self.position += 1;
        }
        match self.byte(self.position) {
            Some(quote @ (b'"' | b'\'')) => {
                self.position += 1;
                let place = if quote == b'"' {
                    Place::DoubleQuoted {
                        element: reading.name.clone(),
                        attribute: name,
                        end_tag: reading.end_tag,
                    }
                } else {
                    Place::SingleQuoted
                };
                self.until(|byte| byte == quote, &place);
                if self.byte(self.position).is_some() {
                    self.position += 1;
                }
            }
            Some(b'>') | None => {}
            Some(_) => self.until(|byte| is_space(byte) || byte == b'>', &Place::Unquoted),
        }
    }

    /// Steps forward until `stop` holds for the current byte or the template
    /// ends, recording each Mustache tag on the way at `place`.
    fn until(&mut self, stop: impl Fn(u8) -> bool, place: &Place) {
        loop {
            if self.mustache(place) {
                continue;
            }
            match self.byte(self.position) {
                Some(byte) if !stop(byte) => self.position += 1,
                _ => return,
            }
        }
    }

    /// The bytes from `start` to `end`, with ASCII letters in lower case.
    fn lower(&self, start: usize, end: usize) -> String {
        let bytes = self.bytes.get(start..end).unwrap_or_default();
        String::from_utf8_lossy(bytes).to_ascii_lowercase()
    }

    /// Judges a whole start tag, then reads the content of an element whose
    /// content is not markup.
    fn emit(&mut self, open: usize, reading: &TagReading) {
        if reading.end_tag {
            return;
        }
        let name = reading.name.as_str();
        let refresh = name == "meta" && reading.attributes.iter().any(|key| key == "http-equiv");
        if FORBIDDEN.contains(&name) || refresh {
            self.flaw(
                FlawKind::ForbiddenElement {
                    element: reading.name.clone(),
                },
                open,
            );
        }
        if name == "plaintext" {
            self.until(|_| false, &Place::RawText);
        } else if RAW_TEXT.contains(&name) {
            self.raw_text(name);
        }
    }

    /// The content of a raw text element, up to its end tag.
    fn raw_text(&mut self, element: &str) {
        let mut end = b"</".to_vec();
        end.extend_from_slice(element.as_bytes());
        loop {
            self.until(|byte| byte == b'<', &Place::RawText);
            if self.byte(self.position).is_none() {
                return;
            }
            let after = self.position + end.len();
            let closes = self.starts_with(self.position, &end)
                && self
                    .byte(after)
                    .is_none_or(|byte| is_space(byte) || matches!(byte, b'/' | b'>'));
            if closes {
                self.less_than();
                return;
            }
            self.flaw(FlawKind::LessThanInRawText, self.position);
            self.position += 1;
        }
    }
}
