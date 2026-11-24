//! `flare activity`: the responses and comments on flares, read from the
//! server's feed, with the named cursor that `--since-last` keeps here and
//! on the server.

use crate::cli::ActivityArgs;
use crate::client::Api;
use crate::codes::sharing::activity_call;
use crate::codes::{Failed, Refused};
use crate::cursors;
use crate::documents::{ActivityRow, Next};
use crate::env::Snapshot;
use crate::sharing::{body, session, share_id};
use crumbkit::{Listing, Step};
use flare_contract::{ActivityItem, ActivityPage, ApiCode, Cursor, NamedCursor};
use flare_core::{CursorName, ShareId};
use reqwest::Method;

/// The most responses the server lists at once, which each page of
/// `--all` asks for.
const MAX_RESPONSES: usize = 100;

fn activity_row(item: ActivityItem) -> Option<ActivityRow> {
    let row = |kind: &str, flare, comment, parent, by, body, at| ActivityRow {
        kind: kind.to_owned(),
        flare,
        share: None,
        title: None,
        comment,
        parent,
        by,
        version: None,
        body,
        at,
        also: Vec::new(),
    };
    match item {
        ActivityItem::Response {
            flare,
            share,
            title,
            by,
            version,
            at,
            also,
            ..
        } => Some(ActivityRow {
            share: Some(share),
            title: Some(title),
            version: Some(version),
            also: also.into_iter().map(|other| other.id).collect(),
            ..row("response", flare, None, None, by, None, at)
        }),
        ActivityItem::Comment {
            flare,
            comment,
            by,
            body,
            at,
        } => Some(row(
            "comment",
            flare,
            Some(comment),
            None,
            by,
            Some(body),
            at,
        )),
        ActivityItem::Reply {
            flare,
            comment,
            parent,
            by,
            body,
            at,
        } => Some(row(
            "reply",
            flare,
            Some(comment),
            Some(parent),
            by,
            Some(body),
            at,
        )),
        ActivityItem::Edited {
            flare,
            comment,
            parent,
            by,
            body,
            at,
        } => Some(row(
            "edited",
            flare,
            Some(comment),
            parent,
            by,
            Some(body),
            at,
        )),
        ActivityItem::Deleted {
            flare,
            comment,
            parent,
            by,
            at,
        } => Some(row("deleted", flare, Some(comment), parent, by, None, at)),
        // An item of a kind a newer server sends is left out.
        _ => None,
    }
}

/// The step that reads the activity after `next`: `flare activity` with
/// each of `flares`, in the order given, and the name the cursor was saved
/// under, or else the cursor of the last page.
fn next_step(flares: &[ShareId], next: &Next) -> Step {
    let sentence = if next.more {
        "Read the activity that waits past this page."
    } else {
        "Read the activity that arrives after this."
    };
    let call = activity_call(flares);
    let call = match &next.saved_as {
        Some(name) => call.value("since_last", name.clone()),
        None => call.value("since", next.cursor.clone()),
    };
    Step::new(sentence).call(call)
}

/// The cursor name that `--since-last` gives, which the server and this
/// computer's file both key by.
fn cursor_name(api: &Api, text: &str) -> Result<CursorName, Failed> {
    CursorName::try_from(text)
        .map_err(|_| Refused::new(ApiCode::BadQuery, None, api.origin()).into())
}

/// Runs `flare activity`. With `--all` it follows the cursor until the
/// server has no more. With `--since-last` it starts from the larger of
/// `--since` and the cursor this computer saved under the name, and ends by
/// saving the last cursor here and on the server.
///
/// This computer's file is saved before the result prints and fails the
/// command when it cannot be written, so a run that prints nothing leaves
/// the cursor where it was. The server's cursor serves the person's other
/// computers, so a refusal of its save leaves a note on standard error and
/// the exit code alone.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn activity(
    args: &ActivityArgs,
    env: &Snapshot,
) -> Result<Listing<ActivityRow, Next>, Failed> {
    let api = session(env)?;
    let mut query = String::new();
    let mut flares = Vec::with_capacity(args.flares.len());
    for flare in &args.flares {
        let id = share_id(&api, flare)?;
        query.push_str("flare=");
        query.push_str(id.as_str());
        query.push('&');
        flares.push(id);
    }
    let name = args
        .since_last
        .as_deref()
        .map(|text| cursor_name(&api, text))
        .transpose()?;
    let given = match &args.since {
        Some(text) => Some(Cursor::try_from(text.as_str()).map_err(|_| {
            Refused::new(ApiCode::BadCursor, None, api.origin()).for_flares(&flares)
        })?),
        None => None,
    };
    let kept = match &name {
        Some(name) => cursors::read(env, api.origin(), name.as_str())?,
        None => None,
    };
    let mut since = given.max(kept).map(|cursor| cursor.to_string());
    let last = name
        .as_ref()
        .map_or_else(String::new, |name| format!("since_last={name}&"));
    let limit = args.page.limit().unwrap_or(MAX_RESPONSES);
    let mut rows = Vec::new();
    loop {
        let after = since
            .as_ref()
            .map_or_else(String::new, |cursor| format!("since={cursor}&"));
        let target = format!("/api/activity?{query}{after}{last}limit={limit}");
        let answer: Option<ActivityPage> = api
            .send(Method::GET, &target, None::<&()>)
            .await
            .map_err(|failed| failed.for_flares(&flares))?;
        let page = body(&api, answer)?;
        rows.extend(page.items.into_iter().filter_map(activity_row));
        let mut next = Next {
            cursor: page.cursor,
            saved_as: None,
            more: page.more,
        };
        if args.page.limit().is_some() || !next.more {
            let more = next.more;
            let note = match &name {
                Some(name) => save(&api, env, name, &mut next).await?,
                None => None,
            };
            let step = next_step(&flares, &next);
            let listing = Listing::uncounted(rows, more)
                .described("activity items")
                .summary(next)
                .step(step);
            return Ok(match note {
                Some(note) => listing.note(note),
                None => listing,
            });
        }
        since = Some(next.cursor);
    }
}

/// Saves `next`'s cursor under `name` on this computer, then on the server.
/// It returns the note for a server that refused.
async fn save(
    api: &Api,
    env: &Snapshot,
    name: &CursorName,
    next: &mut Next,
) -> Result<Option<String>, Failed> {
    let cursor = Cursor::try_from(next.cursor.as_str())
        .map_err(|_| Refused::new(ApiCode::BadCursor, None, api.origin()))?;
    cursors::save(env, api.origin(), name.as_str(), cursor)?;
    next.saved_as = Some(name.to_string());
    let request = NamedCursor {
        cursor: next.cursor.clone(),
    };
    let target = format!("/api/cursors/{name}");
    Ok(api
        .send::<NamedCursor>(Method::PUT, &target, Some(&request))
        .await
        .err()
        .map(|failed| {
            format!(
                "The server did not save the cursor {name}, so other computers do not see it: {failed}"
            )
        }))
}
