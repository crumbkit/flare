//! The [`Shares`] methods that read group grants: whether a person opens a
//! share through a team or an everyone grant, the shares granted to the
//! caller, and the grants of a share with their state.

use super::{ListedGrant, SharedWithMe, Shares, actor};
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::{Grant, Role};
use crate::name::PrincipalId;
use crate::share::{Listing, MAX_LISTED, ShareId};
use crate::sharing::error::ShareError;
use crate::sharing::{ShareAction, Standing};
use crate::store::{GrantStore, ShareStore, ShareStoreError};
use crate::teams::{Group, TeamName, Teams};
use std::cmp::Reverse;
use std::collections::HashMap;

/// The rank of a role, so the higher of two grants of one share wins.
fn rank(role: Role) -> u8 {
    match role {
        Role::Admin => 2,
        Role::Reader => 1,
    }
}

/// The address an actor's id holds. Every actor id is the lower-case
/// address it came from, so it parses back.
fn address(actor: &PrincipalId) -> Option<Email> {
    Email::try_from(actor.as_str()).ok()
}

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
    /// The active teams that `person` belongs to.
    async fn active_teams(&self, person: &Email) -> Result<Vec<TeamName>, ShareError> {
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
    fn everyone_opens(&self, grant: &Grant, person: &Email) -> bool {
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

    /// The shares granted to the caller, directly or through an active
    /// team, newest grant first, each with the caller's highest role. With
    /// `everyone`, the shares open to everyone whose audience covers the
    /// caller are listed too. A grant whose share no longer exists, and a
    /// share the caller owns, are left out.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`] when a store fails.
    pub async fn shared_with_me(
        &self,
        who: &Principal,
        everyone: bool,
    ) -> Result<Listing<SharedWithMe>, ShareError> {
        let actor = actor(who)?;
        let person = address(&actor).ok_or(ShareError::NoVerifiedEmail)?;
        let unavailable = |_| ShareError::Unavailable;
        let direct = self.grants.granted_to(&actor).await.map_err(unavailable)?;
        let mut truncated = direct.truncated();
        let mut grants = direct.into_items();
        for team in self.active_teams(&person).await? {
            let pid = Group::Team(team).principal_id();
            let listed = self.grants.granted_to(&pid).await.map_err(unavailable)?;
            truncated |= listed.truncated();
            grants.extend(listed.into_items());
        }
        if everyone {
            let pid = Group::Everyone.principal_id();
            let listed = self.grants.granted_to(&pid).await.map_err(unavailable)?;
            truncated |= listed.truncated();
            grants.extend(
                listed
                    .into_items()
                    .into_iter()
                    .filter(|grant| self.everyone_opens(grant, &person)),
            );
        }
        let mut best: HashMap<ShareId, Grant> = HashMap::new();
        for grant in grants {
            let keep = best
                .get(grant.share())
                .is_none_or(|held| rank(grant.role()) > rank(held.role()));
            if keep {
                best.insert(grant.share().clone(), grant);
            }
        }
        let mut grants: Vec<Grant> = best.into_values().collect();
        grants.sort_by_key(|grant| (Reverse(grant.created()), grant.share().clone()));
        if grants.len() > MAX_LISTED {
            truncated = true;
            grants.truncate(MAX_LISTED);
        }
        let mut entries = Vec::with_capacity(grants.len());
        for grant in grants {
            match self.shares.get(grant.share()).await {
                Ok(share) if share.owner() == &actor => {}
                Ok(share) => entries.push(SharedWithMe {
                    share,
                    role: grant.role(),
                }),
                Err(ShareStoreError::NotFound) => {}
                Err(_) => return Err(ShareError::Unavailable),
            }
        }
        Ok(Listing::with_truncated(entries, truncated))
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
