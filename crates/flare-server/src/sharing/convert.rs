//! Turns flare-core's shares, activity and refusals into the contract's
//! bodies and codes.

use super::Refusal;
use flare_contract::{
    ActivityItem as WireItem, ApiCode, FlareRef, Format, Role as WireRole, ShareSummary, utc,
};
use flare_core::{
    ActivityItem, FlareError, ParseError, RecipientError, Role, Share, ShareError, Standing,
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
        ShareError::TooManyShares | ShareError::TooManyVersions => ApiCode::TooManyShares,
        ShareError::NoVerifiedEmail => ApiCode::NoVerifiedEmail,
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
pub(super) fn content_error(error: &ParseError) -> Refusal {
    match error {
        ParseError::TooLong {
            subject: flare_core::Subject::DocumentText,
            ..
        } => ApiCode::ShareTooLarge.into(),
        other => Refusal {
            code: ApiCode::ShareInvalid,
            detail: Some(other.to_string()),
            share: None,
        },
    }
}
