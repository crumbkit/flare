//! The keys of the table's items, built only from parsed values.
//!
//! Every key starts with a fixed prefix that names its kind and puts its
//! one variable part of unknown length last, so no two kinds of item share
//! a key. Share ids are 32 lower-case hexadecimal characters, version
//! numbers are 10 digits and times are 20 digits. A login code's digest is
//! 64 hexadecimal characters, a token id is 32 and a response position is
//! 20 digits. An activity index entry's sort key names its flare before
//! its position, both of fixed length, so one reader's entries on one flare
//! sort in position order.

use flare_core::{
    Digest, Email, Position, PrincipalId, Reader, ShareId, ShareVersion, Timestamp, TokenId,
    TokenLabel,
};

/// The sort key prefix of an owner or grantee entry. A list is a query of
/// the sort keys that start with it.
pub(crate) const ENTRY_PREFIX: &str = "E#";

/// The sort key prefix of a grant item.
pub(crate) const GRANT_PREFIX: &str = "G#";

/// The sort key prefix of a response's link to a flare, under the
/// response's partition.
pub(crate) const PAIR_PREFIX: &str = "F#";

/// The sort key prefix of a link under a flare's partition.
pub(crate) const LINK_PREFIX: &str = "P#";

/// The sort key prefix of an activity index entry, under its reader's
/// partition.
pub(crate) const INDEX_ENTRY_PREFIX: &str = "A#";

/// The sort key prefix of an activity index marker, under its reader's
/// partition.
pub(crate) const INDEX_MARKER_PREFIX: &str = "R#";

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

impl Key {
    /// A login code, by the digest of its text.
    pub(crate) fn code(digest: &Digest) -> Self {
        Self::new(format!("CODE#{}", digest.to_hex()), "CODE")
    }

    /// A token, by its id.
    pub(crate) fn token(id: &TokenId) -> Self {
        Self::new(format!("TOKEN#{}", id.as_str()), "TOKEN")
    }

    /// The slot that names an owner's latest token with one label. The
    /// label is the sort key's variable last part.
    pub(crate) fn label_slot(owner: &Email, label: &TokenLabel) -> Self {
        Self::new(
            format!("LABELS#{}", owner.as_str()),
            format!("L#{}", label.as_str()),
        )
    }

    /// The counter of response positions, which holds the head.
    pub(crate) fn sequence() -> Self {
        Self::new("SEQ#responses".to_owned(), "SEQ")
    }

    /// A response's link to `flare`, under the response's partition.
    pub(crate) fn pair(response: &ShareId, flare: &ShareId) -> Self {
        Self::new(
            response_partition(response),
            format!("{PAIR_PREFIX}{}", flare.as_str()),
        )
    }

    /// The link at `position`, under the flare's partition.
    pub(crate) fn link(flare: &ShareId, position: Position) -> Self {
        Self::new(flare_partition(flare), link_sort_key(position))
    }

    /// The activity index entry of `reader` on `flare` at `position`.
    pub(crate) fn index_entry(reader: &Reader, flare: &ShareId, position: Position) -> Self {
        Self::new(
            index_partition(reader),
            index_entry_sort_key(flare, position),
        )
    }

    /// The marker of `reader`'s entry for `response` on `flare`, which
    /// holds the entry's position.
    pub(crate) fn index_marker(reader: &Reader, response: &ShareId, flare: &ShareId) -> Self {
        Self::new(
            index_partition(reader),
            format!("{}{}", index_markers_of(response), flare.as_str()),
        )
    }
}

/// The partition of a reader's activity index.
pub(crate) fn index_partition(reader: &Reader) -> String {
    match reader {
        Reader::Person(person) => format!("ACT#{}", person.as_str()),
    }
}

/// `A#<flare>#<position>`, with the position in 20 digits.
pub(crate) fn index_entry_sort_key(flare: &ShareId, position: Position) -> String {
    format!(
        "{INDEX_ENTRY_PREFIX}{}#{:020}",
        flare.as_str(),
        position.get()
    )
}

/// `R#<response>#`, the prefix of every marker of one response.
pub(crate) fn index_markers_of(response: &ShareId) -> String {
    format!("{INDEX_MARKER_PREFIX}{}#", response.as_str())
}

/// The partition of the links from a response.
pub(crate) fn response_partition(response: &ShareId) -> String {
    format!("RESP#{}", response.as_str())
}

/// The partition of the links to a flare.
pub(crate) fn flare_partition(flare: &ShareId) -> String {
    format!("RE#{}", flare.as_str())
}

/// `P#<position>` in 20 digits, so sort key order is position order.
pub(crate) fn link_sort_key(position: Position) -> String {
    format!("{LINK_PREFIX}{:020}", position.get())
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
