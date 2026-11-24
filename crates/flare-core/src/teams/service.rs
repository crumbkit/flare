//! [`FlareTeams`]: the default [`Teams`], which keeps teams in flare's own
//! store and applies who may change them.

use super::names::TeamName;
use super::store::{Team, TeamState, TeamStore};
use super::{MAX_TEAMS_JOINED, MAX_TEAMS_OWNED, Teams, TeamsError};
use crate::email::Email;
use crate::time::Timestamp;

/// Teams kept in a [`TeamStore`].
///
/// The owner alone adds members, removes them, hands the team to another
/// member and deletes the team. A member may leave, and the owner may not. A repeated create by the
/// team's owner answers the active team as it is. A member and the owner read
/// the member list; anyone else gets [`TeamsError::NotFound`], the same as
/// for a missing team.
///
/// Shortcut: the owned and joined limits are checked by reading before the
/// write, so creates, adds or transfers that run together for one person
/// can each pass the check, and each racing write can take the person one
/// team over the limit. The member limit is a condition of the store's
/// write and always holds.
#[derive(Debug)]
pub struct FlareTeams<S> {
    store: S,
}

impl<S: TeamStore> FlareTeams<S> {
    /// Teams in `store`.
    #[must_use]
    pub fn new(store: S) -> Self {
        Self { store }
    }

    /// The active team that `actor` may see: one they belong to.
    async fn visible(&self, actor: &Email, team: &TeamName) -> Result<Team, TeamsError> {
        let found = self.store.get(team).await?;
        let Some(found) = found.filter(|found| found.state() == TeamState::Active) else {
            return Err(TeamsError::NotFound);
        };
        let mine = self.store.teams_of(actor).await?;
        if found.owner() == actor || mine.contains(team) {
            Ok(found)
        } else {
            Err(TeamsError::NotFound)
        }
    }

    /// The visible team, when `actor` owns it.
    async fn owned(&self, actor: &Email, team: &TeamName) -> Result<Team, TeamsError> {
        let found = self.visible(actor, team).await?;
        if found.owner() == actor {
            Ok(found)
        } else {
            Err(TeamsError::Forbidden)
        }
    }

    /// How many active teams `person` owns.
    async fn owned_count(&self, person: &Email) -> Result<usize, TeamsError> {
        let mut owned = 0;
        for name in self.store.teams_of(person).await? {
            let found = self.store.get(&name).await?;
            if found
                .is_some_and(|found| found.owner() == person && found.state() == TeamState::Active)
            {
                owned += 1;
            }
        }
        Ok(owned)
    }
}

impl<S: TeamStore> Teams for FlareTeams<S> {
    async fn teams_of(&self, person: &Email) -> Result<Vec<TeamName>, TeamsError> {
        Ok(self.store.teams_of(person).await?)
    }

    async fn exists(&self, team: &TeamName) -> Result<bool, TeamsError> {
        Ok(self.active(team).await?.is_some())
    }

    async fn taken(&self, team: &TeamName) -> Result<bool, TeamsError> {
        Ok(self.store.get(team).await?.is_some())
    }

    async fn active(&self, team: &TeamName) -> Result<Option<Timestamp>, TeamsError> {
        let found = self.store.get(team).await?;
        Ok(found
            .filter(|found| found.state() == TeamState::Active)
            .map(|found| found.created()))
    }

    async fn members(&self, team: &TeamName) -> Result<Vec<Email>, TeamsError> {
        if !self.exists(team).await? {
            return Err(TeamsError::NotFound);
        }
        Ok(self.store.members(team).await?)
    }

    async fn create(
        &self,
        owner: &Email,
        team: TeamName,
        at: Timestamp,
    ) -> Result<Team, TeamsError> {
        match self.store.get(&team).await? {
            // A repeat by the owner of a create whose answer was lost finds
            // the team whole.
            Some(found) if found.owner() == owner && found.state() == TeamState::Active => {
                return Ok(found);
            }
            Some(_) => return Err(TeamsError::Taken),
            None => {}
        }
        let joined = self.store.teams_of(owner).await?;
        if joined.len() >= MAX_TEAMS_JOINED {
            return Err(TeamsError::TooManyJoined);
        }
        if self.owned_count(owner).await? >= MAX_TEAMS_OWNED {
            return Err(TeamsError::TooManyOwned);
        }
        let created = Team::new(team, owner.clone(), at, TeamState::Active, 1);
        self.store.create(&created).await?;
        Ok(created)
    }

    async fn add(
        &self,
        actor: &Email,
        team: &TeamName,
        member: &Email,
    ) -> Result<bool, TeamsError> {
        self.owned(actor, team).await?;
        let joined = self.store.teams_of(member).await?;
        if joined.contains(team) {
            return Ok(false);
        }
        if joined.len() >= MAX_TEAMS_JOINED {
            return Err(TeamsError::TooManyJoined);
        }
        Ok(self.store.add(team, member).await?)
    }

    async fn remove(
        &self,
        actor: &Email,
        team: &TeamName,
        member: &Email,
    ) -> Result<bool, TeamsError> {
        let found = self.visible(actor, team).await?;
        let owner = found.owner() == actor;
        if member == actor && owner {
            // The owner stays a member while the team exists, so that a
            // team always has someone who manages it.
            return Err(TeamsError::OwnerCannotLeave);
        }
        if member != actor && !owner {
            return Err(TeamsError::Forbidden);
        }
        Ok(self.store.remove(team, member).await?)
    }

    async fn delete(&self, actor: &Email, team: &TeamName) -> Result<(), TeamsError> {
        match self.store.get(team).await? {
            // A repeat by the owner continues a delete that stopped midway.
            Some(found) if found.state() == TeamState::Deleting && found.owner() == actor => Ok(()),
            Some(found) if found.state() == TeamState::Deleting => Err(TeamsError::NotFound),
            _ => {
                self.owned(actor, team).await?;
                Ok(self.store.mark_deleting(team).await?)
            }
        }
    }

    async fn finish_delete(&self, actor: &Email, team: &TeamName) -> Result<(), TeamsError> {
        match self.store.get(team).await? {
            None => Ok(()),
            Some(found) if found.state() == TeamState::Deleting && found.owner() == actor => {
                for member in self.store.members(team).await? {
                    self.store.remove(team, &member).await?;
                }
                self.store.forget(team).await?;
                Ok(())
            }
            Some(_) => Err(TeamsError::NotFound),
        }
    }

    async fn transfer(
        &self,
        actor: &Email,
        team: &TeamName,
        to: &Email,
    ) -> Result<Team, TeamsError> {
        let found = self.visible(actor, team).await?;
        if found.owner() == to {
            return Ok(found);
        }
        if found.owner() != actor {
            return Err(TeamsError::Forbidden);
        }
        if !self.store.teams_of(to).await?.contains(team) {
            return Err(TeamsError::NotAMember);
        }
        if self.owned_count(to).await? >= MAX_TEAMS_OWNED {
            return Err(TeamsError::TooManyOwned);
        }
        self.store.transfer(team, actor, to).await?;
        Ok(Team::new(
            found.name().clone(),
            to.clone(),
            found.created(),
            found.state(),
            found.members(),
        ))
    }

    async fn show(&self, actor: &Email, team: &TeamName) -> Result<(Team, Vec<Email>), TeamsError> {
        let found = self.visible(actor, team).await?;
        let members = self.store.members(team).await?;
        Ok((found, members))
    }

    async fn list(&self, actor: &Email) -> Result<Vec<Team>, TeamsError> {
        let mut teams = Vec::new();
        for name in self.store.teams_of(actor).await? {
            if let Some(found) = self.store.get(&name).await?
                && found.state() == TeamState::Active
            {
                teams.push(found);
            }
        }
        Ok(teams)
    }
}
