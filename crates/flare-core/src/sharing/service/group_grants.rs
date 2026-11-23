//! The [`Shares`] methods that write and end group grants.
//!
//! | Action | Owner | Admin | Reader | Stranger |
//! | --- | --- | --- | --- | --- |
//! | Grant a team they belong to | yes | yes | forbidden | not found |
//! | Grant a team they do not belong to | team not found | team not found | forbidden | not found |
//! | Revoke a team grant | yes, any team | only a team they belong to | forbidden | not found |
//! | Grant or revoke everyone | yes | forbidden | forbidden | not found |
//!
//! A group grant is always Reader. An everyone grant records the audience
//! of the deployment's admit rule, and under a rule that admits anyone it
//! is refused with [`ShareError::EveryoneUnbounded`].

use super::{Shares, actor};
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::{Grant, GrantRevoke, GrantWrite, Granter, Revoker, Role};
use crate::name::PrincipalId;
use crate::share::ShareId;
use crate::sharing::error::ShareError;
use crate::sharing::{Authority, ShareAction, Step, decide_actor};
use crate::store::{GrantStore, ShareStore};
use crate::teams::{Audience, Group, Teams};
use crate::time::Timestamp;

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// The authority under which `actor` grants or revokes on the share.
    async fn authority(
        &self,
        actor: &PrincipalId,
        id: &ShareId,
        action: ShareAction,
    ) -> Result<Authority, ShareError> {
        let (_, standing) = self.standing(actor, id).await?;
        match decide_actor(standing, action) {
            Step::ReadTarget(authority) => Ok(authority),
            Step::Refuse(refusal) => Err(refusal.into()),
            Step::Allow => Err(ShareError::Forbidden),
        }
    }

    /// Checks that `actor` belongs to the active team of `group`, when it
    /// is a team.
    async fn member_of(&self, actor: &PrincipalId, group: &Group) -> Result<(), ShareError> {
        let Group::Team(name) = group else {
            return Ok(());
        };
        let person = Email::try_from(actor.as_str()).map_err(|_| ShareError::TeamNotFound)?;
        let unavailable = |_| ShareError::Unavailable;
        let mine = self.teams.teams_of(&person).await.map_err(unavailable)?;
        if mine.contains(name) && self.teams.exists(name).await.map_err(unavailable)? {
            Ok(())
        } else {
            Err(ShareError::TeamNotFound)
        }
    }

    /// The audience a new grant to `group` records.
    fn audience_for(&self, group: &Group) -> Result<Option<Audience>, ShareError> {
        match group {
            Group::Team(_) => Ok(None),
            Group::Everyone => self
                .admission
                .audience()
                .map(Some)
                .ok_or(ShareError::EveryoneUnbounded),
        }
    }

    /// Checks, before a share exists, that the caller as its owner could
    /// grant it to `group`.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`],
    /// [`ShareError::TeamNotFound`], [`ShareError::EveryoneUnbounded`] and
    /// [`ShareError::Unavailable`].
    pub async fn check_group(&self, who: &Principal, group: &Group) -> Result<(), ShareError> {
        let actor = actor(who)?;
        self.audience_for(group)?;
        self.member_of(&actor, group).await
    }

    /// Grants Reader on a share to a team or to everyone. A group that
    /// already holds the grant keeps it, and an everyone grant then records
    /// the current audience.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`] for a
    /// reader and for an admin who names everyone,
    /// [`ShareError::TeamNotFound`], [`ShareError::EveryoneUnbounded`],
    /// [`ShareError::GrantChanged`], [`ShareError::TooManyGrants`],
    /// [`ShareError::NoVerifiedEmail`] and [`ShareError::Unavailable`].
    pub async fn grant_group(
        &self,
        who: &Principal,
        id: &ShareId,
        group: &Group,
        at: Timestamp,
    ) -> Result<Grant, ShareError> {
        let actor = actor(who)?;
        let authority = self.authority(&actor, id, ShareAction::Grant).await?;
        if *group == Group::Everyone && authority != Authority::Owner {
            return Err(ShareError::Forbidden);
        }
        let audience = self.audience_for(group)?;
        self.member_of(&actor, group).await?;
        let grantee = group.principal_id();
        let current = self
            .grants
            .get(id, &grantee)
            .await
            .map_err(|_| ShareError::Unavailable)?
            .map(|grant| grant.role());
        let by = match authority {
            Authority::Owner => Granter::Owner(actor),
            Authority::Admin => Granter::Admin(actor),
        };
        let write = GrantWrite::new(id.clone(), grantee, Role::Reader, current, by, at)
            .with_audience(audience);
        Ok(self.grants.grant(write).await?)
    }

    /// Ends the grant of a team or of everyone. The owner ends any group
    /// grant, a dormant one included; an admin ends a grant to a team they
    /// belong to.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`],
    /// [`ShareError::TeamNotFound`], [`ShareError::GrantNotFound`],
    /// [`ShareError::GrantChanged`], [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn revoke_group(
        &self,
        who: &Principal,
        id: &ShareId,
        group: &Group,
    ) -> Result<(), ShareError> {
        let actor = actor(who)?;
        let authority = self.authority(&actor, id, ShareAction::Revoke).await?;
        let by = match authority {
            Authority::Owner => Revoker::Owner(actor),
            Authority::Admin if *group == Group::Everyone => return Err(ShareError::Forbidden),
            Authority::Admin => {
                self.member_of(&actor, group).await?;
                Revoker::Admin(actor)
            }
        };
        let grantee = group.principal_id();
        let current = self
            .grants
            .get(id, &grantee)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let expected = current
            .map(|grant| grant.role())
            .ok_or(ShareError::GrantNotFound)?;
        let revoke = GrantRevoke::new(id.clone(), grantee, expected, by);
        Ok(self.grants.revoke(revoke).await?)
    }
}
