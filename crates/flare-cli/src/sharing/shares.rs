//! `flare shares`: one page of the caller's shares or of the shares
//! granted to the caller, or every page with `--all`.

use super::{body, role_name, session};
use crate::cli::SharesArgs;
use crate::codes::sharing::shares_call;
use crate::codes::{Failed, Refused};
use crate::documents::{ShareRow, SharesNext};
use crate::env::Snapshot;
use crumbkit::{Listing, Step};
use flare_contract::{ApiCode, ListScope, ShareCursor, ShareList};
use reqwest::Method;

/// The most shares the server lists at once, which each page of `--all`
/// asks for.
const MAX_SHARES: usize = 500;

/// The row of one share.
fn row(share: flare_contract::ShareSummary) -> ShareRow {
    ShareRow {
        id: share.id,
        title: share.title,
        owner: share.owner,
        role: role_name(share.role).to_owned(),
        version: share.version,
        created: share.created,
    }
}

/// Runs `flare shares`. A `--cursor` that is not a share cursor of the
/// list asked for fails before any request. With `--all` it follows the
/// server's cursor until no cursor comes back.
///
/// # Errors
///
/// Returns the failure that stopped it. A failure on a later page of
/// `--all` prints none of the rows already read.
pub(crate) async fn shares(
    args: &SharesArgs,
    env: &Snapshot,
) -> Result<Listing<ShareRow, SharesNext>, Failed> {
    let api = session(env)?;
    let (scope, name) = if args.with_me {
        (ListScope::WithMe, "with-me")
    } else {
        (ListScope::Mine, "mine")
    };
    let mut cursor = match &args.cursor {
        Some(text) => Some(
            ShareCursor::try_from(text.as_str())
                .ok()
                .filter(|cursor| cursor.scope() == scope)
                .ok_or_else(|| {
                    Failed::from(Refused::new(ApiCode::SharesBadCursor, None, api.origin()))
                        .for_shares(args.with_me)
                })?
                .to_string(),
        ),
        None => None,
    };
    let limit = args.page.limit().unwrap_or(MAX_SHARES);
    let mut rows = Vec::new();
    loop {
        let after = cursor
            .as_ref()
            .map_or_else(String::new, |cursor| format!("&cursor={cursor}"));
        let target = format!("/api/shares?scope={name}&limit={limit}{after}");
        let answer: Option<ShareList> = api
            .send(Method::GET, &target, None::<&()>)
            .await
            .map_err(|failed| failed.for_shares(args.with_me))?;
        let list = body(&api, answer)?;
        rows.extend(list.items.into_iter().map(row));
        match list.next {
            Some(next) if args.page.limit().is_none() => cursor = Some(next),
            next => {
                let listing = Listing::uncounted(rows, next.is_some()).described("shares");
                let Some(next) = next else {
                    return Ok(listing.summary(SharesNext { cursor: None }));
                };
                let step = Step::new("Read the shares that follow this page.")
                    .call(shares_call(args.with_me).value("cursor", next.clone()));
                return Ok(listing
                    .summary(SharesNext { cursor: Some(next) })
                    .step(step));
            }
        }
    }
}
