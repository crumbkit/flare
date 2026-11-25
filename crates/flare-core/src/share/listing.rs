//! [`Listing`], a bounded list that a store returns.

/// A bounded list: at most [`MAX_LISTED`](super::MAX_LISTED) entries, and
/// whether more exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listing<T> {
    items: Vec<T>,
    truncated: bool,
}

impl<T> Listing<T> {
    /// Builds a listing from the entries in their order. Entries past
    /// [`MAX_LISTED`](super::MAX_LISTED) are dropped, and then the listing
    /// says it is truncated, so a store that reads one entry more than the
    /// limit learns whether more exist.
    #[must_use]
    pub fn new(items: Vec<T>) -> Self {
        Self::bounded(items, super::MAX_LISTED)
    }

    /// Builds a listing as [`Listing::new`] does, bounded by `limit` as
    /// well as by [`MAX_LISTED`](super::MAX_LISTED).
    #[must_use]
    pub fn bounded(mut items: Vec<T>, limit: usize) -> Self {
        let bound = limit.min(super::MAX_LISTED);
        let truncated = items.len() > bound;
        items.truncate(bound);
        Self { items, truncated }
    }

    /// A listing of entries drawn from another listing, which keeps that
    /// listing's truncation flag.
    pub(crate) fn with_truncated(mut items: Vec<T>, truncated: bool) -> Self {
        let over = items.len() > super::MAX_LISTED;
        items.truncate(super::MAX_LISTED);
        Self {
            items,
            truncated: truncated || over,
        }
    }

    /// The entries.
    #[must_use]
    pub fn items(&self) -> &[T] {
        &self.items
    }

    /// Whether more entries exist than the listing holds.
    #[must_use]
    pub fn truncated(&self) -> bool {
        self.truncated
    }

    /// The entries, taken out of the listing.
    #[must_use]
    pub fn into_items(self) -> Vec<T> {
        self.items
    }
}
