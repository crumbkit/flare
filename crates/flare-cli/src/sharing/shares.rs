//! `flare shares`: one page of the caller's shares or of the shares
//! granted to the caller, or every page with `--all`.

use super::{body, role_name, session};
use crate::cli::SharesArgs;
use crate::codes::sharing::shares_call;
use crate::codes::{Failed, NextRun, Refused, SharingFailed};
use crate::documents::{ShareRow, SharesNext};
use crate::env::Snapshot;
use crumbkit::{Listing, Step};
use flare_contract::{ApiCode, ListScope, ShareCursor, ShareList};
use reqwest::Method;
use std::cmp::Reverse;

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

/// The place of `cursor` in its list, which grows down the list: newest
/// first, then by share id.
fn place(cursor: ShareCursor) -> (Reverse<u64>, String) {
    (Reverse(cursor.seconds()), cursor.share_hex())
}

/// The server's `next` as a share cursor of `scope` that sorts after
/// `after`, the cursor the request started from. Any other text is an
/// answer flare cannot use, so `--all` never asks for the same page twice
/// or walks back up the list.
fn advanced(
    origin: &str,
    scope: ListScope,
    after: Option<ShareCursor>,
    next: &str,
) -> Result<ShareCursor, Failed> {
    ShareCursor::try_from(next)
        .ok()
        .filter(|cursor| cursor.scope() == scope)
        .filter(|cursor| after.is_none_or(|after| place(after) < place(*cursor)))
        .ok_or_else(|| {
            SharingFailed::BadAnswer {
                server: origin.to_owned(),
                problem: format!(
                    "the next cursor {next} is not a cursor of this list that moves past the last one"
                ),
                next: NextRun::MaybeDone,
            }
            .into()
        })
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
                })?,
        ),
        None => None,
    };
    let limit = args.page.limit().unwrap_or(MAX_SHARES);
    let mut rows = Vec::new();
    loop {
        let after = cursor
            .as_ref()
            .map_or_else(String::new, |cursor| format!("&cursor={cursor}"));
        let everyone = if args.everyone { "&everyone=true" } else { "" };
        let target = format!("/api/shares?scope={name}&limit={limit}{after}{everyone}");
        let answer: Option<ShareList> = api
            .send(Method::GET, &target, None::<&()>)
            .await
            .map_err(|failed| failed.for_shares(args.with_me))?;
        let list = body(&api, answer)?;
        let next = list
            .next
            .map(|next| advanced(api.origin(), scope, cursor, &next))
            .transpose()?;
        rows.extend(list.items.into_iter().map(row));
        match (next, args.page.limit()) {
            (Some(next), None) => cursor = Some(next),
            (next, _) => {
                let listing = Listing::uncounted(rows, next.is_some()).described("shares");
                let Some(next) = next.map(|next| next.to_string()) else {
                    return Ok(listing.summary(SharesNext { cursor: None }));
                };
                // The next call keeps the page size the caller asked for.
                let call = shares_call(args.with_me);
                let call = if args.everyone {
                    call.flag("everyone")
                } else {
                    call
                };
                let step = Step::new("Read the shares that follow this page.").call(
                    call.value("limit", limit.to_string())
                        .value("cursor", next.clone()),
                );
                return Ok(listing
                    .summary(SharesNext { cursor: Some(next) })
                    .step(step));
            }
        }
    }
}
