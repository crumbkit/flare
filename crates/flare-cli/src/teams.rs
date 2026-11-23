//! `flare teams`: create, list, show and delete teams, and add or remove
//! their members, through the server's team routes.

use crate::cli::ListTeamsArgs;
use crate::codes::{Failed, Refused};
use crate::documents::teams::{MemberRow, MembersOut, TeamDeletedRow, TeamRow};
use crate::env::Snapshot;
use crate::sharing::{body, session};
use crumbkit::{Call, Listing, Step};
use flare_contract::{
    ApiCode, MembersChanged, MembersRequest, TeamDeleted, TeamList, TeamRequest, TeamRole,
    TeamShown, TeamSummary,
};
use flare_core::TeamName;
use reqwest::Method;

/// The team that `text` names; a malformed name is a missing team, as the
/// server would answer.
fn team_path(origin: &str, text: &str) -> Result<String, Failed> {
    TeamName::try_from(text)
        .map(|name| format!("/api/teams/{}", name.as_str()))
        .map_err(|_| Refused::new(ApiCode::TeamNotFound, None, origin).into())
}

fn row(team: TeamSummary) -> TeamRow {
    TeamRow {
        name: team.name,
        owner: team.owner,
        role: match team.role {
            TeamRole::Owner => "owner",
            TeamRole::Member => "member",
        }
        .to_owned(),
        members: team.members,
        created: team.created,
    }
}

/// Runs `flare teams create`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn create(name: &str, env: &Snapshot) -> Result<TeamRow, Failed> {
    let api = session(env)?;
    let request = TeamRequest {
        name: name.to_owned(),
    };
    let answer: Option<TeamSummary> = api.send(Method::POST, "/api/teams", Some(&request)).await?;
    Ok(row(body(&api, answer)?))
}

/// Runs `flare teams add`, when `add`, or `flare teams remove`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn members(
    name: &str,
    with: &[String],
    add: bool,
    env: &Snapshot,
) -> Result<MembersOut, Failed> {
    let api = session(env)?;
    let target = format!("{}/members", team_path(api.origin(), name)?);
    let request = MembersRequest {
        with: with.to_vec(),
    };
    let method = if add { Method::POST } else { Method::DELETE };
    let answer: Option<MembersChanged> = api.send(method, &target, Some(&request)).await?;
    let changed = body(&api, answer)?;
    Ok(MembersOut {
        team: changed.team.name,
        members: changed.team.members,
        changed: changed.changed,
    })
}

/// Runs `flare teams list`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn list(args: &ListTeamsArgs, env: &Snapshot) -> Result<Listing<TeamRow>, Failed> {
    let api = session(env)?;
    let answer: Option<TeamList> = api.send(Method::GET, "/api/teams", None::<&()>).await?;
    let rows = body(&api, answer)?.items.into_iter().map(row).collect();
    Ok(Listing::from_all(rows, args.page.limit()).described("teams"))
}

/// Runs `flare teams show`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn show(names: &[String], env: &Snapshot) -> Result<Listing<MemberRow>, Failed> {
    let api = session(env)?;
    let mut rows = Vec::new();
    for name in names {
        let target = team_path(api.origin(), name)?;
        let answer: Option<TeamShown> = api.send(Method::GET, &target, None::<&()>).await?;
        let shown = body(&api, answer)?;
        let team = shown.team;
        rows.extend(shown.members.into_iter().map(|member| {
            MemberRow {
                team: team.name.clone(),
                role: if member == team.owner {
                    "owner"
                } else {
                    "member"
                }
                .to_owned(),
                member,
            }
        }));
    }
    Ok(Listing::from_all(rows, None).described("members"))
}

/// Runs `flare teams delete`. The text output says that the team's grants
/// stay dormant and that its name is never used again.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn delete(name: &str, env: &Snapshot) -> Result<Listing<TeamDeletedRow>, Failed> {
    let api = session(env)?;
    let target = team_path(api.origin(), name)?;
    let answer: Option<TeamDeleted> = api.send(Method::DELETE, &target, None::<&()>).await?;
    let deleted = body(&api, answer)?;
    let note = format!(
        "The team {team} is deleted. Its grants stay on each flare and open nothing. The name {team} is never used again, so no new team can take over those grants.",
        team = deleted.team
    );
    let step = Step::new("Each flare's owner removes the dormant grant from their flare.").call(
        Call::new(&["revoke"])
            .arg("id")
            .value("with", format!("team:{}", deleted.team)),
    );
    let rows = vec![TeamDeletedRow {
        team: deleted.team,
        grants_dormant: deleted.grants_dormant,
        name_reusable: deleted.name_reusable,
    }];
    Ok(Listing::from_all(rows, None)
        .described("teams")
        .note(note)
        .step(step))
}
