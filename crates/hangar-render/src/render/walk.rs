//! The worst-case output size of a template over its data, the record data
//! mustache2 copies while it renders, and the refusal of a variable tag that
//! names a list or an object.

use super::{MAX_COPIED_BYTES, MAX_OUTPUT_BYTES, RenderError};
use crate::check::{Node, line_and_column};
use mustache2::Data;

/// What one value counts toward the copy budget, on top of the length of
/// its text and of its key in a map. A copy allocates for each string and
/// moves each value, so a list of many short values costs more than its
/// text.
const VALUE_COPY_BYTES: usize = 32;

/// The bytes counted so far, for output and for copies.
///
/// Each node visited counts at least one output byte, so a template whose
/// sections loop over long lists with little in them is bounded by its
/// visits as well as by its output. mustache2 copies the whole list each
/// time it enters a section over a list, the element each time it pushes
/// one, and the value each time a section pushes a map or another truthy
/// value, so each of those counts the copied value's size.
#[derive(Debug, Default)]
struct Budget {
    used: usize,
    copied: usize,
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

    /// Counts one lookup of `name` on a context stack `depth` values deep.
    /// mustache2 hashes the name once for each value on the stack, so a
    /// lookup counts at least the name's length times the depth.
    fn look_up(&mut self, name: &str, depth: usize) -> Result<(), RenderError> {
        self.charge(name.len().max(1).saturating_mul(depth.max(1)))
    }

    fn charge(&mut self, bytes: usize) -> Result<(), RenderError> {
        self.copied = self.copied.saturating_add(bytes);
        if self.copied > MAX_COPIED_BYTES {
            return Err(RenderError::TooCostly {
                max: MAX_COPIED_BYTES,
            });
        }
        Ok(())
    }

    /// Counts one copy of `value`. The count stops as soon as it passes the
    /// budget, so measuring a large value costs no more than the budget.
    fn copy(&mut self, value: &Data) -> Result<(), RenderError> {
        let left = MAX_COPIED_BYTES.saturating_sub(self.copied);
        self.charge(copy_size(value, left))
    }
}

/// The bytes a copy of `value` counts, or a number over `limit` once the
/// count passes it.
fn copy_size(value: &Data, limit: usize) -> usize {
    let mut size = 0_usize;
    let mut pending = vec![value];
    while let Some(value) = pending.pop() {
        size = size.saturating_add(VALUE_COPY_BYTES);
        match value {
            Data::String(text) => size = size.saturating_add(text.len()),
            Data::List(items) => pending.extend(items),
            Data::Map(map) => {
                for (key, item) in map {
                    size = size.saturating_add(key.len());
                    pending.push(item);
                }
            }
            _ => {}
        }
        if size > limit {
            break;
        }
    }
    size
}

/// Walks the template's tags against `data` with the context stack that
/// Mustache uses, and returns an upper bound on the bytes mustache2 writes.
///
/// A section over a list visits each element, a section over another truthy
/// value pushes it, and an inverted section pushes nothing. The walk stops
/// as soon as the bound passes 4 MiB, or the bytes mustache2 would copy and
/// look up pass 64 MiB.
///
/// # Errors
///
/// Returns [`RenderError::TooLarge`] when the bound passes 4 MiB,
/// [`RenderError::TooCostly`] when the copies and lookups pass 64 MiB, and
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
                budget.look_up(name, stack.len())?;
                budget.add(written(text, name, *at, resolve(stack, name))?)?
            }
            Node::Section {
                name,
                inverted,
                children,
                ..
            } => {
                budget.add(0)?;
                budget.look_up(name, stack.len())?;
                let value = resolve(stack, name);
                if *inverted {
                    if !value.is_some_and(Data::is_truthy) {
                        walk(text, children, stack, budget)?;
                    }
                    continue;
                }
                match value {
                    Some(list @ Data::List(items)) => {
                        budget.copy(list)?;
                        for item in items {
                            budget.add(0)?;
                            budget.copy(item)?;
                            stack.push(item);
                            walk(text, children, stack, budget)?;
                            stack.pop();
                        }
                    }
                    Some(value) if value.is_truthy() => {
                        budget.copy(value)?;
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

/// The bytes the cost budget counts for `template` with `a` filling the
/// slot `a`.
#[cfg(test)]
pub(super) fn copied(template: &str, a: &str) -> usize {
    use crate::test_support::{html_page, record, slot, slot_name};
    let page = html_page(template, vec![slot("a", &[], None)]);
    let fills = [(slot_name("a"), record("/tasks/a.md", '1', a))];
    let Ok(data) = super::data::page_data(&page, &fills) else {
        return 0;
    };
    let Ok(Some(nodes)) = crate::check::template(&page) else {
        return 0;
    };
    let mut budget = Budget::default();
    let mut stack = vec![&data];
    let _ = walk(template, &nodes, &mut stack, &mut budget);
    budget.copied
}
