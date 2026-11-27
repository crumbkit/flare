//! The [`Shares`] service: the one place that turns a request about a share
//! into a decision and a write.

mod grants;
mod group_grants;
mod groups;
mod images;
mod lists;

use super::error::ShareError;
use super::{Decision, ShareAction, Standing, Step, decide_actor};
use crate::auth::Principal;
use crate::grant::Role;
use crate::name::PrincipalId;
use crate::person::{IdentityKind, Person};
use crate::share::{
    NewShare, NewVersion, Share, ShareContent, ShareId, ShareTitle, ShareVersion, SharedVersion,
};
use crate::store::{GrantStore, ShareStore, ShareStoreError};
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
    /// Whether the grant is to a team that is not active, such as one
    /// being deleted, so it opens nothing.
    pub dormant: bool,
    /// Whether the grant is to a team that the caller neither belongs to
    /// nor owns the share of, so a listing shows a placeholder in place of
    /// the team's name.
    pub hidden: bool,
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
/// The actor is the principal's person of the deployment's
/// [`IdentityKind`], its verified address or its username, never the
/// principal's own id, so a sign-in configuration that maps one address to
/// another address-shaped id opens no share granted to the second address.
/// A principal with no person of that kind is refused with
/// [`ShareError::NoPerson`].
/// Every call reads the share and the actor's grant again, keeps no cache,
/// logs nothing, and turns every store failure into a refusal, never into
/// access. Writes name the role the service read and the authority it acts
/// under, so the store refuses a write whose grounds changed in between.
/// The service does not retry.
///
/// Anyone with a person of the kind may create a share; a caller that needs
/// a narrower rule checks it before calling [`Shares::create`].
///
/// A person also opens a share through a team that holds a grant on it, or
/// through an everyone grant whose audience covers them. The service reads
/// teams through `T` and decides everyone grants under its [`Admission`],
/// which is [`Admission::Anyone`] until the caller sets the deployment's,
/// and its `everyone_signed_in` setting, which is off until the caller
/// sets it. The identifier kind is [`IdentityKind::Email`] until the caller
/// sets the deployment's.
#[derive(Debug)]
pub struct Shares<S, G, T = NoTeams> {
    shares: S,
    grants: G,
    teams: T,
    admission: Admission,
    everyone_signed_in: bool,
    kind: IdentityKind,
}

/// The caller as the service decides by: the id that owners and grants are
/// keyed by, the person it is, and the principal, whose verified email a
/// domain audience reads.
#[derive(Debug, Clone)]
pub(super) struct Actor<'a> {
    /// The person's id.
    pub(super) id: PrincipalId,
    /// The person.
    pub(super) person: &'a Person,
    /// The principal who makes the request.
    pub(super) who: &'a Principal,
}

/// The person of `kind` that `who` signed in as.
pub(crate) fn person_of(who: &Principal, kind: IdentityKind) -> Result<&Person, ShareError> {
    who.person()
        .filter(|person| person.kind() == kind)
        .ok_or(ShareError::NoPerson(kind))
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
            everyone_signed_in: false,
            kind: IdentityKind::Email,
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
            everyone_signed_in: self.everyone_signed_in,
            kind: self.kind,
        }
    }

    /// The same service under the deployment's admit rule and its
    /// `everyone_signed_in` setting, which write and read everyone grants.
    /// With the setting on, an everyone grant under a rule that admits
    /// anyone records [`Audience::SignedIn`](crate::Audience::SignedIn)
    /// and opens to every signed-in person while the setting stays on.
    #[must_use]
    pub fn admitting(mut self, admission: Admission, everyone_signed_in: bool) -> Self {
        self.admission = admission;
        self.everyone_signed_in = everyone_signed_in;
        self
    }

    /// The same service naming people by `kind`.
    #[must_use]
    pub fn with_identity(mut self, kind: IdentityKind) -> Self {
        self.kind = kind;
        self
    }

    /// The identifier kind the service names people by.
    #[must_use]
    pub fn identity(&self) -> IdentityKind {
        self.kind
    }

    /// The caller's person of the service's kind.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoPerson`] for a principal with no person of
    /// the kind.
    pub fn person<'a>(&self, who: &'a Principal) -> Result<&'a Person, ShareError> {
        person_of(who, self.kind)
    }

    /// The caller as the service decides by.
    fn actor<'a>(&self, who: &'a Principal) -> Result<Actor<'a>, ShareError> {
        let person = self.person(who)?;
        Ok(Actor {
            id: person.principal_id(),
            person,
            who,
        })
    }

    /// The teams the service reads.
    #[must_use]
    pub fn teams(&self) -> &T {
        &self.teams
    }
}

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// The share store, for reads that work out who can open a share
    /// without a caller to check.
    pub(crate) fn share_store(&self) -> &S {
        &self.shares
    }

    /// The grant store, for reads that work out who can open a share
    /// without a caller to check.
    pub(crate) fn grant_store(&self) -> &G {
        &self.grants
    }

    /// Reads the share and works out what `actor` is to it.
    async fn standing(
        &self,
        actor: &Actor<'_>,
        id: &ShareId,
    ) -> Result<(Share, Standing), ShareError> {
        let share = self.shares.get(id).await?;
        if share.owner() == &actor.id {
            return Ok((share, Standing::Owner));
        }
        let grant = self
            .grants
            .get(id, &actor.id)
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
        actor: &Actor<'_>,
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
    /// Returns [`ShareError::NoPerson`], [`ShareError::IdTaken`],
    /// [`ShareError::TooManyShares`] or [`ShareError::Unavailable`].
    pub async fn create(
        &self,
        who: &Principal,
        id: ShareId,
        title: ShareTitle,
        content: ShareContent,
        at: Timestamp,
    ) -> Result<Share, ShareError> {
        let owner = self.actor(who)?.id;
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
    /// above the latest, [`ShareError::NoPerson`] and
    /// [`ShareError::Unavailable`].
    pub async fn open(
        &self,
        who: &Principal,
        id: &ShareId,
        number: Option<ShareVersion>,
    ) -> Result<Opened, ShareError> {
        let actor = self.actor(who)?;
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
    /// [`ShareError::NoPerson`] and [`ShareError::Unavailable`].
    pub async fn publish(
        &self,
        who: &Principal,
        id: &ShareId,
        content: ShareContent,
        at: Timestamp,
    ) -> Result<ShareVersion, ShareError> {
        let actor = self.actor(who)?;
        let (share, _) = self.admit(&actor, id, ShareAction::Publish).await?;
        if share.kind() != content.kind() {
            return Err(ShareError::KindChanged);
        }
        Ok(self
            .shares
            .publish(id, NewVersion::new(content, actor.id, at))
            .await?)
    }

    /// Checks that the caller may publish a new version of a share, which
    /// only its owner may, and writes nothing. A caller that stores files
    /// for the version checks this before the first one.
    ///
    /// # Errors
    ///
    /// Returns the refusals of [`Shares::publish`] that come before its
    /// write.
    pub async fn may_publish(&self, who: &Principal, id: &ShareId) -> Result<(), ShareError> {
        let actor = self.actor(who)?;
        self.admit(&actor, id, ShareAction::Publish).await?;
        Ok(())
    }
}
