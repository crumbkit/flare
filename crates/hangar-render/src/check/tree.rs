//! Nests a template's sections.

use super::mustache::{Tag, TagKind};
use super::{FlawKind, Found};

/// The deepest nesting of sections a template may have.
const MAX_DEPTH: usize = 8;

/// One node of a template's section tree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Node {
    /// Literal text of this many bytes.
    Text(usize),
    /// A variable tag at a byte offset.
    Variable { name: String, at: usize },
    /// A section or inverted section and what it holds.
    Section {
        name: String,
        at: usize,
        inverted: bool,
        children: Vec<Node>,
    },
}

/// A section whose close tag is still to come.
struct Open {
    name: String,
    at: usize,
    inverted: bool,
    children: Vec<Node>,
}

/// Nests the tags of a template of `length` bytes into a tree, and finds
/// unbalanced and too deeply nested sections.
pub(crate) fn build(length: usize, tags: &[Tag]) -> (Vec<Node>, Vec<Found>) {
    let mut root = Vec::new();
    let mut stack: Vec<Open> = Vec::new();
    let mut found = Vec::new();
    let mut cursor = 0;
    for tag in tags {
        if tag.start > cursor {
            current(&mut root, &mut stack).push(Node::Text(tag.start - cursor));
        }
        cursor = tag.end;
        match tag.kind {
            TagKind::Variable => current(&mut root, &mut stack).push(Node::Variable {
                name: tag.name.clone(),
                at: tag.start,
            }),
            TagKind::Section | TagKind::Inverted => {
                if stack.len() == MAX_DEPTH {
                    found.push(Found {
                        kind: FlawKind::TooDeep,
                        at: tag.start,
                    });
                }
                stack.push(Open {
                    name: tag.name.clone(),
                    at: tag.start,
                    inverted: tag.kind == TagKind::Inverted,
                    children: Vec::new(),
                });
            }
            TagKind::Close => match stack.pop() {
                Some(open) if open.name == tag.name => {
                    let node = close(open);
                    current(&mut root, &mut stack).push(node);
                }
                other => {
                    stack.extend(other);
                    found.push(Found {
                        kind: FlawKind::UnbalancedSection {
                            name: tag.name.clone(),
                        },
                        at: tag.start,
                    });
                }
            },
            TagKind::Comment | TagKind::Refused => {}
        }
    }
    if length > cursor {
        current(&mut root, &mut stack).push(Node::Text(length - cursor));
    }
    while let Some(open) = stack.pop() {
        found.push(Found {
            kind: FlawKind::UnbalancedSection {
                name: open.name.clone(),
            },
            at: open.at,
        });
        let node = close(open);
        current(&mut root, &mut stack).push(node);
    }
    (root, found)
}

fn current<'a>(root: &'a mut Vec<Node>, stack: &'a mut [Open]) -> &'a mut Vec<Node> {
    match stack.last_mut() {
        Some(open) => &mut open.children,
        None => root,
    }
}

fn close(open: Open) -> Node {
    Node::Section {
        name: open.name,
        at: open.at,
        inverted: open.inverted,
        children: open.children,
    }
}
