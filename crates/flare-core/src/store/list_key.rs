//! [`ListKey`], the place of one entry in a person's list of shares, and
//! [`ListPage`], one page of such a list.

use crate::grant::Grant;
use crate::share::{Share, ShareId};
use crate::time::Timestamp;
use std::cmp::{Ordering, Reverse};

/// The place of one entry in a list of shares: the entry's time and its
/// share. Lists run newest first and then by share id, and a key compares
/// in that order, so the entries that follow a key are the ones greater
/// than it. A key need not name an entry that still exists: a list read
/// after a key whose grant was revoked starts at the entry after it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ListKey {
    created: Timestamp,
    share: ShareId,
}

impl ListKey {
    /// The key of the entry of `share` at `created`.
    #[must_use]
    pub fn new(created: Timestamp, share: ShareId) -> Self {
        Self { created, share }
    }

    /// The key of a grant in its grantee's list: its time of first grant
    /// and its share.
    #[must_use]
    pub fn of_grant(grant: &Grant) -> Self {
        Self::new(grant.created(), grant.share().clone())
    }

    /// The key of a share in its owner's list: its creation time and its
    /// id.
    #[must_use]
    pub fn of_share(share: &Share) -> Self {
        Self::new(share.created(), share.id().clone())
    }

    /// The entry's time.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// The entry's share.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// Whether the entry at `other` comes after this key in a list.
    #[must_use]
    pub fn precedes(&self, other: &Self) -> bool {
        self < other
    }
}

impl Ord for ListKey {
    fn cmp(&self, other: &Self) -> Ordering {
        (Reverse(self.created), &self.share).cmp(&(Reverse(other.created), &other.share))
    }
}

impl PartialOrd for ListKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// One page of a list of shares: its entries, and the key of the last
/// entry read when more follow.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ListPage<T> {
    /// The entries, in list order.
    pub items: Vec<T>,
    /// The key to read the next page after, present only when more entries
    /// follow. It can name an entry that the page left out, so a page may
    /// hold fewer entries than its limit and still have a next key.
    pub next: Option<ListKey>,
}

impl<T> ListPage<T> {
    /// A page of `items` with `next`.
    pub(crate) fn new(items: Vec<T>, next: Option<ListKey>) -> Self {
        Self { items, next }
    }
}
