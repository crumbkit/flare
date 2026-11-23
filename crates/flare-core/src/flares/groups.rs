//! [`Flares::grant_all`], which grants an existing share to more people,
//! teams and everyone, [`Flares::revoke`], which ends grants, and
//! [`Flares::create_team`], which applies the creators rule to a new team.

use super::{FlareError, Flared, Flares, once_each};
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::Role;
use crate::share::ShareId;
use crate::sharing::{ShareError, Standing};
use crate::store::{GrantStore, ResponseStore, ShareStore};
use crate::teams::{Group, Team, TeamName, Teams, TeamsError};
use crate::time::Timestamp;

/// The grants that a [`Flares::revoke`] ended.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Revoked {
    /// The share.
    pub share: ShareId,
    /// The people whose grant ended.
    pub people: Vec<Email>,
    /// The teams, or everyone, whose grant ended.
    pub groups: Vec<Group>,
}

/// The answer to a write that failed after `written` earlier writes.
fn failed(id: &ShareId, written: usize, cause: ShareError) -> FlareError {
    if written == 0 {
        FlareError::Share(cause)
    } else {
        FlareError::PartlyShared {
            share: id.clone(),
            granted: written,
            cause,
        }
    }
}

impl<S: ShareStore, G: GrantStore, R: ResponseStore, T: Teams> Flares<S, G, R, T> {
    /// Creates a team owned by the caller, who must be a creator.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotACreator`] and the refusals of
    /// [`Teams::create`].
    pub async fn create_team(
        &self,
        who: &Principal,
        name: TeamName,
        at: Timestamp,
    ) -> Result<Team, TeamsError> {
        let Some(owner) = who.email() else {
            return Err(TeamsError::NotACreator);
        };
        if !self.creators.admits(who) {
            return Err(TeamsError::NotACreator);
        }
        self.shares.teams().create(owner, name, at).await
    }

    /// Grants Reader on an existing share to each person in `with` who
    /// holds no grant yet, and, for the share's owner, records each flare
    /// in `re` as one it responds to. A person who already holds a role
    /// keeps it, so an admin is never demoted.
    ///
    /// # Errors
    ///
    /// The same as [`Flares::grant_all`].
    pub async fn grant(
        &self,
        who: &Principal,
        id: &ShareId,
        with: &[Email],
        re: &[ShareId],
        at: Timestamp,
    ) -> Result<Flared, FlareError> {
        self.grant_all(who, id, with, &[], re, at).await
    }

    /// Grants Reader on an existing share to each person in `with` and each
    /// group in `groups` that holds no grant yet, and, for the share's
    /// owner, records each flare in `re` as one it responds to.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::Share`] with the share service's refusal,
    /// [`ShareError::Forbidden`] when someone other than the owner names
    /// `re` or everyone, [`ShareError::TeamNotFound`],
    /// [`ShareError::EveryoneUnbounded`], [`FlareError::NotACreator`],
    /// [`FlareError::NotFound`] for a `re` flare the caller cannot open, and
    /// [`FlareError::PartlyShared`] when a write fails after an earlier one.
    pub async fn grant_all(
        &self,
        who: &Principal,
        id: &ShareId,
        with: &[Email],
        groups: &[Group],
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
        let mut granted_groups = Vec::new();
        if !with.is_empty() || !groups.is_empty() {
            let current = self
                .shares
                .grants(who, id)
                .await
                .map_err(FlareError::Share)?;
            let held = |grantee: crate::PrincipalId| {
                current.iter().any(|grant| grant.grantee() == &grantee)
            };
            for person in once_each(with.to_vec()) {
                if held(person.principal_id()) || opened.share.owner() == &person.principal_id() {
                    continue;
                }
                match self.shares.grant(who, id, &person, Role::Reader, at).await {
                    Ok(_) => granted.push(person),
                    Err(cause) => return Err(failed(id, granted.len(), cause)),
                }
            }
            for group in once_each(groups.to_vec()) {
                if held(group.principal_id()) {
                    continue;
                }
                let written = granted.len() + granted_groups.len();
                match self.shares.grant_group(who, id, &group, at).await {
                    Ok(_) => granted_groups.push(group),
                    Err(cause) => return Err(failed(id, written, cause)),
                }
            }
        }
        let written = granted.len() + granted_groups.len();
        if let Err(cause) = self.grant_owners(who, id, &flares, at).await {
            return Err(failed(id, written, cause));
        }
        if self.link(id, &flares, at).await.is_err() {
            return Err(FlareError::PartlyShared {
                share: id.clone(),
                granted: written,
                cause: ShareError::Unavailable,
            });
        }
        Ok(Flared {
            share: opened.share,
            granted,
            groups: granted_groups,
            responds_to: re,
        })
    }

    /// Ends the grant of each person in `with` and each group in `groups`.
    /// One who holds no grant is skipped, so a repeat finishes a revoke
    /// that failed midway. The caller's own address ends their own grant.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::Share`] with the share service's refusal of
    /// the first revoke that failed: [`ShareError::NotFound`],
    /// [`ShareError::Forbidden`], [`ShareError::OwnerCannotBeChanged`],
    /// [`ShareError::TeamNotFound`], [`ShareError::GrantChanged`],
    /// [`ShareError::NoVerifiedEmail`] or [`ShareError::Unavailable`].
    pub async fn revoke(
        &self,
        who: &Principal,
        id: &ShareId,
        with: &[Email],
        groups: &[Group],
    ) -> Result<Revoked, FlareError> {
        let mut revoked = Revoked {
            share: id.clone(),
            people: Vec::new(),
            groups: Vec::new(),
        };
        for person in once_each(with.to_vec()) {
            match self.shares.revoke(who, id, &person).await {
                Ok(()) => revoked.people.push(person),
                Err(ShareError::GrantNotFound) => {}
                Err(cause) => return Err(FlareError::Share(cause)),
            }
        }
        for group in once_each(groups.to_vec()) {
            match self.shares.revoke_group(who, id, &group).await {
                Ok(()) => revoked.groups.push(group),
                Err(ShareError::GrantNotFound) => {}
                Err(cause) => return Err(FlareError::Share(cause)),
            }
        }
        Ok(revoked)
    }
}
