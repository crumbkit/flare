//! Who gets an activity index entry, and when.
//!
//! An entry says that one reader can open both a response and a flare it
//! responds to, so [`Flares::activity`] reads only the caller's own entries
//! and never a response the caller cannot open. The service writes entries
//! after each write that can make a person able to open both:
//!
//! - After a link of response R to flare F: R's owner and each grantee of
//!   R who also owns F or holds a grant on F.
//! - After a grant of person P on share S, which can be both a response
//!   and a flare: for S as a response, each flare S links to that P can
//!   open; for S as a flare, each share P owns or holds a grant on that
//!   links to S.
//! - After a revoke or a leave of P on S: P's entries for S as a response
//!   are removed. A revoke of P on a flare removes nothing, because
//!   `activity` refuses that flare to P, and a new grant makes the old
//!   entries valid again.
//!
//! Each writer writes its own grant or link first and then reads the other
//! side, so of a grant and a link that run together, at least one sees
//! both and writes the entry. Markers in the store make every repeat write
//! only the missing entries, so repeating a share's grants with
//! `flare grant` repairs what a failed index write left out.
//!
//! A grant on a flare reads at most [`MAX_LISTED`](crate::MAX_LISTED)
//! owned shares and as many granted shares, newest first. A person who
//! holds more gets no entries for the older ones; only links already
//! written before the grant are affected.

use super::Flares;
use crate::email::Email;
use crate::name::PrincipalId;
use crate::response::{NewEntry, Reader, ResponseLink};
use crate::share::ShareId;
use crate::store::{GrantStore, ResponseStore, ShareStore, ShareStoreError};

/// An index write could not finish; the caller repeats the grant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Unfinished;

impl<S: ShareStore, G: GrantStore, R: ResponseStore> Flares<S, G, R> {
    /// The people who can open `share`: its owner and each grantee, or
    /// `None` when the share does not exist.
    async fn openers(&self, share: &ShareId) -> Result<Option<Vec<PrincipalId>>, Unfinished> {
        let owner = match self.shares.share_store().get(share).await {
            Ok(found) => found.owner().clone(),
            Err(ShareStoreError::NotFound) => return Ok(None),
            Err(_) => return Err(Unfinished),
        };
        let grants = self
            .shares
            .grant_store()
            .of_share(share)
            .await
            .map_err(|_| Unfinished)?;
        let mut people = Vec::with_capacity(grants.len().saturating_add(1));
        people.push(owner);
        people.extend(grants.iter().map(|grant| grant.grantee().clone()));
        Ok(Some(people))
    }

    /// Whether `person` owns `share` or holds a grant on it.
    async fn can_open(&self, person: &PrincipalId, share: &ShareId) -> Result<bool, Unfinished> {
        match self.shares.share_store().get(share).await {
            Ok(found) if found.owner() == person => return Ok(true),
            Ok(_) => {}
            Err(ShareStoreError::NotFound) => return Ok(false),
            Err(_) => return Err(Unfinished),
        }
        let grant = self
            .shares
            .grant_store()
            .get(share, person)
            .await
            .map_err(|_| Unfinished)?;
        Ok(grant.is_some())
    }

    /// Writes the entries of `link`, which is already stored: one for each
    /// person who can open both its response and its flare.
    pub(super) async fn index_link(&self, link: &ResponseLink) -> Result<(), Unfinished> {
        let (Some(readers), Some(flare_readers)) = (
            self.openers(&link.response).await?,
            self.openers(&link.flare).await?,
        ) else {
            return Ok(());
        };
        let entries = readers
            .into_iter()
            .filter(|person| flare_readers.contains(person))
            .map(|person| NewEntry::new(Reader::Person(person), link.clone()))
            .collect();
        self.write_entries(entries).await
    }

    /// Writes the entries that a grant of `person` on `share`, already
    /// stored, makes possible, with `share` as a response and as a flare.
    async fn index_grant(&self, person: &PrincipalId, share: &ShareId) -> Result<(), Unfinished> {
        let reader = Reader::Person(person.clone());
        let mut entries = Vec::new();
        let links = self
            .responses
            .of_response(share)
            .await
            .map_err(|_| Unfinished)?;
        for link in links {
            if self.can_open(person, &link.flare).await? {
                entries.push(NewEntry::new(reader.clone(), link));
            }
        }
        let held = self.held(person).await?;
        let pairs = self
            .responses
            .pairs(&held, share)
            .await
            .map_err(|_| Unfinished)?;
        entries.extend(
            pairs
                .into_iter()
                .map(|link| NewEntry::new(reader.clone(), link)),
        );
        self.write_entries(entries).await
    }

    /// Writes the entries of a grant on `share` to each person in
    /// `named`, including those who already held a grant, so repeating a
    /// grant repairs missing entries. A person in `granted` held no grant
    /// before, so any entry they have for `share` as a response is stale,
    /// as when an unindex failed; it goes first, so the response takes a
    /// new position and shows as new.
    pub(super) async fn reindex(
        &self,
        named: &[Email],
        granted: &[Email],
        share: &ShareId,
    ) -> Result<(), Unfinished> {
        for person in named {
            let id = person.principal_id();
            if granted.contains(person) {
                self.unindex(id.clone(), share).await?;
            }
            self.index_grant(&id, share).await?;
        }
        Ok(())
    }

    /// The shares `person` owns or holds a grant on, newest first in each
    /// list, owned shares first.
    ///
    /// Each list stops at [`MAX_LISTED`](crate::MAX_LISTED), so a person
    /// who holds more than 500 grants is not indexed for the older ones on
    /// a grant of a flare. Reading every grant needs a paged list, which
    /// the grant store does not offer; docs/activity.md states the limit.
    async fn held(&self, person: &PrincipalId) -> Result<Vec<ShareId>, Unfinished> {
        let owned = self
            .shares
            .share_store()
            .owned_by(person)
            .await
            .map_err(|_| Unfinished)?;
        let granted = self
            .shares
            .grant_store()
            .granted_to(person)
            .await
            .map_err(|_| Unfinished)?;
        let mut held: Vec<ShareId> = owned
            .into_items()
            .into_iter()
            .map(|share| share.id().clone())
            .collect();
        held.extend(
            granted
                .into_items()
                .into_iter()
                .map(|grant| grant.share().clone()),
        );
        Ok(held)
    }

    /// Removes the entries of `person` for `response`.
    pub(super) async fn unindex(
        &self,
        person: PrincipalId,
        response: &ShareId,
    ) -> Result<(), Unfinished> {
        self.responses
            .unindex(&Reader::Person(person), response)
            .await
            .map_err(|_| Unfinished)
    }

    async fn write_entries(&self, entries: Vec<NewEntry>) -> Result<(), Unfinished> {
        if entries.is_empty() {
            return Ok(());
        }
        self.responses.index(entries).await.map_err(|_| Unfinished)
    }
}
