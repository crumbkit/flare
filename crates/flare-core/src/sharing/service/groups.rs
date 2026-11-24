//! The [`Shares`] methods that read group grants: whether a person opens a
//! share through a team or an everyone grant, and the grants of a share
//! with their state.

use super::{ListedGrant, Shares, actor};
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::{Grant, Role};
use crate::name::PrincipalId;
use crate::share::ShareId;
use crate::sharing::error::ShareError;
use crate::sharing::{ShareAction, Standing};
use crate::store::{GrantStore, ShareStore};
use crate::teams::{Group, TeamName, Teams};

/// The rank of a role, so the higher of two grants of one share wins.
pub(super) fn rank(role: Role) -> u8 {
    match role {
        Role::Admin => 2,
        Role::Reader => 1,
    }
}

/// The address an actor's id holds. Every actor id is the lower-case
/// address it came from, so it parses back.
pub(super) fn address(actor: &PrincipalId) -> Option<Email> {
    Email::try_from(actor.as_str()).ok()
}

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// The active teams that `person` belongs to.
    pub(super) async fn active_teams(&self, person: &Email) -> Result<Vec<TeamName>, ShareError> {
        let mut active = Vec::new();
        let names = self
            .teams
            .teams_of(person)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        for name in names {
            if self
                .teams
                .exists(&name)
                .await
                .map_err(|_| ShareError::Unavailable)?
            {
                active.push(name);
            }
        }
        Ok(active)
    }

    /// Whether `grant`, an everyone grant, opens its share to `person`.
    pub(super) fn everyone_opens(&self, grant: &Grant, person: &Email) -> bool {
        grant
            .audience()
            .is_some_and(|audience| self.admission.opens(audience, person))
    }

    /// Whether the actor opens the share through one of its group grants.
    /// The share's group grants are read first, so a share with none never
    /// reads the teams. A failed read is `Unavailable` and never a partial
    /// answer.
    pub(super) async fn opens_through_group(
        &self,
        actor: &PrincipalId,
        id: &ShareId,
    ) -> Result<bool, ShareError> {
        let Some(person) = address(actor) else {
            return Ok(false);
        };
        let grants = self
            .grants
            .of_share(id)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let mut teams: Option<Vec<TeamName>> = None;
        for grant in &grants {
            match Group::of_grantee(grant.grantee()) {
                Some(Group::Everyone) if self.everyone_opens(grant, &person) => return Ok(true),
                Some(Group::Team(name)) => {
                    if teams.is_none() {
                        teams = Some(self.active_teams(&person).await?);
                    }
                    if teams.as_ref().is_some_and(|teams| teams.contains(&name)) {
                        return Ok(true);
                    }
                }
                _ => {}
            }
        }
        Ok(false)
    }

    /// The groups through which `person` reads `id`: each team grant on it
    /// of an active team the person belongs to, and its everyone grant when
    /// the grant's audience covers them. The teams are read only when the
    /// share has a team grant.
    pub(crate) async fn groups_reading(
        &self,
        person: &Email,
        id: &ShareId,
    ) -> Result<Vec<Group>, ShareError> {
        let grants = self
            .grants
            .of_share(id)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let mut teams: Option<Vec<TeamName>> = None;
        let mut groups = Vec::new();
        for grant in &grants {
            match Group::of_grantee(grant.grantee()) {
                Some(Group::Everyone) if self.everyone_opens(grant, person) => {
                    groups.push(Group::Everyone);
                }
                Some(Group::Team(name)) => {
                    if teams.is_none() {
                        teams = Some(self.active_teams(person).await?);
                    }
                    if teams.as_ref().is_some_and(|teams| teams.contains(&name)) {
                        groups.push(Group::Team(name));
                    }
                }
                _ => {}
            }
        }
        Ok(groups)
    }

    /// The grants of a share, for its owner and its admins, each marked
    /// dormant when it is a grant to a deleted team. For an admin, a grant
    /// to a team the admin does not belong to is marked hidden, so no one
    /// outside a team learns its name.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`] for a
    /// reader, [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn listed_grants(
        &self,
        who: &Principal,
        id: &ShareId,
    ) -> Result<Vec<ListedGrant>, ShareError> {
        let actor = actor(who)?;
        let (_, standing) = self.admit(&actor, id, ShareAction::ListGrants).await?;
        let mine = if standing == Standing::Owner {
            None
        } else {
            let person = address(&actor).ok_or(ShareError::NoVerifiedEmail)?;
            Some(self.active_teams(&person).await?)
        };
        let grants = self
            .grants
            .of_share(id)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let mut listed = Vec::with_capacity(grants.len());
        for grant in grants {
            let (dormant, hidden) = match Group::of_grantee(grant.grantee()) {
                Some(Group::Team(name)) => {
                    let dormant = !self
                        .teams
                        .exists(&name)
                        .await
                        .map_err(|_| ShareError::Unavailable)?;
                    let hidden = mine.as_ref().is_some_and(|mine| !mine.contains(&name));
                    (dormant, hidden)
                }
                _ => (false, false),
            };
            listed.push(ListedGrant {
                grant,
                dormant,
                hidden,
            });
        }
        Ok(listed)
    }
}
