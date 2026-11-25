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
mod groups;
mod index;
mod owners;
mod people;
mod team_life;

pub use activity::{ActivityItem, ActivityPage, CommentActivity, MAX_EXAMINED, ResponseActivity};
pub use comments::{CommentDraft, CommentThread, Posted};
pub use error::FlareError;
pub use groups::Revoked;
pub use people::{
    AddressesOnly, AllAdmitted, CreatorDomain, CreatorList, Creators, CreatorsError,
    NamedRecipients, RecipientError, Recipients,
};
pub use team_life::{MAX_SWEPT, TeamDeletion};

use crate::auth::Principal;
use crate::email::Email;
use crate::file::VersionImages;
use crate::response::NewLink;
use crate::share::{Share, ShareContent, ShareId, ShareTitle};
use crate::sharing::{ShareError, Shares};
use crate::store::{CommentStore, CursorStore, GrantStore, ResponseStore, ShareStore};
use crate::teams::{Admission, Group, NoTeams, Teams};
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
    groups: Vec<Group>,
    re: Vec<ShareId>,
    images: VersionImages,
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
            groups: Vec::new(),
            re: once_each(re),
            images: VersionImages::none(),
        }
    }

    /// The same flare, whose version 1 shows `images`. The caller has
    /// stored each file under the flare's id first.
    #[must_use]
    pub fn with_images(mut self, images: VersionImages) -> Self {
        self.images = images;
        self
    }

    /// The same flare, also granted to each team or to everyone in
    /// `groups`. Repeats count once.
    #[must_use]
    pub fn with_groups(mut self, groups: Vec<Group>) -> Self {
        self.groups = once_each(groups);
        self
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
    /// The teams, or everyone, granted Reader.
    pub groups: Vec<Group>,
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
/// `T` reads and manages teams, and `K` is the app's [`Creators`] rule.
#[derive(Debug)]
pub struct Flares<S, G, R, C, U, T = NoTeams, K = AllAdmitted> {
    shares: Shares<S, G, T>,
    responses: R,
    comments: C,
    cursors: U,
    creators: K,
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

impl<S, G, R, C, U, K> Flares<S, G, R, C, U, NoTeams, K> {
    /// A service over the five stores, whose `creators` rule says who may
    /// create, with no teams. The response and comment stores share one
    /// position sequence.
    #[must_use]
    pub fn new(shares: S, grants: G, responses: R, comments: C, cursors: U, creators: K) -> Self {
        Self {
            shares: Shares::new(shares, grants),
            responses,
            comments,
            cursors,
            creators,
        }
    }
}

impl<S, G, R, C, U, T, K> Flares<S, G, R, C, U, T, K> {
    /// The same service, reading and managing teams through `teams`.
    #[must_use]
    pub fn with_teams<V>(self, teams: V) -> Flares<S, G, R, C, U, V, K> {
        Flares {
            shares: self.shares.with_teams(teams),
            responses: self.responses,
            comments: self.comments,
            cursors: self.cursors,
            creators: self.creators,
        }
    }

    /// The same service under the deployment's admit rule.
    #[must_use]
    pub fn admitting(mut self, admission: Admission) -> Self {
        self.shares = self.shares.admitting(admission);
        self
    }

    /// The share service, for reads and lists. A grant or a revoke made
    /// through it writes no activity index entry; use [`Flares::grant`]
    /// and [`Flares::revoke`].
    #[must_use]
    pub fn shares(&self) -> &Shares<S, G, T> {
        &self.shares
    }
}

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C: CommentStore, U: CursorStore, T: Teams, K>
    Flares<S, G, R, C, U, T, K>
{
    /// The response store, for tests that read the index. It is not built
    /// into a server, so no caller there can write the index around
    /// [`Flares`].
    #[cfg(any(test, feature = "testing"))]
    #[must_use]
    pub fn responses(&self) -> &R {
        &self.responses
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
}

impl<
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: Teams,
    K: Creators,
> Flares<S, G, R, C, U, T, K>
{
    /// Checks that `who` has a verified address and is a creator.
    async fn creator(&self, who: &Principal) -> Result<(), FlareError> {
        if who.email().is_none() {
            return Err(FlareError::Share(ShareError::NoVerifiedEmail));
        }
        match self.creators.may_create(who).await {
            Ok(true) => Ok(()),
            Ok(false) => Err(FlareError::NotACreator),
            Err(_) => Err(FlareError::CreatorsUnavailable),
        }
    }

    /// Runs every check of [`Flares::share`] that comes before its first
    /// write, and writes nothing. A caller that stores files for the flare
    /// runs it before the first one.
    ///
    /// # Errors
    ///
    /// Returns the refusals that [`Flares::share`] gives before any write.
    pub async fn may_share(&self, who: &Principal, flare: &NewFlare) -> Result<(), FlareError> {
        self.creator(who).await?;
        for group in &flare.groups {
            self.shares
                .check_group(who, group)
                .await
                .map_err(FlareError::Share)?;
        }
        self.openable(who, &flare.re).await.map(|_| ())
    }

    /// Creates a share owned by the caller, grants Reader to each recipient
    /// and records each `--re` flare as one it responds to.
    ///
    /// # Errors
    ///
    /// Before any write: [`FlareError::Share`] with
    /// [`ShareError::NoVerifiedEmail`], [`FlareError::NotACreator`],
    /// [`FlareError::CreatorsUnavailable`],
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
        self.creator(who).await?;
        for group in &flare.groups {
            self.shares
                .check_group(who, group)
                .await
                .map_err(FlareError::Share)?;
        }
        let flares = self.openable(who, &flare.re).await?;
        let share = self
            .shares
            .create_with_images(who, flare.id, flare.title, flare.content, flare.images, at)
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
        for (index, group) in flare.groups.iter().enumerate() {
            if let Err(cause) = self.shares.grant_group(who, share.id(), group, at).await {
                return Err(partly(granted.len() + index, cause));
            }
        }
        let written = granted.len() + flare.groups.len();
        if let Err(cause) = self.grant_owners(who, share.id(), &flares, at).await {
            return Err(partly(written, cause));
        }
        if self.link(share.id(), &flares, at).await.is_err() {
            return Err(partly(written, ShareError::Unavailable));
        }
        Ok(Flared {
            share,
            granted,
            groups: flare.groups,
            responds_to: flare.re,
        })
    }
}

#[cfg(test)]
mod tests;
