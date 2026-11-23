//! `flare revoke` and `flare grants`: end a share's grants and list them.

use super::{body, role_name, session, share_id};
use crate::cli::{GrantsArgs, RevokeArgs};
use crate::codes::Failed;
use crate::documents::{GrantRow, RevokedOut};
use crate::env::Snapshot;
use crumbkit::Listing;
use flare_contract::{GrantList, GrantState, GranteeKind, RevokeRequest, Revoked};
use reqwest::Method;

/// Runs `flare revoke`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn revoke(args: &RevokeArgs, env: &Snapshot) -> Result<RevokedOut, Failed> {
    let api = session(env)?;
    let id = share_id(&api, &args.id)?;
    let request = RevokeRequest {
        with: args.with.clone(),
        everyone: args.everyone,
    };
    let target = format!("/api/grants/{}", id.as_str());
    let answer: Option<Revoked> = api.send(Method::DELETE, &target, Some(&request)).await?;
    let revoked = body(&api, answer)?;
    Ok(RevokedOut {
        id: revoked.share,
        revoked: revoked.revoked,
    })
}

/// Runs `flare grants`. Dormant grants, to deleted teams, are listed
/// first.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn grants(args: &GrantsArgs, env: &Snapshot) -> Result<Listing<GrantRow>, Failed> {
    let api = session(env)?;
    let id = share_id(&api, &args.id)?;
    let target = format!("/api/grants/{}", id.as_str());
    let answer: Option<GrantList> = api.send(Method::GET, &target, None::<&()>).await?;
    let mut items = body(&api, answer)?.items;
    items.sort_by_key(|entry| entry.state != GrantState::Dormant);
    let rows = items
        .into_iter()
        .map(|entry| GrantRow {
            grantee: entry.grantee,
            kind: match entry.kind {
                GranteeKind::Person => "person",
                GranteeKind::Team => "team",
                GranteeKind::Everyone => "everyone",
            }
            .to_owned(),
            role: role_name(entry.role).to_owned(),
            state: match entry.state {
                GrantState::Active => "active",
                GrantState::Dormant => "dormant",
            }
            .to_owned(),
            granted_by: entry.granted_by,
            created: entry.created,
            audience: entry.audience,
        })
        .collect();
    Ok(Listing::from_all(rows, None).described("grants"))
}
