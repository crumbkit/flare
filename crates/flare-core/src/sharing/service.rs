//! The [`Shares`] service: the one place that turns a request about a share
//! into a decision and a write.

mod grants;
mod group_grants;
mod groups;

use super::error::ShareError;
use super::{Decision, ShareAction, Standing, Step, decide_actor};
use crate::auth::Principal;
use crate::file::{FileId, StoredFile};
use crate::grant::Role;
use crate::name::PrincipalId;
use crate::share::{
    Listing, NewShare, NewVersion, Share, ShareContent, ShareId, ShareTitle, ShareVersion,
    SharedVersion,
};
use crate::store::{FileStore, GrantStore, ShareStore, ShareStoreError};
use crate::teams::{Admission, NoTeams, Teams};
use crate::time::Timestamp;

/// One version of a share that a caller opened, with what the caller is to
/// the share.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Opened {
    /// The share.
    pub share: Share,
    /// The version asked for, or the latest.
    pub version: SharedVersion,
    /// What the caller is to the share.
    pub standing: Standing,
}

/// One grant of a share as its owner or an admin lists it.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ListedGrant {
    /// The grant, to a person or a group.
    pub grant: crate::grant::Grant,
    /// Whether the grant is to a deleted team, so it opens nothing.
    pub dormant: bool,
}

/// A share that another person granted to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct SharedWithMe {
    /// The share.
    pub share: Share,
    /// The caller's role on it.
    pub role: Role,
}

/// Decides and applies every request about a share, from the principal, the
/// share and the grants.
///
/// The actor is the principal's verified email address, never the
/// principal's own id, so a sign-in configuration that maps one address to
/// another address-shaped id opens no share granted to the second address.
/// Every call reads the share and the actor's grant again, keeps no cache,
/// logs nothing, and turns every store failure into a refusal, never into
/// access. Writes name the role the service read and the authority it acts
/// under, so the store refuses a write whose grounds changed in between.
/// The service does not retry.
///
/// Anyone with a verified address may create a share; a caller that needs
/// a narrower rule checks it before calling [`Shares::create`].
///
/// A person also opens a share through a team that holds a grant on it, or
/// through an everyone grant whose audience covers them. The service reads
/// teams through `T` and decides everyone grants under its [`Admission`],
/// which is [`Admission::Anyone`] until the caller sets the deployment's.
#[derive(Debug)]
pub struct Shares<S, G, T = NoTeams> {
    shares: S,
    grants: G,
    teams: T,
    admission: Admission,
}

/// The actor's id: the principal's verified address.
fn actor(who: &Principal) -> Result<PrincipalId, ShareError> {
    who.email()
        .map(crate::Email::principal_id)
        .ok_or(ShareError::NoVerifiedEmail)
}

/// Applies a first-step answer for an action that needs no target.
fn allowed(step: Step) -> Result<(), ShareError> {
    match step {
        Step::Allow => Ok(()),
        Step::Refuse(refusal) => Err(refusal.into()),
        // Only a grant or a revoke reads a target; any other action that
        // asks for one is refused.
        Step::ReadTarget(_) => Err(ShareError::Forbidden),
    }
}

/// Applies a second-step answer.
fn decided(decision: Decision) -> Result<(), ShareError> {
    match decision {
        Decision::Allow => Ok(()),
        Decision::Refuse(refusal) => Err(refusal.into()),
    }
}

impl<S, G> Shares<S, G> {
    /// A service over a share store and a grant store, with no teams.
    #[must_use]
    pub fn new(shares: S, grants: G) -> Self {
        Self {
            shares,
            grants,
            teams: NoTeams,
            admission: Admission::Anyone,
        }
    }
}

impl<S, G, T> Shares<S, G, T> {
    /// The same service, reading teams through `teams`.
    #[must_use]
    pub fn with_teams<U>(self, teams: U) -> Shares<S, G, U> {
        Shares {
            shares: self.shares,
            grants: self.grants,
            teams,
            admission: self.admission,
        }
    }

    /// The same service under the deployment's admit rule, which writes
    /// and reads everyone grants.
    #[must_use]
    pub fn admitting(mut self, admission: Admission) -> Self {
        self.admission = admission;
        self
    }

    /// The teams the service reads.
    #[must_use]
    pub fn teams(&self) -> &T {
        &self.teams
    }
}

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// Reads the share and works out what `actor` is to it.
    async fn standing(
        &self,
        actor: &PrincipalId,
        id: &ShareId,
    ) -> Result<(Share, Standing), ShareError> {
        let share = self.shares.get(id).await?;
        if share.owner() == actor {
            return Ok((share, Standing::Owner));
        }
        let grant = self
            .grants
            .get(id, actor)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let standing = match grant.map(|grant| grant.role()) {
            Some(Role::Admin) => Standing::Admin,
            Some(Role::Reader) => Standing::Reader,
            // A group grant gives Reader at most, so it is read only when
            // the actor holds no grant of their own.
            None if self.opens_through_group(actor, id).await? => Standing::Reader,
            None => Standing::Stranger,
        };
        Ok((share, standing))
    }

    /// Reads the share and checks that `actor` may do `action` to it.
    async fn admit(
        &self,
        actor: &PrincipalId,
        id: &ShareId,
        action: ShareAction,
    ) -> Result<(Share, Standing), ShareError> {
        let (share, standing) = self.standing(actor, id).await?;
        allowed(decide_actor(standing, action))?;
        Ok((share, standing))
    }

    /// Creates a share owned by the caller, with `content` as its version 1.
    ///
    /// The caller draws `id` at random on the server and never takes it
    /// from a client. This method answers [`ShareError::IdTaken`] for an id
    /// that exists whatever the caller's standing in that share, so an id a
    /// client chose would let anyone signed in learn whether a share
    /// exists. On `IdTaken`, the caller draws a new id and tries again.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`], [`ShareError::IdTaken`],
    /// [`ShareError::TooManyShares`] or [`ShareError::Unavailable`].
    pub async fn create(
        &self,
        who: &Principal,
        id: ShareId,
        title: ShareTitle,
        content: ShareContent,
        at: Timestamp,
    ) -> Result<Share, ShareError> {
        let owner = actor(who)?;
        Ok(self
            .shares
            .create(NewShare::new(id, owner, title, content, at))
            .await?)
    }

    /// Opens version `number` of a share, or its latest when `number` is
    /// `None`.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`] for a missing share and for a caller
    /// with no standing in it, [`ShareError::VersionNotFound`] for a number
    /// above the latest, [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn open(
        &self,
        who: &Principal,
        id: &ShareId,
        number: Option<ShareVersion>,
    ) -> Result<Opened, ShareError> {
        let actor = actor(who)?;
        let (share, standing) = self.admit(&actor, id, ShareAction::Read).await?;
        let number = number.unwrap_or_else(|| share.latest());
        if number > share.latest() {
            return Err(ShareError::VersionNotFound);
        }
        let version = self
            .shares
            .version(id, number)
            .await
            .map_err(|error| match error {
                ShareStoreError::NotFound => ShareError::VersionNotFound,
                _ => ShareError::Unavailable,
            })?;
        Ok(Opened {
            share,
            version,
            standing,
        })
    }

    /// Publishes `content` as the next version of a share that the caller
    /// owns.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`] for an
    /// admin or a reader, [`ShareError::TooManyVersions`],
    /// [`ShareError::NoVerifiedEmail`] and [`ShareError::Unavailable`].
    pub async fn publish(
        &self,
        who: &Principal,
        id: &ShareId,
        content: ShareContent,
        at: Timestamp,
    ) -> Result<ShareVersion, ShareError> {
        let actor = actor(who)?;
        self.admit(&actor, id, ShareAction::Publish).await?;
        Ok(self
            .shares
            .publish(id, NewVersion::new(content, actor, at))
            .await?)
    }

    /// The shares that the caller owns, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn mine(&self, who: &Principal) -> Result<Listing<Share>, ShareError> {
        let actor = actor(who)?;
        self.shares
            .owned_by(&actor)
            .await
            .map_err(|_| ShareError::Unavailable)
    }

    /// Reads a file of a share that the caller may read.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`] for a missing share, a caller with
    /// no standing in it and a file the share does not hold,
    /// [`ShareError::NoVerifiedEmail`] and [`ShareError::Unavailable`].
    pub async fn file<F: FileStore>(
        &self,
        who: &Principal,
        files: &F,
        id: &ShareId,
        file: &FileId,
    ) -> Result<StoredFile, ShareError> {
        let actor = actor(who)?;
        self.admit(&actor, id, ShareAction::Read).await?;
        Ok(files.get(id, file).await?)
    }
}
