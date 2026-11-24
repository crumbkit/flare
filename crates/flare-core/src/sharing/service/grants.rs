//! The [`Shares`] methods that read and change grants.

use super::{Shares, actor, decided};
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::{Grant, GrantRevoke, GrantWrite, Granter, Revoker, Role};
use crate::name::PrincipalId;
use crate::share::{Share, ShareId};
use crate::sharing::error::ShareError;
use crate::sharing::{
    Authority, ShareAction, Standing, Step, Target, TargetAction, decide_actor, decide_target,
};
use crate::store::{GrantStore, ShareStore};
use crate::teams::Teams;
use crate::time::Timestamp;

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// Reads what `person` is to `share`, as the target of a grant or a
    /// revoke.
    async fn target(&self, share: &Share, person: &PrincipalId) -> Result<Target, ShareError> {
        if share.owner() == person {
            return Ok(Target::Owner);
        }
        let grant = self
            .grants
            .get(share.id(), person)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        Ok(Target::Person(grant.map(|grant| grant.role())))
    }

    /// Decides a grant or a revoke of `person` by `actor`, after the actor
    /// step, and answers the authority the actor acts under and the
    /// target's role as it was read.
    async fn decide_on_target(
        &self,
        actor: &PrincipalId,
        id: &ShareId,
        action: ShareAction,
        on: TargetAction,
        person: &PrincipalId,
    ) -> Result<(Authority, Option<Role>), ShareError> {
        let (share, standing) = self.standing(actor, id).await?;
        let authority = match decide_actor(standing, action) {
            Step::ReadTarget(authority) => authority,
            Step::Refuse(refusal) => return Err(refusal.into()),
            // A grant or a revoke always depends on its target.
            Step::Allow => return Err(ShareError::Forbidden),
        };
        let target = self.target(&share, person).await?;
        decided(decide_target(authority, on, target))?;
        match target {
            Target::Person(role) => Ok((authority, role)),
            Target::Owner => Err(ShareError::OwnerCannotBeChanged),
        }
    }

    /// Gives the person with the address `grantee` the role `role` on a
    /// share, or changes their role. The write applies only if their grant
    /// is still the one the service read, and, for an admin, only while
    /// the admin still holds the Admin grant.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`] for a
    /// caller who lacks the right or an admin whose grant ended,
    /// [`ShareError::OwnerCannotBeChanged`] when `grantee` is the owner,
    /// [`ShareError::GrantChanged`], [`ShareError::TooManyGrants`],
    /// [`ShareError::NoVerifiedEmail`] and [`ShareError::Unavailable`].
    pub async fn grant(
        &self,
        who: &Principal,
        id: &ShareId,
        grantee: &Email,
        role: Role,
        at: Timestamp,
    ) -> Result<Grant, ShareError> {
        let actor = actor(who)?;
        let person = grantee.principal_id();
        let on = TargetAction::Grant(role);
        let (authority, expected) = self
            .decide_on_target(&actor, id, ShareAction::Grant, on, &person)
            .await?;
        let by = match authority {
            Authority::Owner => Granter::Owner(actor),
            Authority::Admin => Granter::Admin(actor),
        };
        let write = GrantWrite::new(id.clone(), person, role, expected, by, at);
        Ok(self.grants.grant(write).await?)
    }

    /// Ends the grant of the person with the address `grantee`. When
    /// `grantee` is the caller's own address, the caller leaves the share.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`],
    /// [`ShareError::OwnerCannotBeChanged`] when `grantee` is the owner,
    /// [`ShareError::GrantNotFound`] when `grantee` holds no grant,
    /// [`ShareError::GrantChanged`], [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn revoke(
        &self,
        who: &Principal,
        id: &ShareId,
        grantee: &Email,
    ) -> Result<(), ShareError> {
        let actor = actor(who)?;
        let person = grantee.principal_id();
        if person == actor {
            return self.leave(&actor, id).await;
        }
        let (authority, current) = self
            .decide_on_target(
                &actor,
                id,
                ShareAction::Revoke,
                TargetAction::Revoke,
                &person,
            )
            .await?;
        let expected = current.ok_or(ShareError::GrantNotFound)?;
        let by = match authority {
            Authority::Owner => Revoker::Owner(actor),
            Authority::Admin => Revoker::Admin(actor),
        };
        let revoke = GrantRevoke::new(id.clone(), person, expected, by);
        Ok(self.grants.revoke(revoke).await?)
    }

    /// Ends the actor's own grant.
    async fn leave(&self, actor: &PrincipalId, id: &ShareId) -> Result<(), ShareError> {
        let (_, standing) = self.admit(actor, id, ShareAction::Leave).await?;
        let expected = match standing {
            Standing::Admin => Role::Admin,
            Standing::Reader => Role::Reader,
            Standing::Owner => return Err(ShareError::OwnerCannotBeChanged),
            Standing::Stranger => return Err(ShareError::NotFound),
        };
        let revoke = GrantRevoke::new(id.clone(), actor.clone(), expected, Revoker::Grantee);
        Ok(self.grants.revoke(revoke).await?)
    }

    /// The grants of a share, for its owner and its admins.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`] for a
    /// reader, [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn grants(&self, who: &Principal, id: &ShareId) -> Result<Vec<Grant>, ShareError> {
        let actor = actor(who)?;
        self.admit(&actor, id, ShareAction::ListGrants).await?;
        self.grants
            .of_share(id)
            .await
            .map_err(|_| ShareError::Unavailable)
    }
}
