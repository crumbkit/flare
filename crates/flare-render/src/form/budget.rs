//! The bytes a form's post takes, so that the largest form flare draws
//! posts within the server's request body limit.

use crate::token::FormToken;
use flare_core::{BindingName, Blob, FieldName, SlotName};
use serde_json::Value;

/// The server's limit on a request body in bytes. A form's post is
/// `application/x-www-form-urlencoded`, so this counts encoded bytes.
pub(crate) const MAX_POST_BYTES: usize = 1024 * 1024;

/// The bytes of `text` in an `application/x-www-form-urlencoded` body, after
/// a browser turns each line break into CRLF: an ASCII letter or digit, `*`,
/// `-`, `.`, `_` and a space take one byte, a line break takes six, and every
/// other byte takes three.
pub(crate) fn encoded(text: &str) -> usize {
    let mut bytes = text.bytes().peekable();
    let mut size = 0_usize;
    while let Some(byte) = bytes.next() {
        size = size.saturating_add(match byte {
            b'\r' => {
                bytes.next_if_eq(&b'\n');
                6
            }
            b'\n' => 6,
            _ if byte.is_ascii_alphanumeric() || b"*-._ ".contains(&byte) => 1,
            _ => 3,
        });
    }
    size
}

/// What is left of a post's bytes once the hidden inputs are counted.
#[derive(Debug)]
pub(crate) struct PostBudget {
    left: usize,
}

impl PostBudget {
    /// The budget of a form that carries these hidden inputs and an empty
    /// `base`.
    pub(crate) fn new(
        token: &FormToken,
        based_on: &Blob,
        through: &BindingName,
        slot: &SlotName,
    ) -> Self {
        let pair = |name: &str, value: usize| encoded(name).saturating_add(value).saturating_add(2);
        let fixed = [
            pair("token", token.encoded_len()),
            pair("based_on", encoded(based_on.as_str())),
            pair("through", encoded(through.as_str())),
            pair("slot", encoded(slot.as_str())),
            pair("base", encoded("{}")),
        ]
        .iter()
        .fold(0_usize, |sum, size| sum.saturating_add(*size));
        Self {
            left: MAX_POST_BYTES.saturating_sub(fixed),
        }
    }

    /// Takes the bytes of one field: its entry `text`, posted as `f.<name>`,
    /// and its member in `base` with the value `loaded`. Returns false, and
    /// takes nothing, when the field does not fit in what is left.
    pub(crate) fn take(&mut self, name: &FieldName, text: &str, loaded: &Value) -> bool {
        let key = Value::String(name.as_str().to_owned()).to_string();
        let member = encoded(&key)
            .saturating_add(encoded(&loaded.to_string()))
            .saturating_add(6);
        let entry = encoded("f.")
            .saturating_add(encoded(name.as_str()))
            .saturating_add(encoded(text))
            .saturating_add(2);
        let size = entry.saturating_add(member);
        if size > self.left {
            return false;
        }
        self.left -= size;
        true
    }
}

#[cfg(test)]
mod tests;
