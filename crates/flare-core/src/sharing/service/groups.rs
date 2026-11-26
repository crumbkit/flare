//! The [`Shares`] methods that read group grants: whether a person opens a
//! share through a team or an everyone grant, and the grants of a share
//! with their state.

use super::{Actor, ListedGrant, Shares};
use crate::auth::Principal;
use crate::grant::{Grant, Role};
use crate::person::Person;
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

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// The active teams that `person` belongs to.
    pub(super) async fn active_teams(&self, person: &Person) -> Result<Vec<TeamName>, ShareError> {
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

    /// Whether `grant`, an everyone grant, opens its share to `who`.
    pub(super) fn everyone_opens(&self, grant: &Grant, who: &Principal) -> bool {
        grant
            .audience()
            .is_some_and(|audience| self.admission.opens(audience, who, self.everyone_signed_in))
    }

    /// Whether the actor opens the share through one of its group grants.
    /// The share's group grants are read first, so a share with none never
    /// reads the teams. A failed read is `Unavailable` and never a partial
    /// answer.
    pub(super) async fn opens_through_group(
        &self,
        actor: &Actor<'_>,
        id: &ShareId,
    ) -> Result<bool, ShareError> {
        let grants = self
            .grants
            .of_share(id)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let mut teams: Option<Vec<TeamName>> = None;
        for grant in &grants {
            match Group::of_grantee(grant.grantee()) {
                Some(Group::Everyone) if self.everyone_opens(grant, actor.who) => {
                    return Ok(true);
                }
                Some(Group::Team(name)) => {
                    if teams.is_none() {
                        teams = Some(self.active_teams(actor.person).await?);
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

    /// The groups through which `who` reads `id`: each team grant on it
    /// of an active team the caller's person belongs to, and its everyone
    /// grant when the grant's audience covers them. The teams are read only
    /// when the share has a team grant.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoPerson`] and [`ShareError::Unavailable`].
    pub(crate) async fn groups_reading(
        &self,
        who: &Principal,
        id: &ShareId,
    ) -> Result<Vec<Group>, ShareError> {
        let person = self.person(who)?;
        let grants = self
            .grants
            .of_share(id)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let mut teams: Option<Vec<TeamName>> = None;
        let mut groups = Vec::new();
        for grant in &grants {
            match Group::of_grantee(grant.grantee()) {
                Some(Group::Everyone) if self.everyone_opens(grant, who) => {
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
    /// dormant when it is a grant to a team that is not active. For an admin, a grant
    /// to a team the admin does not belong to is marked hidden, so no one
    /// outside a team learns its name.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NotFound`], [`ShareError::Forbidden`] for a
    /// reader, [`ShareError::NoPerson`] and
    /// [`ShareError::Unavailable`].
    pub async fn listed_grants(
        &self,
        who: &Principal,
        id: &ShareId,
    ) -> Result<Vec<ListedGrant>, ShareError> {
        let actor = self.actor(who)?;
        let (_, standing) = self.admit(&actor, id, ShareAction::ListGrants).await?;
        let mine = if standing == Standing::Owner {
            None
        } else {
            Some(self.active_teams(actor.person).await?)
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
