//! [`Cursor`]: the text that names a place in the activity sequence, and
//! [`ShareCursor`]: the text that names a place in a list of shares.

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

/// Why a cursor's text was refused: another prefix, another length, a
/// character outside base64url, or a share cursor's scope byte or share id
/// that names nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CursorError;

impl fmt::Display for CursorError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("the cursor is not one that flare wrote")
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

/// The version prefix of every share cursor this contract writes. A
/// change to the order of a share list takes a new prefix.
const SHARE_PREFIX: &str = "fs1.";

/// The bytes of a share id.
const SHARE_BYTES: usize = 16;

/// The bytes of a share cursor: the scope, the time and the share id.
const SHARE_CURSOR_BYTES: usize = 1 + 8 + SHARE_BYTES;

/// Which list of shares a [`ShareCursor`] names a place in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ListScope {
    /// The caller's own shares, `scope=mine`.
    Mine,
    /// The shares granted to the caller, `scope=with-me`.
    WithMe,
}

impl ListScope {
    /// The scope's byte in a cursor.
    const fn byte(self) -> u8 {
        match self {
            Self::Mine => 0,
            Self::WithMe => 1,
        }
    }

    /// The scope that `byte` names.
    const fn from_byte(byte: u8) -> Option<Self> {
        match byte {
            0 => Some(Self::Mine),
            1 => Some(Self::WithMe),
            _ => None,
        }
    }
}

/// The last entry of one page of a share list: `fs1.` followed by
/// base64url, without padding, of 25 bytes. The first byte is the scope,
/// the next eight are the entry's time in seconds, big-endian, and the last
/// sixteen are the share id. A list continues with the entries that sort
/// after it, newest first and then by share id. It holds no person, so the
/// server reads the list of the caller its token proves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ShareCursor {
    scope: ListScope,
    seconds: u64,
    share: [u8; SHARE_BYTES],
}

impl ShareCursor {
    /// The cursor after the entry of `share` at `seconds` in `scope`.
    #[must_use]
    pub const fn new(scope: ListScope, seconds: u64, share: [u8; SHARE_BYTES]) -> Self {
        Self {
            scope,
            seconds,
            share,
        }
    }

    /// The cursor after the entry of the share whose id is `hex`, 32
    /// lower-case hexadecimal characters.
    ///
    /// # Errors
    ///
    /// Returns [`CursorError`] when `hex` is not 32 lower-case hexadecimal
    /// characters.
    pub fn from_hex(scope: ListScope, seconds: u64, hex: &str) -> Result<Self, CursorError> {
        let (pairs, rest) = hex.as_bytes().as_chunks::<2>();
        if pairs.len() != SHARE_BYTES || !rest.is_empty() {
            return Err(CursorError);
        }
        let mut share = [0; SHARE_BYTES];
        for (byte, [high, low]) in share.iter_mut().zip(pairs) {
            *byte = (nibble(*high)? << 4) | nibble(*low)?;
        }
        Ok(Self::new(scope, seconds, share))
    }

    /// The list the cursor names a place in.
    #[must_use]
    pub const fn scope(self) -> ListScope {
        self.scope
    }

    /// The entry's time, in seconds since the Unix epoch.
    #[must_use]
    pub const fn seconds(self) -> u64 {
        self.seconds
    }

    /// The entry's share id as 32 lower-case hexadecimal characters.
    #[must_use]
    pub fn share_hex(self) -> String {
        const DIGITS: &[u8; 16] = b"0123456789abcdef";
        let mut text = String::with_capacity(SHARE_BYTES * 2);
        for byte in self.share {
            for half in [byte >> 4, byte & 0x0f] {
                let digit = DIGITS.get(usize::from(half)).copied().unwrap_or(b'0');
                text.push(char::from(digit));
            }
        }
        text
    }
}

/// The value of one lower-case hexadecimal digit.
fn nibble(digit: u8) -> Result<u8, CursorError> {
    match digit {
        b'0'..=b'9' => Ok(digit - b'0'),
        b'a'..=b'f' => Ok(digit - b'a' + 10),
        _ => Err(CursorError),
    }
}

impl fmt::Display for ShareCursor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut bytes = [0; SHARE_CURSOR_BYTES];
        let (scope, rest) = bytes.split_at_mut(1);
        let (seconds, share) = rest.split_at_mut(8);
        scope.copy_from_slice(&[self.scope.byte()]);
        seconds.copy_from_slice(&self.seconds.to_be_bytes());
        share.copy_from_slice(&self.share);
        formatter.write_str(SHARE_PREFIX)?;
        formatter.write_str(&URL_SAFE_NO_PAD.encode(bytes))
    }
}

impl TryFrom<&str> for ShareCursor {
    type Error = CursorError;

    fn try_from(text: &str) -> Result<Self, CursorError> {
        let body = text.strip_prefix(SHARE_PREFIX).ok_or(CursorError)?;
        let bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| CursorError)?;
        let bytes: [u8; SHARE_CURSOR_BYTES] = bytes.try_into().map_err(|_| CursorError)?;
        let (scope, rest) = bytes.split_at(1);
        let (seconds, share) = rest.split_at(8);
        let scope = scope
            .first()
            .copied()
            .and_then(ListScope::from_byte)
            .ok_or(CursorError)?;
        let seconds: [u8; 8] = seconds.try_into().map_err(|_| CursorError)?;
        let share: [u8; SHARE_BYTES] = share.try_into().map_err(|_| CursorError)?;
        Ok(Self::new(scope, u64::from_be_bytes(seconds), share))
    }
}

#[cfg(test)]
mod tests;
