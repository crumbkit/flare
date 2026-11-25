//! [`first_heading`]: the plain text of a Markdown document's first
//! level-one heading, which `flare share` uses as a title.

use crate::markdown::OPTIONS;
use flare_core::MAX_TITLE;
use pulldown_cmark::{Event, HeadingLevel, Parser, Tag, TagEnd};

/// The plain text of the first level-one heading in `markdown`, or `None`
/// when there is no level-one heading or the first one has no text.
///
/// The document is parsed with the options the renderer uses, so a `#`
/// line inside a code block or an HTML block is not a heading, and a
/// Setext heading (a line over `===`) is one. The text keeps the words of
/// emphasis, code spans and links without their markup, and leaves out
/// image alternative text. Inline HTML tags are left out and the text
/// between them is kept, so `# <span>Plan</span>` gives `Plan`. Each run
/// of whitespace becomes one space, other control characters are removed,
/// and the ends are trimmed. A text over [`MAX_TITLE`] bytes is cut at the last character
/// boundary at or below it, so the server accepts it as a title.
#[must_use]
pub fn first_heading(markdown: &str) -> Option<String> {
    let mut events = Parser::new_ext(markdown, OPTIONS).skip_while(|event| {
        !matches!(
            event,
            Event::Start(Tag::Heading {
                level: HeadingLevel::H1,
                ..
            })
        )
    });
    events.next()?;
    let mut raw = String::new();
    let mut images = 0_usize;
    for event in events {
        match event {
            Event::End(TagEnd::Heading(_)) => break,
            Event::Start(Tag::Image { .. }) => images += 1,
            Event::End(TagEnd::Image) => images = images.saturating_sub(1),
            Event::Text(text) | Event::Code(text) if images == 0 => raw.push_str(&text),
            Event::SoftBreak | Event::HardBreak => raw.push(' '),
            _ => {}
        }
    }
    let text = plain(&raw);
    let title = cut(&text);
    (!title.is_empty()).then(|| title.to_owned())
}

/// `text` with each run of whitespace made one space, other control
/// characters removed and the ends trimmed.
fn plain(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            space = true;
        } else if !character.is_control() {
            if space && !out.is_empty() {
                out.push(' ');
            }
            space = false;
            out.push(character);
        }
    }
    out
}

/// `text` cut at the last character boundary at or below [`MAX_TITLE`]
/// bytes, with any space the cut leaves at the end trimmed.
fn cut(text: &str) -> &str {
    text[..text.floor_char_boundary(MAX_TITLE)].trim_end()
}

#[cfg(test)]
mod tests;
