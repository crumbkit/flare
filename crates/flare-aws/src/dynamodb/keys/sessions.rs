//! The keys of sessions, their links and the sign-in state's sealing
//! keys.

use super::{SEALING_KEY_PREFIX, SESSION_LINK_PREFIX, TIME_DIGITS};
use flare_core::{Digest, KeyId, PrincipalId, Timestamp};

/// The partition of the sealing keys of the sign-in state.
pub(crate) const SEALING_PARTITION: &str = "KEY#SIGNIN";

/// The partition of a person's session links.
pub(crate) fn session_partition(person: &PrincipalId) -> String {
    format!("SESSOF#{}", person.as_str())
}

/// The id of a sealing key's sort key, or `None` for any other sort key.
pub(crate) fn sealing_key_of(sort_key: &str) -> Option<KeyId> {
    let digits = sort_key.strip_prefix(SEALING_KEY_PREFIX)?;
    if digits.len() != 10 || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    KeyId::new(digits.parse().ok()?)
}

/// The lowest and highest sort keys of the links of sessions that expire
/// after `now`.
pub(crate) fn live_links(now: Timestamp) -> (String, String) {
    (
        format!(
            "{SESSION_LINK_PREFIX}{:020}#",
            now.as_secs().saturating_add(1)
        ),
        format!("{SESSION_LINK_PREFIX}99999999999999999999#~"),
    )
}

/// The expiry and the session digest of a link's sort key, or `None` for
/// any other sort key.
pub(crate) fn link_of(sort_key: &str) -> Option<(Timestamp, Digest)> {
    let rest = sort_key.strip_prefix(SESSION_LINK_PREFIX)?;
    let (digits, hex) = rest.split_once('#')?;
    if digits.len() != TIME_DIGITS || !digits.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    Some((
        Timestamp::from_secs(digits.parse().ok()?),
        Digest::from_hex(hex)?,
    ))
}
