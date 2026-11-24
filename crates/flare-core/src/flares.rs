//! [`Flares`]: sharing a flare with named people, marking it as a response
//! to other flares with `--re`, and reading the responses to a flare since
//! a cursor.
//!
//! [`Flares`] wraps [`Shares`], which still decides every read and grant,
//! and adds the two rules this layer owns: only people the [`Creators`]
//! rule admits may create a share, responses included, and a response is
//! listed only to a caller who can open both the response and the flare it
//! responds to. A refused flare and a missing flare give the same
//! [`FlareError::NotFound`].
//!
//! `share` checks in this order and writes nothing until every check has
//! passed: a verified address, the creators rule, then that the caller can
//! open each `--re` flare. It then creates the share, grants each
//! recipient, and writes the response links and their activity index
//! entries. A write that fails after the share exists answers
//! [`FlareError::PartlyShared`], which names the share, so the owner
//! finishes with [`Flares::grant`].
//!
//! A response is always shared with the owner of each flare it responds
//! to: before the links are written, the owner of each `--re` flare is
//! granted Reader on the response, unless they already hold a role or own
//! it. The responder never has to name them with `--with`.

mod activity;
mod comments;
mod error;
mod index;
mod owners;
mod people;
mod revoke;

pub use activity::{ActivityItem, ActivityPage, CommentActivity, MAX_EXAMINED, ResponseActivity};
pub use comments::{CommentDraft, CommentThread, Posted};
pub use error::FlareError;
pub use people::{
    AddressesOnly, CreatorDomain, Creators, NamedRecipients, RecipientError, Recipients,
};

use crate::auth::Principal;
use crate::email::Email;
use crate::response::NewLink;
use crate::share::{Share, ShareContent, ShareId, ShareTitle};
use crate::sharing::{ShareError, Shares, Standing};
use crate::store::{CommentStore, CursorStore, GrantStore, ResponseStore, ShareStore};
use crate::time::Timestamp;
use crate::{PrincipalId, Role, ShareVersion};
use index::Unfinished;

/// The most flares one share responds to, and one activity call reads.
pub const MAX_FLARES: usize = 16;

/// A flare to share.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct NewFlare {
    id: ShareId,
    title: ShareTitle,
    content: ShareContent,
    with: Vec<Email>,
    re: Vec<ShareId>,
}

impl NewFlare {
    /// A flare with the server-drawn `id`, shared with `with` and
    /// responding to each flare in `re`. Repeats in either list count once.
    #[must_use]
    pub fn new(
        id: ShareId,
        title: ShareTitle,
        content: ShareContent,
        with: Vec<Email>,
        re: Vec<ShareId>,
    ) -> Self {
        Self {
            id,
            title,
            content,
            with: once_each(with),
            re: once_each(re),
        }
    }
}

/// What `share` created.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Flared {
    /// The new share.
    pub share: Share,
    /// The people granted Reader.
    pub granted: Vec<Email>,
    /// The flares it responds to.
    pub responds_to: Vec<ShareId>,
}

/// A flare a share responds to, as the caller opened it.
struct Responded {
    flare: ShareId,
    version: ShareVersion,
    owner: PrincipalId,
}

/// The service: [`Shares`] with the creators rule and the response store.
#[derive(Debug)]
pub struct Flares<S, G, R, C, U> {
    shares: Shares<S, G>,
    responses: R,
    comments: C,
    cursors: U,
    creators: Creators,
}

/// `items` in their first order, each once.
fn once_each<T: PartialEq>(items: Vec<T>) -> Vec<T> {
    let mut kept = Vec::with_capacity(items.len());
    for item in items {
        if !kept.contains(&item) {
            kept.push(item);
        }
    }
    kept
}

/// A refusal to open a named flare, where missing and refused are one
/// answer.
fn flare_refused(error: ShareError) -> FlareError {
    match error {
        ShareError::NotFound | ShareError::VersionNotFound => FlareError::NotFound,
        other => FlareError::Share(other),
    }
}

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C: CommentStore, U: CursorStore>
    Flares<S, G, R, C, U>
{
    /// A service over the five stores, admitting `creators`. The response
    /// and comment stores share one position sequence.
    #[must_use]
    pub fn new(
        shares: S,
        grants: G,
        responses: R,
        comments: C,
        cursors: U,
        creators: Creators,
    ) -> Self {
        Self {
            shares: Shares::new(shares, grants),
            responses,
            comments,
            cursors,
            creators,
        }
    }

    /// The share service, for reads and lists. A grant or a revoke made
    /// through it writes no activity index entry; use [`Flares::grant`]
    /// and [`Flares::revoke`].
    #[must_use]
    pub fn shares(&self) -> &Shares<S, G> {
        &self.shares
    }

    /// The response store, for tests that read the index. It is not built
    /// into a server, so no caller there can write the index around
    /// [`Flares`].
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn responses(&self) -> &R {
        &self.responses
    }

    /// Checks that `who` has a verified address and is a creator.
    fn creator(&self, who: &Principal) -> Result<(), FlareError> {
        if who.email().is_none() {
            return Err(FlareError::Share(ShareError::NoVerifiedEmail));
        }
        if self.creators.admits(who) {
            Ok(())
        } else {
            Err(FlareError::NotACreator)
        }
    }

    /// The latest version of each flare in `re`, which `who` must be able
    /// to open.
    async fn openable(
        &self,
        who: &Principal,
        re: &[ShareId],
    ) -> Result<Vec<Responded>, FlareError> {
        if re.len() > MAX_FLARES {
            return Err(FlareError::TooManyFlares);
        }
        let mut versions = Vec::with_capacity(re.len());
        for flare in re {
            let opened = self
                .shares
                .open(who, flare, None)
                .await
                .map_err(flare_refused)?;
            versions.push(Responded {
                flare: flare.clone(),
                version: opened.share.latest(),
                owner: opened.share.owner().clone(),
            });
        }
        Ok(versions)
    }

    /// Writes a link from `response` to each flare, then the index entries
    /// of each link. A repeated link keeps its first position, and its
    /// entries are written again where they are missing.
    async fn link(
        &self,
        response: &ShareId,
        flares: &[Responded],
        at: Timestamp,
    ) -> Result<(), Unfinished> {
        for Responded { flare, version, .. } in flares {
            let link = NewLink::new(response.clone(), flare.clone(), *version, at);
            let stored = self.responses.link(link).await.map_err(|_| Unfinished)?;
            self.index_link(&stored).await?;
        }
        Ok(())
    }

    /// Creates a share owned by the caller, grants Reader to each recipient
    /// and records each `--re` flare as one it responds to.
    ///
    /// # Errors
    ///
    /// Before any write: [`FlareError::Share`] with
    /// [`ShareError::NoVerifiedEmail`], [`FlareError::NotACreator`],
    /// [`FlareError::TooManyFlares`] and [`FlareError::NotFound`] for a
    /// `--re` flare the caller cannot open. The create's own refusals as
    /// [`FlareError::Share`]. After the share exists,
    /// [`FlareError::PartlyShared`].
    pub async fn share(
        &self,
        who: &Principal,
        flare: NewFlare,
        at: Timestamp,
    ) -> Result<Flared, FlareError> {
        self.creator(who)?;
        let flares = self.openable(who, &flare.re).await?;
        let share = self
            .shares
            .create(who, flare.id, flare.title, flare.content, at)
            .await
            .map_err(FlareError::Share)?;
        let partly = |granted, cause| FlareError::PartlyShared {
            share: share.id().clone(),
            granted,
            cause,
        };
        let mut granted = Vec::with_capacity(flare.with.len());
        for person in flare.with {
            if who.email() == Some(&person) {
                continue;
            }
            match self
                .shares
                .grant(who, share.id(), &person, Role::Reader, at)
                .await
            {
                Ok(_) => granted.push(person),
                Err(cause) => return Err(partly(granted.len(), cause)),
            }
        }
        if let Err(cause) = self.grant_owners(who, share.id(), &flares, at).await {
            return Err(partly(granted.len(), cause));
        }
        if self.link(share.id(), &flares, at).await.is_err() {
            return Err(partly(granted.len(), ShareError::Unavailable));
        }
        Ok(Flared {
            share,
            granted,
            responds_to: flare.re,
        })
    }

    /// Grants Reader on an existing share to each person in `with` who
    /// holds no grant yet, and, for the share's owner, records each flare
    /// in `re` as one it responds to. A person who already holds a role
    /// keeps it, so an admin is never demoted.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::Share`] with the share service's refusal,
    /// [`ShareError::Forbidden`] when someone other than the owner names
    /// `re`, [`FlareError::NotACreator`], [`FlareError::NotFound`] for a
    /// `re` flare the caller cannot open, and [`FlareError::PartlyShared`]
    /// when a write fails after an earlier one.
    pub async fn grant(
        &self,
        who: &Principal,
        id: &ShareId,
        with: &[Email],
        re: &[ShareId],
        at: Timestamp,
    ) -> Result<Flared, FlareError> {
        let opened = self
            .shares
            .open(who, id, None)
            .await
            .map_err(FlareError::Share)?;
        let re = once_each(re.to_vec());
        let flares = if re.is_empty() {
            Vec::new()
        } else {
            if opened.standing != Standing::Owner {
                return Err(FlareError::Share(ShareError::Forbidden));
            }
            self.creator(who)?;
            self.openable(who, &re).await?
        };
        let mut granted = Vec::new();
        let named = once_each(with.to_vec());
        if !named.is_empty() {
            let current = self
                .shares
                .grants(who, id)
                .await
                .map_err(FlareError::Share)?;
            for person in &named {
                let held = current
                    .iter()
                    .any(|grant| grant.grantee() == &person.principal_id());
                let owner = opened.share.owner() == &person.principal_id();
                if held || owner {
                    continue;
                }
                match self.shares.grant(who, id, person, Role::Reader, at).await {
                    Ok(_) => granted.push(person.clone()),
                    Err(cause) if granted.is_empty() => return Err(FlareError::Share(cause)),
                    Err(cause) => {
                        return Err(FlareError::PartlyShared {
                            share: id.clone(),
                            granted: granted.len(),
                            cause,
                        });
                    }
                }
            }
        }
        if self.reindex(&named, &granted, id).await.is_err() {
            return Err(FlareError::PartlyShared {
                share: id.clone(),
                granted: granted.len(),
                cause: ShareError::Unavailable,
            });
        }
        if let Err(cause) = self.grant_owners(who, id, &flares, at).await {
            if granted.is_empty() {
                return Err(FlareError::Share(cause));
            }
            return Err(FlareError::PartlyShared {
                share: id.clone(),
                granted: granted.len(),
                cause,
            });
        }
        if self.link(id, &flares, at).await.is_err() {
            return Err(FlareError::PartlyShared {
                share: id.clone(),
                granted: granted.len(),
                cause: ShareError::Unavailable,
            });
        }
        Ok(Flared {
            share: opened.share,
            granted,
            responds_to: re,
        })
    }
}

#[cfg(test)]
mod tests;
