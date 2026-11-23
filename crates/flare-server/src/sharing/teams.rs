//! The share API's team calls: create, list, show and delete a team, and
//! add or remove its members.

use super::convert::{recipient_error, team_summary, teams_error};
use super::run::json;
use super::{Answer, Refusal, Sharing};
use flare_contract::{
    ApiCode, MembersChanged, MembersRequest, TeamDeleted, TeamList, TeamRequest, TeamShown,
};
use flare_core::{
    Email, GrantStore, Principal, Recipients, ResponseStore, ShareStore, Team, TeamName, Teams,
    Timestamp, TokenStore,
};

/// The caller's verified address.
fn caller(who: &Principal) -> Result<&Email, Refusal> {
    who.email().ok_or_else(|| ApiCode::NoVerifiedEmail.into())
}

/// The team that `text` names; a malformed name is a missing team.
fn team_name(text: &str) -> Result<TeamName, Refusal> {
    TeamName::try_from(text).map_err(|_| ApiCode::TeamNotFound.into())
}

impl<S, G, R, T, X, M> Sharing<S, G, R, T, X, M>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
{
    pub(super) async fn create_team(
        &self,
        who: &Principal,
        request: &TeamRequest,
        now: Timestamp,
    ) -> Answer {
        let owner = caller(who)?;
        let name = TeamName::try_from(request.name.as_str()).map_err(|error| Refusal {
            code: ApiCode::TeamNameInvalid,
            detail: Some(error.to_string()),
            share: None,
        })?;
        let team = self
            .flares
            .create_team(who, name, now)
            .await
            .map_err(teams_error)?;
        json(201, &team_summary(&team, owner))
    }

    pub(super) async fn list_teams(&self, who: &Principal) -> Answer {
        let person = caller(who)?;
        let teams = self
            .flares
            .shares()
            .teams()
            .list(person)
            .await
            .map_err(teams_error)?;
        let items = teams
            .iter()
            .map(|team| team_summary(team, person))
            .collect();
        json(200, &TeamList { items })
    }

    pub(super) async fn show_team(&self, who: &Principal, name: &str) -> Answer {
        let person = caller(who)?;
        let name = team_name(name)?;
        let (team, members) = self
            .flares
            .shares()
            .teams()
            .show(person, &name)
            .await
            .map_err(teams_error)?;
        json(
            200,
            &TeamShown {
                team: team_summary(&team, person),
                members: members.iter().map(|m| m.as_str().to_owned()).collect(),
            },
        )
    }

    pub(super) async fn delete_team(&self, who: &Principal, name: &str) -> Answer {
        let person = caller(who)?;
        let name = team_name(name)?;
        self.flares
            .shares()
            .teams()
            .delete(person, &name)
            .await
            .map_err(teams_error)?;
        json(
            200,
            &TeamDeleted {
                team: name.as_str().to_owned(),
                grants_dormant: true,
                name_reusable: false,
            },
        )
    }

    /// Adds the people the request names, when `add`, or removes them.
    pub(super) async fn change_members(
        &self,
        who: &Principal,
        name: &str,
        request: &MembersRequest,
        add: bool,
    ) -> Answer {
        let person = caller(who)?;
        let name = team_name(name)?;
        let teams = self.flares.shares().teams();
        // The team is read first, so a caller who cannot see it learns
        // nothing about the people named, and a member who leaves still
        // gets the team's summary.
        let (before, _) = teams.show(person, &name).await.map_err(teams_error)?;
        let mut changed = Vec::new();
        for identifier in &request.with {
            let member = self
                .recipients
                .resolve(identifier)
                .await
                .map_err(|error| recipient_error(error, identifier))?;
            let done = if add {
                teams.add(person, &name, &member).await
            } else {
                teams.remove(person, &name, &member).await
            };
            if done.map_err(teams_error)? {
                changed.push(member.as_str().to_owned());
            }
        }
        let count = teams.members(&name).await.map_err(teams_error)?.len();
        let team = Team::new(
            before.name().clone(),
            before.owner().clone(),
            before.created(),
            before.state(),
            count,
        );
        json(
            200,
            &MembersChanged {
                team: team_summary(&team, person),
                changed,
            },
        )
    }
}
