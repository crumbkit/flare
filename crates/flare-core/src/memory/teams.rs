//! The in-memory [`TeamStore`].

use super::locked;
use crate::teams::{Team, TeamState, TeamStore, TeamStoreError};
use crate::{MAX_MEMBERS, Person, TeamName};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// One team and its members, by id.
struct Held {
    team: Team,
    members: BTreeMap<String, Person>,
}

/// A [`TeamStore`] that keeps its teams in memory. Every operation is one
/// step under one lock, so both entries of a member pair are one entry
/// here and always agree.
#[derive(Default)]
pub struct MemoryTeamStore {
    state: Mutex<BTreeMap<TeamName, Held>>,
}

impl std::fmt::Debug for MemoryTeamStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryTeamStore")
            .finish_non_exhaustive()
    }
}

impl MemoryTeamStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// `team` with `members` as its count.
fn counted(team: &Team, members: usize) -> Team {
    Team::new(
        team.name().clone(),
        team.owner().clone(),
        team.created(),
        team.state(),
        members,
    )
}

impl TeamStore for MemoryTeamStore {
    async fn create(&self, team: &Team) -> Result<(), TeamStoreError> {
        let mut state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        if state.contains_key(team.name()) {
            return Err(TeamStoreError::Taken);
        }
        let owner = team.owner().clone();
        let members = BTreeMap::from([(owner.as_str().to_owned(), owner)]);
        let team = counted(team, 1);
        state.insert(team.name().clone(), Held { team, members });
        Ok(())
    }

    async fn get(&self, name: &TeamName) -> Result<Option<Team>, TeamStoreError> {
        let state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        Ok(state.get(name).map(|held| held.team.clone()))
    }

    async fn add(&self, name: &TeamName, member: &Person) -> Result<bool, TeamStoreError> {
        let mut state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        let held = state
            .get_mut(name)
            .filter(|held| held.team.state() == TeamState::Active)
            .ok_or(TeamStoreError::NotFound)?;
        if held.members.contains_key(member.as_str()) {
            return Ok(false);
        }
        if held.team.members() >= MAX_MEMBERS {
            return Err(TeamStoreError::Full);
        }
        held.members
            .insert(member.as_str().to_owned(), member.clone());
        held.team = counted(&held.team, held.team.members().saturating_add(1));
        Ok(true)
    }

    async fn remove(&self, name: &TeamName, member: &Person) -> Result<bool, TeamStoreError> {
        let mut state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        let Some(held) = state.get_mut(name) else {
            return Ok(false);
        };
        if held.members.remove(member.as_str()).is_none() {
            return Ok(false);
        }
        held.team = counted(&held.team, held.team.members().saturating_sub(1));
        Ok(true)
    }

    async fn mark_deleting(&self, name: &TeamName) -> Result<(), TeamStoreError> {
        let mut state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        let held = state.get_mut(name).ok_or(TeamStoreError::NotFound)?;
        held.team = Team::new(
            held.team.name().clone(),
            held.team.owner().clone(),
            held.team.created(),
            TeamState::Deleting,
            held.team.members(),
        );
        Ok(())
    }

    async fn forget(&self, name: &TeamName) -> Result<bool, TeamStoreError> {
        let mut state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        match state.get(name) {
            None => Ok(false),
            Some(held) if held.team.state() == TeamState::Deleting && held.members.is_empty() => {
                state.remove(name);
                Ok(true)
            }
            Some(_) => Err(TeamStoreError::NotFound),
        }
    }

    async fn transfer(
        &self,
        name: &TeamName,
        from: &Person,
        to: &Person,
    ) -> Result<(), TeamStoreError> {
        let mut state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        let held = state
            .get_mut(name)
            .filter(|held| {
                held.team.state() == TeamState::Active
                    && held.team.owner() == from
                    && held.members.contains_key(to.as_str())
            })
            .ok_or(TeamStoreError::NotFound)?;
        held.team = Team::new(
            held.team.name().clone(),
            to.clone(),
            held.team.created(),
            TeamState::Active,
            held.team.members(),
        );
        Ok(())
    }

    async fn members(&self, name: &TeamName) -> Result<Vec<Person>, TeamStoreError> {
        let state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        Ok(state
            .get(name)
            .map(|held| held.members.values().cloned().collect())
            .unwrap_or_default())
    }

    async fn teams_of(&self, member: &Person) -> Result<Vec<TeamName>, TeamStoreError> {
        let state = locked(&self.state).ok_or(TeamStoreError::Unavailable)?;
        Ok(state
            .iter()
            .filter(|(_, held)| held.members.contains_key(member.as_str()))
            .map(|(name, _)| name.clone())
            .collect())
    }
}
