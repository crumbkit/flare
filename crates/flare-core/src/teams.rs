//! Teams: named groups of people that a share can be granted to, and the
//! [`Teams`] seam that the share service reads them through.
//!
//! [`FlareTeams`] is the default implementation, over a [`TeamStore`]. A
//! deployment may supply its own, such as one that reads its directory.
//! That one may be read-only: every change then answers
//! [`TeamsError::ManagedElsewhere`], which the default methods of
//! [`Teams`] do. [`NoTeams`] knows no team at all.
//!
//! # Lifecycle of a team
//!
//! | State | Event | Next state | Who |
//! | --- | --- | --- | --- |
//! | absent | create | active, owner as first member | a creator |
//! | active or deleting | create with that name | unchanged, `Taken` | anyone but an active team's owner |
//! | active | add a person | active, one more member pair | the owner |
//! | active | remove a person | active, one less member pair | the owner, or the member who leaves |
//! | active | transfer to a member | active, with that member as owner | the owner |
//! | active | delete | deleting | the owner |
//! | deleting | delete again | deleting, more grants removed, or absent when none are left | the owner |
//!
//! A deleting team opens nothing. [`Flares::delete_team`](crate::Flares::delete_team)
//! marks the team deleting, removes its grant from each share through the
//! team's list of grants (its team-to-shares index), and then
//! [`Teams::finish_delete`] removes the member pairs and the team, which
//! frees the name. A delete that stops midway leaves the team deleting,
//! and the owner's repeat continues it. A new team of a freed name holds
//! none of the old team's grants.

mod names;
mod service;
mod store;

pub use names::{Admission, Audience, Group, MailDomain, TeamName};
pub use service::FlareTeams;
pub use store::{Team, TeamState, TeamStore, TeamStoreError};

use crate::person::Person;
use crate::time::Timestamp;
use std::future::Future;

/// The most members one team has, its owner included. An add to a full
/// team answers [`TeamsError::TooManyMembers`] and writes nothing.
pub const MAX_MEMBERS: usize = 256;

/// The most active teams one person owns. A create or a transfer beyond it
/// answers [`TeamsError::TooManyOwned`] and writes nothing.
pub const MAX_TEAMS_OWNED: usize = 20;

/// The most teams one person belongs to. A create or an add beyond it
/// answers [`TeamsError::TooManyJoined`] and writes nothing.
pub const MAX_TEAMS_JOINED: usize = 32;

/// Why the [`Teams`] service refused a request. No variant holds an
/// address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TeamsError {
    /// The team does not exist, is deleted, or the caller is not a member.
    /// The three give the same answer.
    #[error("no such team")]
    NotFound,
    /// The caller is a member and only the owner may make the change.
    #[error("only the team's owner may make this change")]
    Forbidden,
    /// The team's owner tried to leave it. The owner stays a member while
    /// the team exists and deletes the team instead.
    #[error("the team's owner cannot leave the team")]
    OwnerCannotLeave,
    /// The caller is not one of the deployment's creators.
    #[error("only the deployment's creators may create a team")]
    NotACreator,
    /// A team has the name, active or being deleted.
    #[error("a team with this name exists")]
    Taken,
    /// The person a transfer names is not a member of the team.
    #[error("the new owner must be a member of the team")]
    NotAMember,
    /// The team has [`MAX_MEMBERS`] members.
    #[error("the team has as many members as one team may have")]
    TooManyMembers,
    /// The caller owns [`MAX_TEAMS_OWNED`] active teams.
    #[error("the caller owns as many teams as one person may")]
    TooManyOwned,
    /// The person belongs to [`MAX_TEAMS_JOINED`] teams.
    #[error("the person belongs to as many teams as one person may")]
    TooManyJoined,
    /// The deployment manages its teams elsewhere, such as in its
    /// directory, and flare cannot change them.
    #[error("this deployment manages its teams elsewhere")]
    ManagedElsewhere,
    /// The team store could not answer. After a write the outcome is
    /// unknown.
    #[error("the team store is unavailable")]
    Unavailable,
}

impl From<TeamStoreError> for TeamsError {
    fn from(error: TeamStoreError) -> Self {
        match error {
            TeamStoreError::Taken => Self::Taken,
            TeamStoreError::NotFound => Self::NotFound,
            TeamStoreError::Full => Self::TooManyMembers,
            TeamStoreError::Unavailable => Self::Unavailable,
        }
    }
}

/// The teams that the share service reads, and the changes that the team
/// routes make. Each `actor` is the caller's person.
///
/// The three reads are what a share's standing needs. The changes have
/// default bodies that answer [`TeamsError::ManagedElsewhere`], so a
/// read-only implementation writes only the reads. An implementation that
/// writes the changes also answers `true` from [`Teams::manages`].
pub trait Teams: Send + Sync {
    /// Whether flare creates, changes and deletes these teams. The default
    /// answers `false`, as the default changes answer
    /// [`TeamsError::ManagedElsewhere`]. [`Flares::create_team`](crate::Flares::create_team)
    /// answers `ManagedElsewhere` without touching any grant when this is
    /// `false`.
    fn manages(&self) -> bool {
        false
    }

    /// The names of the teams that `person` belongs to. A name may be of a
    /// deleted team; [`Teams::exists`] tells.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::Unavailable`].
    fn teams_of(
        &self,
        person: &Person,
    ) -> impl Future<Output = Result<Vec<TeamName>, TeamsError>> + Send;

    /// Whether an active team has the name.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::Unavailable`].
    fn exists(&self, team: &TeamName) -> impl Future<Output = Result<bool, TeamsError>> + Send;

    /// Whether a team has the name, active or being deleted. The default
    /// answers [`Teams::exists`].
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::Unavailable`].
    fn taken(&self, team: &TeamName) -> impl Future<Output = Result<bool, TeamsError>> + Send {
        self.exists(team)
    }

    /// When the active team with the name was created, or `None` when no
    /// active team has it. A grant reads it before and after its write, so
    /// a team deleted, or deleted and created again, in between is seen.
    /// The default answers from [`Teams::exists`] with one fixed time,
    /// which suits teams that flare never deletes.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::Unavailable`].
    fn active(
        &self,
        team: &TeamName,
    ) -> impl Future<Output = Result<Option<Timestamp>, TeamsError>> + Send {
        async move { Ok(self.exists(team).await?.then(|| Timestamp::from_secs(0))) }
    }

    /// The members of an active team.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`] and [`TeamsError::Unavailable`].
    fn members(
        &self,
        team: &TeamName,
    ) -> impl Future<Output = Result<Vec<Person>, TeamsError>> + Send;

    /// Creates a team owned by `owner`, who becomes its first member. The
    /// caller has checked that `owner` is a creator. A repeat by the owner
    /// of an active team answers that team as it is.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::Taken`], [`TeamsError::TooManyOwned`],
    /// [`TeamsError::TooManyJoined`], [`TeamsError::ManagedElsewhere`] and
    /// [`TeamsError::Unavailable`].
    fn create(
        &self,
        _owner: &Person,
        _team: TeamName,
        _at: Timestamp,
    ) -> impl Future<Output = Result<Team, TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// Adds `member` to the team, for its owner. Answers `false` when the
    /// person is already a member.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`], [`TeamsError::Forbidden`],
    /// [`TeamsError::TooManyMembers`], [`TeamsError::TooManyJoined`],
    /// [`TeamsError::ManagedElsewhere`] and [`TeamsError::Unavailable`].
    fn add(
        &self,
        _actor: &Person,
        _team: &TeamName,
        _member: &Person,
    ) -> impl Future<Output = Result<bool, TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// Removes `member` from the team, for its owner, or for the member
    /// who leaves. The owner cannot leave. Answers `false` when the person
    /// is not a member.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`], [`TeamsError::Forbidden`],
    /// [`TeamsError::OwnerCannotLeave`], [`TeamsError::ManagedElsewhere`]
    /// and [`TeamsError::Unavailable`].
    fn remove(
        &self,
        _actor: &Person,
        _team: &TeamName,
        _member: &Person,
    ) -> impl Future<Output = Result<bool, TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// Starts deleting the team, for its owner: the team is marked
    /// deleting and opens nothing from then on. A repeat by the owner of a
    /// deleting team answers `Ok`. The caller then removes the team's
    /// grants and calls [`Teams::finish_delete`].
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`], [`TeamsError::Forbidden`],
    /// [`TeamsError::ManagedElsewhere`] and [`TeamsError::Unavailable`].
    fn delete(
        &self,
        _actor: &Person,
        _team: &TeamName,
    ) -> impl Future<Output = Result<(), TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// Finishes deleting a deleting team, for its owner: removes its member
    /// pairs and then the team, which frees the name. A repeat after the
    /// team is gone answers `Ok`.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`] for an active team or one the
    /// caller does not own, [`TeamsError::ManagedElsewhere`] and
    /// [`TeamsError::Unavailable`].
    fn finish_delete(
        &self,
        _actor: &Person,
        _team: &TeamName,
    ) -> impl Future<Output = Result<(), TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// Hands the team to `to`, a member, for its owner. The former owner
    /// stays a member. A repeat that finds `to` already the owner answers
    /// the team as it is.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`], [`TeamsError::Forbidden`],
    /// [`TeamsError::NotAMember`], [`TeamsError::TooManyOwned`],
    /// [`TeamsError::ManagedElsewhere`] and [`TeamsError::Unavailable`].
    fn transfer(
        &self,
        _actor: &Person,
        _team: &TeamName,
        _to: &Person,
    ) -> impl Future<Output = Result<Team, TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// The team and its members, for a member or the owner.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`], [`TeamsError::ManagedElsewhere`]
    /// and [`TeamsError::Unavailable`].
    fn show(
        &self,
        _actor: &Person,
        _team: &TeamName,
    ) -> impl Future<Output = Result<(Team, Vec<Person>), TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }

    /// The active teams that `actor` belongs to, in name order.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::ManagedElsewhere`] and
    /// [`TeamsError::Unavailable`].
    fn list(&self, _actor: &Person) -> impl Future<Output = Result<Vec<Team>, TeamsError>> + Send {
        async { Err(TeamsError::ManagedElsewhere) }
    }
}

/// A deployment with no teams: nobody belongs to a team, no team exists,
/// and every change answers [`TeamsError::ManagedElsewhere`].
#[derive(Debug, Clone, Copy, Default)]
pub struct NoTeams;

impl Teams for NoTeams {
    async fn teams_of(&self, _person: &Person) -> Result<Vec<TeamName>, TeamsError> {
        Ok(Vec::new())
    }

    async fn exists(&self, _team: &TeamName) -> Result<bool, TeamsError> {
        Ok(false)
    }

    async fn members(&self, _team: &TeamName) -> Result<Vec<Person>, TeamsError> {
        Err(TeamsError::NotFound)
    }
}

#[cfg(test)]
mod tests;
