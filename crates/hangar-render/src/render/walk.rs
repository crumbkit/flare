//! The worst-case output size of a template over its data, and the refusal
//! of a variable tag that names a list or an object.

use super::{MAX_OUTPUT_BYTES, RenderError};
use crate::check::{Node, line_and_column};
use mustache2::Data;

/// The bytes counted so far. Each node visited counts at least one byte, so
/// a template whose sections loop over long lists with little in them is
/// bounded by its visits as well as by its output.
#[derive(Debug, Default)]
struct Budget {
    used: usize,
}

impl Budget {
    fn add(&mut self, bytes: usize) -> Result<(), RenderError> {
        self.used = self.used.saturating_add(bytes.max(1));
        if self.used > MAX_OUTPUT_BYTES {
            return Err(RenderError::TooLarge {
                max: MAX_OUTPUT_BYTES,
            });
        }
        Ok(())
    }
}

/// Walks the template's tags against `data` with the context stack that
/// Mustache uses, and returns an upper bound on the bytes mustache2 writes.
///
/// A section over a list visits each element, a section over another truthy
/// value pushes it, and an inverted section pushes nothing. The walk stops
/// as soon as the bound passes 4 MiB.
///
/// # Errors
///
/// Returns [`RenderError::TooLarge`] when the bound passes 4 MiB and
/// [`RenderError::NotText`] when a variable tag resolves to a list or an
/// object.
pub(crate) fn worst_case(text: &str, nodes: &[Node], data: &Data) -> Result<usize, RenderError> {
    let mut budget = Budget::default();
    let mut stack = vec![data];
    walk(text, nodes, &mut stack, &mut budget)?;
    Ok(budget.used)
}

fn walk(
    text: &str,
    nodes: &[Node],
    stack: &mut Vec<&Data>,
    budget: &mut Budget,
) -> Result<(), RenderError> {
    for node in nodes {
        match node {
            Node::Text(bytes) => budget.add(*bytes)?,
            Node::Variable { name, at } => {
                budget.add(written(text, name, *at, resolve(stack, name))?)?
            }
            Node::Section {
                name,
                inverted,
                children,
                ..
            } => {
                budget.add(0)?;
                let value = resolve(stack, name);
                if *inverted {
                    if !value.is_some_and(Data::is_truthy) {
                        walk(text, children, stack, budget)?;
                    }
                    continue;
                }
                match value {
                    Some(Data::List(items)) => {
                        for item in items {
                            budget.add(0)?;
                            stack.push(item);
                            walk(text, children, stack, budget)?;
                            stack.pop();
                        }
                    }
                    Some(value) if value.is_truthy() => {
                        stack.push(value);
                        walk(text, children, stack, budget)?;
                        stack.pop();
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

/// The bytes mustache2 writes for a variable tag that resolves to `value`.
fn written(text: &str, name: &str, at: usize, value: Option<&Data>) -> Result<usize, RenderError> {
    Ok(match value {
        None | Some(Data::Null) => 0,
        Some(Data::String(string)) => string.chars().map(escaped_length).sum(),
        Some(Data::Bool(flag)) => flag.to_string().len(),
        Some(Data::Int(number)) => number.to_string().len(),
        Some(Data::Float(number)) => number.to_string().len(),
        Some(Data::List(_) | Data::Map(_)) => {
            let (line, column) = line_and_column(text, at);
            return Err(RenderError::NotText {
                name: name.to_owned(),
                line,
                column,
            });
        }
    })
}

/// The bytes mustache2 writes for one character of a string variable.
fn escaped_length(character: char) -> usize {
    match character {
        '&' => 5,
        '"' => 6,
        '<' | '>' => 4,
        other => other.len_utf8(),
    }
}

/// Resolves `name` against the context stack as mustache2 does: `.` is the
/// top of the stack, and otherwise the first segment is looked up in the
/// nearest map that holds it and each later segment inside the value found.
fn resolve<'d>(stack: &[&'d Data], name: &str) -> Option<&'d Data> {
    if name == "." {
        return stack.last().copied();
    }
    let mut segments = name.split('.');
    let first = segments.next()?;
    let mut value = stack.iter().rev().find_map(|data| match data {
        Data::Map(map) => map.get(first),
        _ => None,
    })?;
    for segment in segments {
        value = match value {
            Data::Map(map) => map.get(segment)?,
            _ => return None,
        };
    }
    Some(value)
}
