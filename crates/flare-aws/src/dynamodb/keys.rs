//! The keys of the table's items, built only from parsed values.
//!
//! Every key starts with a fixed prefix that names its kind and puts its
//! one variable part of unknown length last, so no two kinds of item share
//! a key. Share ids are 32 lower-case hexadecimal characters, version
//! numbers are 10 digits and times are 20 digits.

use flare_core::{PrincipalId, ShareId, ShareVersion, Timestamp};

/// The sort key prefix of an owner or grantee entry. A list is a query of
/// the sort keys that start with it.
pub(crate) const ENTRY_PREFIX: &str = "E#";

/// The sort key prefix of a grant item.
pub(crate) const GRANT_PREFIX: &str = "G#";

/// The digits of an inverted time in an entry's sort key.
const TIME_DIGITS: usize = 20;

/// The characters of a share id.
const ID_CHARS: usize = 32;

/// The partition key and the sort key of one item.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub(crate) pk: String,
    pub(crate) sk: String,
}

impl Key {
    fn new(pk: String, sk: impl Into<String>) -> Self {
        Self { pk, sk: sk.into() }
    }

    /// A share's item, which holds its owner, title, creation time and
    /// latest version.
    pub(crate) fn share(id: &ShareId) -> Self {
        Self::new(share_partition(id), "META")
    }

    /// One version of a share.
    pub(crate) fn version(id: &ShareId, number: ShareVersion) -> Self {
        Self::new(share_partition(id), format!("V#{:010}", number.get()))
    }

    /// One person's grant on a share.
    pub(crate) fn grant(id: &ShareId, grantee: &PrincipalId) -> Self {
        Self::new(
            share_partition(id),
            format!("{GRANT_PREFIX}{}", grantee.as_str()),
        )
    }

    /// The number of grants of a share.
    pub(crate) fn grant_count(id: &ShareId) -> Self {
        Self::new(share_partition(id), "N#grants")
    }

    /// The number of shares of an owner.
    pub(crate) fn share_count(owner: &PrincipalId) -> Self {
        Self::new(owner_partition(owner), "N#shares")
    }

    /// A share's entry in its owner's list.
    pub(crate) fn owner_entry(owner: &PrincipalId, created: Timestamp, id: &ShareId) -> Self {
        Self::new(owner_partition(owner), entry_sort_key(created, id))
    }

    /// A grant's entry in its grantee's list.
    pub(crate) fn grantee_entry(grantee: &PrincipalId, created: Timestamp, id: &ShareId) -> Self {
        Self::new(grantee_partition(grantee), entry_sort_key(created, id))
    }
}

/// The partition of a share's items.
pub(crate) fn share_partition(id: &ShareId) -> String {
    format!("SHARE#{}", id.as_str())
}

/// The partition of an owner's entries and share count.
pub(crate) fn owner_partition(owner: &PrincipalId) -> String {
    format!("OWN#{}", owner.as_str())
}

/// The partition of a grantee's entries.
pub(crate) fn grantee_partition(grantee: &PrincipalId) -> String {
    format!("TO#{}", grantee.as_str())
}

/// `E#<inverted time>#<id>`. The time is `u64::MAX - created` in 20 digits,
/// so an ascending sort lists the newest first and breaks ties by id.
fn entry_sort_key(created: Timestamp, id: &ShareId) -> String {
    let inverted = u64::MAX - created.as_secs();
    format!("{ENTRY_PREFIX}{inverted:020}#{}", id.as_str())
}

/// The time and the share id of an entry's sort key, or `None` for any
/// other sort key.
pub(crate) fn entry_of(sort_key: &str) -> Option<(Timestamp, ShareId)> {
    let rest = sort_key.strip_prefix(ENTRY_PREFIX)?;
    let (digits, id) = rest.split_once('#')?;
    if digits.len() != TIME_DIGITS
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || id.len() != ID_CHARS
    {
        return None;
    }
    let inverted: u64 = digits.parse().ok()?;
    let id = ShareId::try_from(id).ok()?;
    Some((Timestamp::from_secs(u64::MAX - inverted), id))
}

/// The grantee of a grant item's sort key, or `None` for any other sort
/// key.
pub(crate) fn grantee_of(sort_key: &str) -> Option<PrincipalId> {
    PrincipalId::try_from(sort_key.strip_prefix(GRANT_PREFIX)?).ok()
}

#[cfg(test)]
mod tests;
