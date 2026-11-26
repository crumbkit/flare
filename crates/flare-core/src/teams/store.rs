//! [`Team`] and the [`TeamStore`] trait that keeps teams and their members.

use super::names::TeamName;
use crate::person::Person;
use crate::time::Timestamp;
use std::fmt;
use std::future::Future;

/// Whether a team is in use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TeamState {
    /// The team has members, and its grants open shares to them.
    Active,
    /// The owner started deleting the team. It opens nothing, its name
    /// stays taken, and its grants and member pairs are being removed.
    Deleting,
}

/// A team as a store holds it.
#[derive(Clone, PartialEq, Eq)]
pub struct Team {
    name: TeamName,
    owner: Person,
    created: Timestamp,
    state: TeamState,
    members: usize,
}

impl Team {
    /// Builds a team as a store read it back.
    #[must_use]
    pub fn new(
        name: TeamName,
        owner: Person,
        created: Timestamp,
        state: TeamState,
        members: usize,
    ) -> Self {
        Self {
            name,
            owner,
            created,
            state,
            members,
        }
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> &TeamName {
        &self.name
    }

    /// The person who created the team and alone manages it.
    #[must_use]
    pub fn owner(&self) -> &Person {
        &self.owner
    }

    /// When the team was created.
    #[must_use]
    pub fn created(&self) -> Timestamp {
        self.created
    }

    /// Whether the team is active or deleted.
    #[must_use]
    pub fn state(&self) -> TeamState {
        self.state
    }

    /// How many members the team has, its owner included.
    #[must_use]
    pub fn members(&self) -> usize {
        self.members
    }
}

impl fmt::Debug for Team {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Team")
            .field("name", &self.name)
            .field("owner", &self.owner)
            .field("created", &self.created)
            .field("state", &self.state)
            .field("members", &self.members)
            .finish()
    }
}

/// Why a team store refused a call.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TeamStoreError {
    /// A team with the name exists. Nothing was written.
    #[error("the team name is taken")]
    Taken,
    /// No active team has the name. Nothing was written.
    #[error("no such team")]
    NotFound,
    /// The team has [`MAX_MEMBERS`](crate::MAX_MEMBERS) members. Nothing
    /// was written.
    #[error("the team has as many members as one team may have")]
    Full,
    /// The store could not answer. After a write the outcome is unknown.
    #[error("the team store is unavailable")]
    Unavailable,
}

/// Keeps teams and their members.
///
/// A member is held as a pair: one entry under the team, which lists its
/// members, and one under the person, which lists their teams. A store
/// writes and deletes both entries of a pair together, so the two lists
/// always agree. Every read returns every write that completed before it
/// started. The store applies no rule about who may change a team; the
/// [`Teams`](crate::Teams) service decides that.
pub trait TeamStore: Send + Sync {
    /// Writes an active team with its owner as its one member, when no team
    /// has the name.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::Taken`] and
    /// [`TeamStoreError::Unavailable`].
    fn create(&self, team: &Team) -> impl Future<Output = Result<(), TeamStoreError>> + Send;

    /// The team with the name, active or deleting, or `None`.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::Unavailable`].
    fn get(
        &self,
        name: &TeamName,
    ) -> impl Future<Output = Result<Option<Team>, TeamStoreError>> + Send;

    /// Adds `member` to an active team. Answers `false` and writes nothing
    /// when the person is already a member.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::NotFound`] for a missing or deleting team,
    /// [`TeamStoreError::Full`] and [`TeamStoreError::Unavailable`].
    fn add(
        &self,
        name: &TeamName,
        member: &Person,
    ) -> impl Future<Output = Result<bool, TeamStoreError>> + Send;

    /// Removes `member` from the team, active or deleting. Answers `false`
    /// and writes nothing when the person is not a member.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::Unavailable`].
    fn remove(
        &self,
        name: &TeamName,
        member: &Person,
    ) -> impl Future<Output = Result<bool, TeamStoreError>> + Send;

    /// Marks the team deleting. Its member pairs stay until the caller
    /// removes them. Marking a deleting team again changes nothing.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::NotFound`] when no team has the name, and
    /// [`TeamStoreError::Unavailable`].
    fn mark_deleting(
        &self,
        name: &TeamName,
    ) -> impl Future<Output = Result<(), TeamStoreError>> + Send;

    /// Removes a deleting team that has no member left, which frees its
    /// name. Answers `false` and writes nothing when no team has the name.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::NotFound`] for an active team or one that
    /// still has members, and [`TeamStoreError::Unavailable`].
    fn forget(&self, name: &TeamName) -> impl Future<Output = Result<bool, TeamStoreError>> + Send;

    /// Makes `to` the owner of the active team that `from` owns, while
    /// `to` is a member.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::NotFound`] when no active team of `from`
    /// has the name or `to` is not its member, and
    /// [`TeamStoreError::Unavailable`].
    fn transfer(
        &self,
        name: &TeamName,
        from: &Person,
        to: &Person,
    ) -> impl Future<Output = Result<(), TeamStoreError>> + Send;

    /// The members of the team, in id order. A team has at most
    /// [`MAX_MEMBERS`](crate::MAX_MEMBERS).
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::Unavailable`].
    fn members(
        &self,
        name: &TeamName,
    ) -> impl Future<Output = Result<Vec<Person>, TeamStoreError>> + Send;

    /// The names of the teams that `member` belongs to, in name order,
    /// including a deleting team whose pairs are not yet removed.
    ///
    /// # Errors
    ///
    /// Returns [`TeamStoreError::Unavailable`].
    fn teams_of(
        &self,
        member: &Person,
    ) -> impl Future<Output = Result<Vec<TeamName>, TeamStoreError>> + Send;
}
