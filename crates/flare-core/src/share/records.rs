//! What a share store keeps and returns: shares, their versions, and the
//! inputs that create and publish them.

use super::{HiddenPrincipal, ShareContent, ShareId, ShareKind, ShareTitle, ShareVersion};
use crate::name::PrincipalId;
use crate::time::Timestamp;
use std::fmt;

/// A share as a store holds it. The owner is fixed at creation and never
/// changes, and `latest` is the number of its newest version.
#[derive(Clone, PartialEq, Eq)]
pub struct Share {
    id: ShareId,
    owner: PrincipalId,
    kind: ShareKind,
    title: ShareTitle,
    created: Timestamp,
    latest: ShareVersion,
}

impl Share {
    /// Builds a share as a store read it back.
    #[must_use]
    pub fn new(
        id: ShareId,
        owner: PrincipalId,
        kind: ShareKind,
        title: ShareTitle,
        created: Timestamp,
        latest: ShareVersion,
    ) -> Self {
        Self {
            id,
            owner,
            kind,
            title,
            created,
            latest,
        }
    }

    /// The share's id.
    #[must_use]
    pub fn id(&self) -> &ShareId {
        &self.id
    }

    /// The principal who owns the share.
    #[must_use]
    pub fn owner(&self) -> &PrincipalId {
        &self.owner
    }

    /// What kind of content the share holds.
    #[must_use]
    pub fn kind(&self) -> ShareKind {
        self.kind
    }

    /// The share's title.
    #[must_use]
    pub fn title(&self) -> &ShareTitle {
        &self.title
    }

    /// When the share was created.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// The number of the share's newest version.
    #[must_use]
    pub fn latest(&self) -> ShareVersion {
        self.latest
    }

    /// The same share with `latest` as its newest version, as a store holds
    /// it after a publish.
    #[must_use]
    pub fn with_latest(self, latest: ShareVersion) -> Self {
        Self { latest, ..self }
    }
}

impl fmt::Debug for Share {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Share")
            .field("id", &self.id)
            .field("owner", &HiddenPrincipal)
            .field("kind", &self.kind)
            .field("title", &self.title)
            .field("created", &self.created)
            .field("latest", &self.latest)
            .finish()
    }
}

/// A share to create, with its version 1. Only this crate builds one, so a
/// caller cannot name another person as the owner:
///
/// ```compile_fail,E0624
/// # use flare_core::*;
/// # fn forge(id: ShareId, owner: PrincipalId, title: ShareTitle, content: ShareContent) {
/// let _ = NewShare::new(id, owner, title, content, Timestamp::from_secs(0));
/// # }
/// ```
#[derive(Clone, PartialEq, Eq)]
pub struct NewShare {
    id: ShareId,
    owner: PrincipalId,
    title: ShareTitle,
    content: ShareContent,
    created: Timestamp,
}

impl NewShare {
    pub(crate) fn new(
        id: ShareId,
        owner: PrincipalId,
        title: ShareTitle,
        content: ShareContent,
        created: Timestamp,
    ) -> Self {
        Self {
            id,
            owner,
            title,
            content,
            created,
        }
    }

    /// The new share's id.
    #[must_use]
    pub fn id(&self) -> &ShareId {
        &self.id
    }

    /// The owner, who also publishes version 1.
    #[must_use]
    pub fn owner(&self) -> &PrincipalId {
        &self.owner
    }

    /// The title.
    #[must_use]
    pub fn title(&self) -> &ShareTitle {
        &self.title
    }

    /// The content of version 1.
    #[must_use]
    pub fn content(&self) -> &ShareContent {
        &self.content
    }

    /// When the share and its version 1 were created.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// The share that a store holds after it writes this one.
    #[must_use]
    pub fn share(&self) -> Share {
        Share::new(
            self.id.clone(),
            self.owner.clone(),
            self.content.kind(),
            self.title.clone(),
            self.created,
            ShareVersion::first(),
        )
    }

    /// The version 1 that a store holds after it writes this share.
    #[must_use]
    pub fn first_version(&self) -> SharedVersion {
        SharedVersion::new(
            self.id.clone(),
            ShareVersion::first(),
            self.content.clone(),
            self.owner.clone(),
            self.created,
        )
    }
}

impl fmt::Debug for NewShare {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewShare")
            .field("id", &self.id)
            .field("owner", &HiddenPrincipal)
            .field("title", &self.title)
            .field("content", &self.content)
            .field("created", &self.created)
            .finish()
    }
}

/// A version to publish. Only this crate builds one, so a caller cannot
/// name another person as the publisher.
#[derive(Clone, PartialEq, Eq)]
pub struct NewVersion {
    content: ShareContent,
    published_by: PrincipalId,
    created: Timestamp,
}

impl NewVersion {
    pub(crate) fn new(
        content: ShareContent,
        published_by: PrincipalId,
        created: Timestamp,
    ) -> Self {
        Self {
            content,
            published_by,
            created,
        }
    }

    /// The content.
    #[must_use]
    pub fn content(&self) -> &ShareContent {
        &self.content
    }

    /// The principal who publishes it.
    #[must_use]
    pub fn published_by(&self) -> &PrincipalId {
        &self.published_by
    }

    /// When it is published.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// The version that a store holds after it writes this one to `share`
    /// as `number`.
    #[must_use]
    pub fn shared(&self, share: ShareId, number: ShareVersion) -> SharedVersion {
        SharedVersion::new(
            share,
            number,
            self.content.clone(),
            self.published_by.clone(),
            self.created,
        )
    }
}

impl fmt::Debug for NewVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("NewVersion")
            .field("content", &self.content)
            .field("published_by", &HiddenPrincipal)
            .field("created", &self.created)
            .finish()
    }
}

/// One published version of a share. A store never changes or removes it.
#[derive(Clone, PartialEq, Eq)]
pub struct SharedVersion {
    share: ShareId,
    number: ShareVersion,
    content: ShareContent,
    published_by: PrincipalId,
    created: Timestamp,
}

impl SharedVersion {
    /// Builds a version as a store read it back.
    #[must_use]
    pub fn new(
        share: ShareId,
        number: ShareVersion,
        content: ShareContent,
        published_by: PrincipalId,
        created: Timestamp,
    ) -> Self {
        Self {
            share,
            number,
            content,
            published_by,
            created,
        }
    }

    /// The share it belongs to.
    #[must_use]
    pub fn share(&self) -> &ShareId {
        &self.share
    }

    /// Its number.
    #[must_use]
    pub fn number(&self) -> ShareVersion {
        self.number
    }

    /// Its content.
    #[must_use]
    pub fn content(&self) -> &ShareContent {
        &self.content
    }

    /// The principal who published it.
    #[must_use]
    pub fn published_by(&self) -> &PrincipalId {
        &self.published_by
    }

    /// When it was published.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }
}

impl fmt::Debug for SharedVersion {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedVersion")
            .field("share", &self.share)
            .field("number", &self.number)
            .field("content", &self.content)
            .field("published_by", &HiddenPrincipal)
            .field("created", &self.created)
            .finish()
    }
}

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
