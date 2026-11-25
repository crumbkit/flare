//! The top-level blocks of a Markdown document, the captions that may
//! follow them, and the diagrams a fenced block may draw.

use super::DIAGRAM_TYPES;
use pulldown_cmark::{Event, Tag};

/// The ranges of the top-level blocks in `events`.
pub(super) fn top_level_blocks(events: &[Event<'_>]) -> Vec<(usize, usize)> {
    let mut blocks = Vec::new();
    let mut depth = 0_usize;
    let mut start = 0;
    for (index, event) in events.iter().enumerate() {
        match event {
            Event::Start(_) => {
                if depth == 0 {
                    start = index;
                }
                depth += 1;
            }
            Event::End(_) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    blocks.push((start, index + 1));
                }
            }
            _ if depth == 0 => blocks.push((index, index + 1)),
            _ => {}
        }
    }
    blocks
}

/// Whether `block` is one a caption may follow: a diagram, a table or a
/// code block.
pub(super) fn captionable(block: &[Event<'_>]) -> bool {
    matches!(
        block.first(),
        Some(Event::Start(Tag::CodeBlock(_) | Tag::Table(_)))
    )
}

/// Whether `block` is a paragraph whose text starts with "Figure", a
/// number and a full stop, followed by a space or nothing.
pub(super) fn is_caption(block: &[Event<'_>]) -> bool {
    if !matches!(block.first(), Some(Event::Start(Tag::Paragraph))) {
        return false;
    }
    let mut text = String::new();
    for event in block {
        if let Event::Text(part) | Event::Code(part) = event {
            text.push_str(part);
        }
    }
    let Some(rest) = text.strip_prefix("Figure ") else {
        return false;
    };
    let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
    let after = rest.get(digits..).unwrap_or_default();
    digits > 0
        && after
            .strip_prefix('.')
            .is_some_and(|tail| tail.is_empty() || tail.starts_with(char::is_whitespace))
}

/// The diagram type of a fenced block with `language` and `source`, when
/// it is a Mermaid diagram a page may draw.
pub(super) fn diagram_type(language: &str, source: &str) -> Option<&'static str> {
    if language != "mermaid" {
        return None;
    }
    let first = source.split_whitespace().next()?;
    DIAGRAM_TYPES.iter().copied().find(|kind| *kind == first)
}
