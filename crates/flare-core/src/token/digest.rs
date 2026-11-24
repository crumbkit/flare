//! The SHA-256 digest of a secret.

use std::fmt;

use crate::share::hex_id;

/// The SHA-256 digest of a secret. Its `Debug` output hides the bytes.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Digest([u8; 32]);

impl Digest {
    /// The digest with these bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The bytes as 64 lower-case hexadecimal characters, for a store key.
    #[must_use]
    pub fn to_hex(&self) -> String {
        let mut text = String::with_capacity(64);
        for half in self.0.chunks(16) {
            let mut bytes = [0; 16];
            bytes.copy_from_slice(half);
            text.push_str(&hex_id(bytes));
        }
        text
    }

    /// The digest that 64 lower-case hexadecimal characters name, or `None`
    /// for any other text.
    #[must_use]
    pub fn from_hex(text: &str) -> Option<Self> {
        if text.len() != 64 {
            return None;
        }
        let mut bytes = [0_u8; 32];
        for (byte, pair) in bytes.iter_mut().zip(text.as_bytes().chunks(2)) {
            let [high, low] = <[u8; 2]>::try_from(pair).ok()?;
            *byte = (nibble(high)? << 4) | nibble(low)?;
        }
        Some(Self(bytes))
    }
}

/// The value of one lower-case hexadecimal digit.
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        _ => None,
    }
}

impl fmt::Debug for Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Digest(<hidden>)")
    }
}
