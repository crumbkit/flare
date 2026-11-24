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
//!
//! A team grant reads when the team was created before its write and again
//! after it. When the team was deleted, or deleted and created again, in
//! between, the grant removes itself and answers
//! [`ShareError::TeamNotFound`], so a grant never outlives a delete that
//! swept the team's grants before the grant was written.

use super::{Shares, actor};
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::{Grant, GrantRevoke, GrantWrite, Granter, Revoker, Role};
use crate::name::PrincipalId;
use crate::share::ShareId;
use crate::sharing::error::ShareError;
use crate::sharing::{Authority, ShareAction, Step, decide_actor};
use crate::store::{GrantStore, GrantStoreError, ListKey, ShareStore};
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
        let since = self.team_since(group).await?;
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
        let write = GrantWrite::new(id.clone(), grantee.clone(), Role::Reader, current, by, at)
            .with_audience(audience);
        let granted = self.grants.grant(write).await?;
        if self.team_since(group).await? == since {
            return Ok(granted);
        }
        let revoke = GrantRevoke::new(id.clone(), grantee, Role::Reader, Revoker::Grantee);
        self.grants
            .revoke(revoke)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        Err(ShareError::TeamNotFound)
    }

    /// When the team of `group` was created, or `None` for everyone and for
    /// a team that is not active.
    async fn team_since(&self, group: &Group) -> Result<Option<Timestamp>, ShareError> {
        match group {
            Group::Everyone => Ok(None),
            Group::Team(name) => self
                .teams
                .active(name)
                .await
                .map_err(|_| ShareError::Unavailable),
        }
    }

    /// Ends the grant of `grantee` on `share`, read with `role`, as the
    /// grantee ending its own grant. The caller has decided that the grant
    /// ends, as when its team is deleted; no share's owner is asked. A grant whose role changed in between
    /// is ended under its new role. Answers `false` when no grant is left
    /// to end.
    pub(crate) async fn end_grant(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
        role: Role,
    ) -> Result<bool, ShareError> {
        let mut expected = role;
        // A grant changes role at most once between two reads here, so a
        // second refusal is a store that cannot settle.
        for _ in 0..2 {
            let revoke =
                GrantRevoke::new(share.clone(), grantee.clone(), expected, Revoker::Grantee);
            match self.grants.revoke(revoke).await {
                Ok(()) => return Ok(true),
                Err(GrantStoreError::Changed { current: None }) => return Ok(false),
                Err(GrantStoreError::Changed {
                    current: Some(current),
                }) => expected = current,
                Err(_) => return Err(ShareError::Unavailable),
            }
        }
        Err(ShareError::Unavailable)
    }

    /// At most `limit` grants of `group` across shares that sort after
    /// `after`, newest first, and whether more follow.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::Unavailable`].
    pub(crate) async fn group_grants(
        &self,
        group: &Group,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<(Vec<Grant>, bool), ShareError> {
        let page = self
            .grants
            .granted_page(&group.principal_id(), after, limit)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let more = page.truncated();
        Ok((page.into_items(), more))
    }

    /// Ends the grant of a team or of everyone. The owner ends any group
    /// grant, one to a team being deleted included; an admin ends a grant
    /// to a team they belong to.
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
