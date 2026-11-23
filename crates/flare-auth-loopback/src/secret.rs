//! Random bytes, digests and constant-time comparison, and every place the
//! crate reads a secret's value. This is the only file of the crate that
//! calls `expose_secret` (BASE-24).

use aws_lc_rs::{constant_time, digest, rand};
use secrecy::{ExposeSecret as _, SecretString};

/// The operating system gave no random bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoRandomness;

/// `bytes` random bytes, at most 32, as lower-case hex.
pub(crate) fn random_hex(bytes: usize) -> Result<String, NoRandomness> {
    let mut buffer = [0_u8; 32];
    let wanted = buffer.get_mut(..bytes).ok_or(NoRandomness)?;
    rand::fill(wanted).map_err(|_| NoRandomness)?;
    Ok(hex(wanted))
}

/// A new secret of 32 random bytes, as 64 hex digits.
pub(crate) fn random_secret() -> Result<SecretString, NoRandomness> {
    random_hex(32).map(SecretString::from)
}

/// The SHA-256 of `bytes`.
pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    let mut out = [0_u8; 32];
    out.copy_from_slice(digest::digest(&digest::SHA256, bytes).as_ref());
    out
}

/// The SHA-256 of a secret's text, which the tables keep in place of the
/// secret.
pub(crate) fn digest_of(secret: &SecretString) -> [u8; 32] {
    sha256(secret.expose_secret().as_bytes())
}

/// Whether two byte strings are equal, compared in constant time for
/// strings of one length.
pub(crate) fn same(left: &[u8], right: &[u8]) -> bool {
    constant_time::verify_slices_are_equal(left, right).is_ok()
}

/// Whether two secrets are equal, compared in constant time.
pub(crate) fn same_secret(left: &SecretString, right: &SecretString) -> bool {
    same(
        left.expose_secret().as_bytes(),
        right.expose_secret().as_bytes(),
    )
}

/// The probe's proof: the hex SHA-256 of the token's text followed by the
/// nonce's text.
pub(crate) fn proof(token: &SecretString, nonce: &str) -> String {
    let mut input = String::with_capacity(token.expose_secret().len() + nonce.len());
    input.push_str(token.expose_secret());
    input.push_str(nonce);
    let proof = hex(&sha256(input.as_bytes()));
    drop(SecretString::from(input));
    proof
}

/// A new secret made from `secret`'s text by `make`, such as a URL that
/// carries a code.
pub(crate) fn compose(secret: &SecretString, make: impl FnOnce(&str) -> String) -> SecretString {
    SecretString::from(make(secret.expose_secret()))
}

/// Runs `use_bytes` on a secret's bytes, to write them to a file.
pub(crate) fn with_bytes<R>(secret: &SecretString, use_bytes: impl FnOnce(&[u8]) -> R) -> R {
    use_bytes(secret.expose_secret().as_bytes())
}

/// `bytes` as lower-case hex.
pub(crate) fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0f] {
            text.push(char::from(
                DIGITS.get(usize::from(nibble)).copied().unwrap_or(b'0'),
            ));
        }
    }
    text
}

/// Whether `text` is `len` lower-case hex digits.
pub(crate) fn is_hex(text: &str, len: usize) -> bool {
    text.len() == len
        && text
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
}
