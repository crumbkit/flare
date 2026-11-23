//! The publish-time check of a page's template (threats 9, 10, 13, 14 and
//! 21).
//!
//! The check reads the template in three passes: [`mustache`] finds the
//! Mustache tags, [`markup`] walks the HTML and records where each tag sits,
//! and [`tree`] nests the sections. [`judge`] then refuses each tag that sits
//! where a record value could change the markup.

mod judge;
mod markup;
mod mustache;
mod tree;

pub(crate) use mustache::TagKind;
pub(crate) use tree::Node;

use flare_core::{ContentFormat, Page};
use std::fmt;

/// One thing wrong with a template, at a line and a column, both counted
/// from 1. The column counts characters.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Flaw {
    kind: FlawKind,
    line: usize,
    column: usize,
}

impl Flaw {
    /// What is wrong.
    #[must_use]
    pub fn kind(&self) -> &FlawKind {
        &self.kind
    }

    /// The line it is on, counted from 1.
    #[must_use]
    pub fn line(&self) -> usize {
        self.line
    }

    /// The column it starts at, counted in characters from 1.
    #[must_use]
    pub fn column(&self) -> usize {
        self.column
    }
}

impl fmt::Display for Flaw {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "line {}, column {}: {}",
            self.line, self.column, self.kind
        )
    }
}

/// The kinds of flaw the template check finds.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum FlawKind {
    /// A Mustache tag other than a variable, section, inverted section,
    /// close or comment tag: `{{{`, `{{&`, `{{>`, `{{=`, `{{<` or `{{$`.
    ForbiddenMustacheTag,
    /// A Mustache tag that is never closed, holds `{{` or a character outside
    /// ASCII, or names something with characters other than letters, digits,
    /// `_`, `-` and `.`.
    MalformedMustacheTag,
    /// A Mustache tag whose name is longer than 256 bytes.
    TagNameTooLong {
        /// The longest name in bytes.
        max: usize,
    },
    /// A Mustache tag inside an HTML tag but outside an attribute value in
    /// double quotes, or in an end tag.
    TagInMarkup,
    /// A Mustache tag in an attribute value in single quotes, which a record
    /// value holding `'` would end.
    TagInSingleQuotedValue,
    /// A Mustache tag in an attribute value without quotes.
    TagInUnquotedValue,
    /// A section, inverted section or close tag inside an HTML tag.
    SectionInMarkup,
    /// A Mustache tag in an HTML comment, a doctype or another declaration.
    TagInComment,
    /// A Mustache tag in the content of a `<style>` or `<title>` element.
    TagInRawText,
    /// A Mustache tag in the value of a URL attribute, of `style`, or of an
    /// SVG animation attribute that can set a URL.
    TagInUrlOrStyle {
        /// The attribute, in lower case.
        attribute: String,
    },
    /// A Mustache tag directly after `<` or `</`, where a record value could
    /// start an HTML tag.
    TagAfterLessThan,
    /// An event handler attribute, such as `onclick`.
    EventHandler {
        /// The attribute, in lower case.
        attribute: String,
    },
    /// An element a template may not hold, such as `script`, a form control,
    /// or `meta` with `http-equiv`.
    ForbiddenElement {
        /// The element, in lower case.
        element: String,
    },
    /// `<` inside the content of a `<style>` or `<title>` element, other
    /// than the element's end tag.
    LessThanInRawText,
    /// `<![CDATA[`.
    CData,
    /// `<?`.
    ProcessingInstruction,
    /// A comment that opens with `<!-->` or `<!--->`, or holds `<!--` or
    /// `--!>`.
    MalformedComment,
    /// A NUL character.
    Nul,
    /// A tag or comment that the template ends inside.
    Unterminated,
    /// An attribute given twice in one tag.
    DuplicateAttribute {
        /// The attribute, in lower case.
        attribute: String,
    },
    /// A section that is not closed, or a close tag that closes no open
    /// section of its name.
    UnbalancedSection {
        /// The section's name.
        name: String,
    },
    /// A section nested more than 8 deep.
    TooDeep,
}

impl fmt::Display for FlawKind {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ForbiddenMustacheTag => formatter.write_str(
                "only variable, section, inverted section, close and comment tags are allowed",
            ),
            Self::MalformedMustacheTag => formatter.write_str(
                "the Mustache tag is unclosed or names something with characters other than letters, digits, `_`, `-` and `.`",
            ),
            Self::TagNameTooLong { max } => {
                write!(formatter, "a Mustache tag's name can be at most {max} bytes")
            }
            Self::TagInMarkup => formatter.write_str(
                "a Mustache tag inside an HTML tag must be in an attribute value in double quotes",
            ),
            Self::TagInSingleQuotedValue => formatter.write_str(
                "a Mustache tag in an attribute value must be in double quotes, because a value holding ' would end single quotes",
            ),
            Self::TagInUnquotedValue => {
                formatter.write_str("a Mustache tag in an attribute value must be in double quotes")
            }
            Self::SectionInMarkup => formatter
                .write_str("a section, inverted section or close tag cannot be inside an HTML tag"),
            Self::TagInComment => formatter
                .write_str("a Mustache tag cannot be inside an HTML comment or declaration"),
            Self::TagInRawText => formatter
                .write_str("a Mustache tag cannot be inside a <style> or <title> element"),
            Self::TagInUrlOrStyle { attribute } => write!(
                formatter,
                "a Mustache tag cannot be in the {attribute} attribute, because a record value would choose a URL or a style"
            ),
            Self::TagAfterLessThan => formatter.write_str(
                "a Mustache tag cannot follow < directly, because a record value could start an HTML tag",
            ),
            Self::EventHandler { attribute } => {
                write!(formatter, "the event handler attribute {attribute} is not allowed")
            }
            Self::ForbiddenElement { element } => {
                write!(formatter, "the element {element} is not allowed")
            }
            Self::LessThanInRawText => formatter.write_str(
                "< cannot appear inside a <style> or <title> element except in its end tag",
            ),
            Self::CData => formatter.write_str("<![CDATA[ is not allowed"),
            Self::ProcessingInstruction => formatter.write_str("<? is not allowed"),
            Self::MalformedComment => formatter.write_str(
                "a comment cannot open with <!--> or <!---> or hold <!-- or --!>",
            ),
            Self::Nul => formatter.write_str("the template holds a NUL character"),
            Self::Unterminated => formatter.write_str("the template ends inside a tag or comment"),
            Self::DuplicateAttribute { attribute } => {
                write!(formatter, "the attribute {attribute} is given twice")
            }
            Self::UnbalancedSection { name } => write!(
                formatter,
                "the section {name} is not closed, or is closed without being opened"
            ),
            Self::TooDeep => formatter.write_str("sections are nested more than 8 deep"),
        }
    }
}

/// The flaws in a template, in the order they appear. It is never empty.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("the template has {} flaws, the first at {}", .0.len(), first(.0))]
#[non_exhaustive]
pub struct TemplateFlaws(Vec<Flaw>);

fn first(flaws: &[Flaw]) -> String {
    flaws.first().map(Flaw::to_string).unwrap_or_default()
}

impl TemplateFlaws {
    /// The flaws, in the order they appear in the template.
    pub fn iter(&self) -> impl Iterator<Item = &Flaw> {
        self.0.iter()
    }

    /// The number of flaws.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Always false: a `TemplateFlaws` holds at least one flaw.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// A flaw found at a byte offset, before it gets its line and column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Found {
    pub(crate) kind: FlawKind,
    pub(crate) at: usize,
}

/// Where the HTML puts a Mustache tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Place {
    /// In text.
    Text,
    /// Directly after `<` or `</`.
    AfterLessThan,
    /// In an attribute value in double quotes.
    DoubleQuoted {
        element: String,
        attribute: String,
        end_tag: bool,
    },
    /// In an attribute value in single quotes.
    SingleQuoted,
    /// In an attribute value without quotes.
    Unquoted,
    /// Elsewhere inside an HTML tag.
    Markup,
    /// In a comment, a doctype or another declaration.
    Comment,
    /// In the content of an element whose content is not markup, such as
    /// `<style>`.
    RawText,
}

/// One Mustache tag and where it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Site {
    pub(crate) at: usize,
    pub(crate) tag: TagKind,
    pub(crate) place: Place,
}

/// What the three passes found in one template.
#[derive(Debug)]
pub(crate) struct Scan {
    pub(crate) sites: Vec<Site>,
    pub(crate) nodes: Vec<Node>,
    found: Vec<Found>,
}

/// Reads a template: its Mustache tags, where each sits, its section tree
/// and the flaws those passes find.
pub(crate) fn scan(template: &str) -> Scan {
    let (tags, mut found) = mustache::lex(template);
    let (sites, markup_found) = markup::scan(template, &tags);
    let (nodes, tree_found) = tree::build(template.len(), &tags);
    found.extend(markup_found);
    found.extend(tree_found);
    Scan {
        sites,
        nodes,
        found,
    }
}

/// Checks a page's template before it is published.
///
/// A Markdown page is not a template, so it always passes. An HTML page
/// passes when its HTML has tokens that do not depend on the parser's tree
/// state and each Mustache tag sits in text or in an attribute value in
/// double quotes that names no URL or style.
///
/// # Errors
///
/// Returns every [`Flaw`] the check finds.
pub fn check_page(page: &Page) -> Result<(), TemplateFlaws> {
    template(page).map(|_| ())
}

/// The section tree of an HTML page's template, or `None` for a Markdown
/// page.
pub(crate) fn template(page: &Page) -> Result<Option<Vec<Node>>, TemplateFlaws> {
    let content = page.content();
    if content.format() != ContentFormat::Html {
        return Ok(None);
    }
    let text = content.text();
    let scanned = scan(text);
    let mut found = scanned.found;
    found.extend(judge::judge(&scanned.sites));
    if found.is_empty() {
        return Ok(Some(scanned.nodes));
    }
    found.sort_by_key(|flaw| flaw.at);
    Err(TemplateFlaws(
        found.into_iter().map(|flaw| locate(text, flaw)).collect(),
    ))
}

/// Gives a flaw its line and column.
fn locate(text: &str, flaw: Found) -> Flaw {
    let (line, column) = line_and_column(text, flaw.at);
    Flaw {
        kind: flaw.kind,
        line,
        column,
    }
}

/// The line and the column, both counted from 1, of the byte offset `at`.
/// The column counts characters.
pub(crate) fn line_and_column(text: &str, at: usize) -> (usize, usize) {
    let before = text.get(..at).unwrap_or(text);
    let line_start = before.rfind('\n').map_or(0, |newline| newline + 1);
    let line = before.matches('\n').count() + 1;
    let column = before
        .get(line_start..)
        .map_or(0, |rest| rest.chars().count())
        + 1;
    (line, column)
}

#[cfg(test)]
mod tests;
