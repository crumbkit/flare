//! The probe's nonce. `flare open` sends a fresh nonce to the port a
//! server file names, and a server that holds the file's token answers
//! with the proof, so `flare open` learns the server is the one it wants
//! before it sends the token.

use crate::secret::random_hex;

/// The fewest hex digits a nonce may have.
const MIN_DIGITS: usize = 16;
/// The most hex digits a nonce may have.
const MAX_DIGITS: usize = 128;

/// A probe nonce: 16 to 128 lower-case hex digits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nonce(String);

impl Nonce {
    /// A new nonce of 32 random hex digits.
    ///
    /// # Errors
    ///
    /// Returns [`NonceError::NoRandomness`] when the operating system gives
    /// no random bytes.
    pub fn new() -> Result<Self, NonceError> {
        random_hex(16)
            .map(Self)
            .map_err(|_| NonceError::NoRandomness)
    }

    /// The nonce as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for Nonce {
    type Error = NonceError;

    fn try_from(text: &str) -> Result<Self, NonceError> {
        let digits = (MIN_DIGITS..=MAX_DIGITS).contains(&text.len())
            && text
                .bytes()
                .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'));
        if digits {
            Ok(Self(text.to_owned()))
        } else {
            Err(NonceError::Malformed)
        }
    }
}

/// Why no nonce was made.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum NonceError {
    /// The operating system gave no random bytes.
    #[error("the operating system gave no random bytes")]
    NoRandomness,
    /// The text is not 16 to 128 lower-case hex digits.
    #[error("a probe nonce must be 16 to 128 lower-case hex digits")]
    Malformed,
}
