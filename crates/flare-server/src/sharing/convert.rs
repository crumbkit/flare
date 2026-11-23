//! Turns flare-core's shares, activity and refusals into the contract's
//! bodies and codes.

use super::{BodyRefused, Refusal};
use flare_contract::{
    ActivityItem as WireItem, ApiCode, FlareRef, Format, GrantEntry, GrantState, GranteeKind,
    Role as WireRole, ShareSummary, TeamRole, TeamSummary, utc,
};
use flare_core::{
    ActivityItem, Email, FlareError, Group, ListedGrant, ParseError, RecipientError, Role, Share,
    ShareError, Standing, Team, TeamsError,
};

/// The summary of `share` for a caller with `role`.
pub(super) fn summary(share: &Share, role: WireRole) -> ShareSummary {
    ShareSummary {
        id: share.id().as_str().to_owned(),
        title: share.title().as_str().to_owned(),
        format: Format::Markdown,
        owner: share.owner().as_str().to_owned(),
        role,
        version: share.latest().get(),
        created: utc(share.created().as_secs()),
    }
}

/// The contract's role for a grant's role.
pub(super) fn role(role: Role) -> WireRole {
    match role {
        Role::Admin => WireRole::Admin,
        _ => WireRole::Reader,
    }
}

/// The contract's role for a standing that opened a share.
pub(super) fn standing(standing: Standing) -> WireRole {
    match standing {
        Standing::Owner => WireRole::Owner,
        Standing::Admin => WireRole::Admin,
        _ => WireRole::Reader,
    }
}

/// The contract's activity item.
pub(super) fn item(item: &ActivityItem) -> WireItem {
    WireItem::Response {
        flare: item.flare.as_str().to_owned(),
        share: item.response.id().as_str().to_owned(),
        title: item.response.title().as_str().to_owned(),
        format: Format::Markdown,
        by: item.response.owner().as_str().to_owned(),
        version: item.version.get(),
        at: utc(item.at.as_secs()),
        also: item
            .also
            .iter()
            .map(|share| FlareRef {
                id: share.id().as_str().to_owned(),
                title: share.title().as_str().to_owned(),
            })
            .collect(),
    }
}

/// The code of a share service refusal. A refused share and a missing one
/// share a code, and so do a version and a grant that a caller cannot see.
pub(super) fn share_error(error: ShareError) -> ApiCode {
    match error {
        ShareError::NotFound | ShareError::VersionNotFound | ShareError::GrantNotFound => {
            ApiCode::ShareNotFound
        }
        ShareError::Forbidden | ShareError::OwnerCannotBeChanged => ApiCode::ShareForbidden,
        ShareError::GrantChanged => ApiCode::GrantChanged,
        ShareError::TooManyGrants => ApiCode::TooManyGrants,
        ShareError::TooManyShares => ApiCode::TooManyShares,
        ShareError::TooManyVersions => ApiCode::TooManyVersions,
        ShareError::NoVerifiedEmail => ApiCode::NoVerifiedEmail,
        ShareError::TeamNotFound => ApiCode::TeamNotFound,
        ShareError::EveryoneUnbounded => ApiCode::EveryoneUnbounded,
        _ => ApiCode::SharesUnavailable,
    }
}

/// The refusal for a [`FlareError`]. A partly finished share names the
/// share, so the owner can finish it with `flare grant`.
pub(super) fn flare_error(error: FlareError) -> Refusal {
    match error {
        FlareError::NotACreator => ApiCode::NotACreator.into(),
        FlareError::NotFound => ApiCode::ShareNotFound.into(),
        FlareError::TooManyFlares => ApiCode::TooManyFlares.into(),
        FlareError::PartlyShared {
            share,
            granted,
            cause,
        } => Refusal {
            code: ApiCode::PartlyShared,
            detail: Some(format!(
                "{granted} grants were written before this failed: {}",
                share_error(cause).title()
            )),
            share: Some(share.as_str().to_owned()),
        },
        FlareError::Share(error) => share_error(error).into(),
        _ => ApiCode::ActivityUnavailable.into(),
    }
}

/// The refusal for a recipient that does not resolve.
pub(super) fn recipient_error(error: RecipientError, identifier: &str) -> Refusal {
    match error {
        RecipientError::NotAnAddress => Refusal {
            code: ApiCode::NotAnAddress,
            detail: Some(format!("{identifier} is not an email address")),
            share: None,
        },
        _ => ApiCode::RecipientsUnavailable.into(),
    }
}

/// The refusal for a title or content that its type refused: too large
/// for a document over the limit, and invalid otherwise.
pub(super) fn content_error(error: &ParseError) -> BodyRefused {
    match error {
        ParseError::TooLong {
            subject: flare_core::Subject::DocumentText,
            ..
        } => BodyRefused {
            code: ApiCode::ShareTooLarge,
            detail: None,
        },
        other => BodyRefused {
            code: ApiCode::ShareInvalid,
            detail: Some(other.to_string()),
        },
    }
}

/// The code of a teams service refusal.
pub(super) fn teams_error(error: TeamsError) -> ApiCode {
    match error {
        TeamsError::NotFound => ApiCode::TeamNotFound,
        TeamsError::Forbidden => ApiCode::TeamForbidden,
        TeamsError::OwnerCannotLeave => ApiCode::TeamOwnerCannotLeave,
        TeamsError::NotACreator => ApiCode::TeamNotACreator,
        TeamsError::Taken => ApiCode::TeamNameTaken,
        TeamsError::TooManyMembers => ApiCode::TooManyMembers,
        TeamsError::TooManyOwned => ApiCode::TooManyTeamsOwned,
        TeamsError::TooManyJoined => ApiCode::TooManyTeamsJoined,
        TeamsError::ManagedElsewhere => ApiCode::TeamsManagedElsewhere,
        _ => ApiCode::TeamsUnavailable,
    }
}

/// The summary of `team` for the caller with the address `caller`.
pub(super) fn team_summary(team: &Team, caller: &Email) -> TeamSummary {
    TeamSummary {
        name: team.name().as_str().to_owned(),
        owner: team.owner().as_str().to_owned(),
        role: if team.owner() == caller {
            TeamRole::Owner
        } else {
            TeamRole::Member
        },
        members: u32::try_from(team.members()).unwrap_or(u32::MAX),
        created: utc(team.created().as_secs()),
    }
}

/// What a listing shows in place of the name of a team that the caller
/// does not belong to. `*` cannot occur in a team's name.
const HIDDEN_TEAM: &str = "team:*";

/// The contract's entry for one listed grant.
pub(super) fn grant_entry(listed: &ListedGrant) -> GrantEntry {
    let grant = &listed.grant;
    let grantee = if listed.hidden {
        HIDDEN_TEAM.to_owned()
    } else {
        grant.grantee().as_str().to_owned()
    };
    let kind = match Group::of_grantee(grant.grantee()) {
        Some(Group::Team(_)) => GranteeKind::Team,
        Some(Group::Everyone) => GranteeKind::Everyone,
        None => GranteeKind::Person,
    };
    GrantEntry {
        grantee,
        kind,
        role: role(grant.role()),
        state: if listed.dormant {
            GrantState::Dormant
        } else {
            GrantState::Active
        },
        granted_by: grant.granted_by().as_str().to_owned(),
        created: utc(grant.created().as_secs()),
        audience: grant
            .audience()
            .map(|audience| audience.as_str().to_owned()),
    }
}
