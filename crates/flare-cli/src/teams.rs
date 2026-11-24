//! `flare teams`: create, list, show, transfer and delete teams, and add or
//! remove their members, through the server's team routes.

use crate::cli::ListTeamsArgs;
use crate::codes::{Failed, Refused};
use crate::documents::teams::{MemberRow, MembersOut, TeamDeletedRow, TeamRow};
use crate::env::Snapshot;
use crate::sharing::{body, session};
use crumbkit::{Call, Listing, Step};
use flare_contract::{
    ApiCode, MembersChanged, MembersRequest, TeamDeleted, TeamList, TeamRequest, TeamRole,
    TeamShown, TeamSummary, TransferRequest,
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

/// The most calls one `flare teams delete` makes. Each call removes at
/// most 200 grants, so one run removes up to 200,000; a team with more
/// answers `finished` false, and running the command again goes on.
const MAX_DELETE_CALLS: usize = 1_000;

/// Runs `flare teams delete`, repeating the server's call until the delete
/// finishes.
///
/// # Errors
///
/// Returns the failure that stopped it. The team then opens nothing, and
/// running the command again goes on.
pub(crate) async fn delete(name: &str, env: &Snapshot) -> Result<Listing<TeamDeletedRow>, Failed> {
    let api = session(env)?;
    let target = team_path(api.origin(), name)?;
    let mut removed: u32 = 0;
    let mut team = name.to_owned();
    let mut finished = false;
    for _ in 0..MAX_DELETE_CALLS {
        let answer: Option<TeamDeleted> = api.send(Method::DELETE, &target, None::<&()>).await?;
        let deleted = body(&api, answer)?;
        removed = removed.saturating_add(deleted.grants_removed);
        team = deleted.team;
        finished = deleted.finished;
        if finished {
            break;
        }
    }
    let rows = vec![TeamDeletedRow {
        team: team.clone(),
        finished,
        grants_removed: removed,
    }];
    let listing = Listing::from_all(rows, None).described("teams");
    if finished {
        let note = format!(
            "The team {team} is deleted and removed its grant from {removed} flares. The name {team} is free, and a new team of that name gets none of this team's access."
        );
        return Ok(listing.note(note));
    }
    let note = format!(
        "The team {team} opens nothing and removed its grant from {removed} flares, and more grants remain."
    );
    let step = Step::new("Run the delete again to remove the remaining grants.").call(
        Call::new(&["teams", "delete"])
            .value("name", team)
            .flag("yes"),
    );
    Ok(listing.note(note).step(step))
}

/// Runs `flare teams transfer`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn transfer(name: &str, to: &str, env: &Snapshot) -> Result<TeamRow, Failed> {
    let api = session(env)?;
    let target = format!("{}/owner", team_path(api.origin(), name)?);
    let request = TransferRequest { to: to.to_owned() };
    let answer: Option<TeamSummary> = api.send(Method::PUT, &target, Some(&request)).await?;
    Ok(row(body(&api, answer)?))
}
