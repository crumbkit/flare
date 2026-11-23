//! [`Cursor`]: the text that names a place in the activity sequence.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::fmt;

/// The version prefix of every cursor this contract writes.
const PREFIX: &str = "fc1.";

/// A place in the server's one increasing activity sequence: `fc1.`
/// followed by base64url, without padding, of the position as eight
/// big-endian bytes. It holds no flare id, person or secret, so a caller
/// states the flares on each call and the server checks access each time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Cursor(u64);

/// Why a cursor's text was refused: another prefix, another length, or a
/// character outside base64url.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorError;

impl fmt::Display for CursorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a cursor is fc1. followed by 11 base64url characters")
    }
}

impl std::error::Error for CursorError {}

impl Cursor {
    /// The cursor at `position`.
    #[must_use]
    pub const fn new(position: u64) -> Self {
        Self(position)
    }

    /// The position.
    #[must_use]
    pub const fn position(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Cursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(PREFIX)?;
        formatter.write_str(&URL_SAFE_NO_PAD.encode(self.0.to_be_bytes()))
    }
}

impl TryFrom<&str> for Cursor {
    type Error = CursorError;

    fn try_from(text: &str) -> Result<Self, CursorError> {
        let body = text.strip_prefix(PREFIX).ok_or(CursorError)?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| CursorError)?;
        let bytes: [u8; 8] = bytes.try_into().map_err(|_| CursorError)?;
        Ok(Self(u64::from_be_bytes(bytes)))
    }
}

#[cfg(test)]
mod tests;
