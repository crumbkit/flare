//! [`TeamView`], a team as [`Teams::list`](super::Teams::list) and
//! [`Teams::show`](super::Teams::show) give it to a member, and
//! [`ManagedBy`], which says who keeps the team.

use super::names::TeamName;
use super::store::Team;
use crate::person::Person;
use crate::time::Timestamp;

/// Who keeps a team and changes its members. Its `Debug` output hides the
/// owner, as [`Person`]'s does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManagedBy {
    /// flare keeps the team in its own store, and its owner changes it.
    Flare {
        /// The person who alone manages the team.
        owner: Person,
        /// When the team was created.
        created: Timestamp,
    },
    /// The deployment's directory keeps the team. No flare person owns it,
    /// and flare cannot change it.
    Directory,
}

/// An active team as a member sees it: its name, who keeps it and how
/// many members it has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamView {
    name: TeamName,
    managed_by: ManagedBy,
    members: usize,
}

impl TeamView {
    /// A team of the deployment's directory with `members` members.
    #[must_use]
    pub fn directory(name: TeamName, members: usize) -> Self {
        Self {
            name,
            managed_by: ManagedBy::Directory,
            members,
        }
    }

    /// The name.
    #[must_use]
    pub fn name(&self) -> &TeamName {
        &self.name
    }

    /// Who keeps the team.
    #[must_use]
    pub fn managed_by(&self) -> &ManagedBy {
        &self.managed_by
    }

    /// The person who manages the team, or `None` for a directory team.
    #[must_use]
    pub fn owner(&self) -> Option<&Person> {
        match &self.managed_by {
            ManagedBy::Flare { owner, .. } => Some(owner),
            ManagedBy::Directory => None,
        }
    }

    /// When flare created the team, or `None` for a directory team.
    #[must_use]
    pub fn created(&self) -> Option<Timestamp> {
        match &self.managed_by {
            ManagedBy::Flare { created, .. } => Some(*created),
            ManagedBy::Directory => None,
        }
    }

    /// How many members the team has, its owner included.
    #[must_use]
    pub fn members(&self) -> usize {
        self.members
    }

    /// The same team with `members` members.
    #[must_use]
    pub fn with_members(self, members: usize) -> Self {
        Self { members, ..self }
    }
}

impl From<Team> for TeamView {
    /// The view of a stored team. Only an active team is listed or shown,
    /// so the view keeps no state.
    fn from(team: Team) -> Self {
        let members = team.members();
        let created = team.created();
        let name = team.name().clone();
        Self {
            managed_by: ManagedBy::Flare {
                owner: team.owner().clone(),
                created,
            },
            name,
            members,
        }
    }
}
