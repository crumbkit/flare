//! `flare share`, `grant`, `shares` and `activity`: each reads the saved
//! login and its token from the Keychain, and calls the server's share API.

use crate::cli::{ActivityArgs, GrantArgs, ShareArgs, SharesArgs};
use crate::client::Api;
use crate::codes::{Failed, Refused, SharingFailed};
use crate::cursors;
use crate::documents::{ActivityRow, Next, ShareRow, SharedOut};
use crate::env::Snapshot;
use crate::keychain;
use crate::login::saved;
use crumbkit::Listing;
use flare_contract::{
    ActivityItem, ActivityPage, ApiCode, Cursor, Format, GrantRequest, NamedCursor, Role,
    ShareList, ShareRequest, Shared,
};
use flare_core::{CursorName, ShareId};
use reqwest::Method;
use std::io::Read as _;
use std::path::Path;

/// The largest file `flare share` reads, in bytes. The server takes far
/// less, and refuses a larger document with `flare::shares::too_large`.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// The most shares the server lists at once, which `--all` asks for.
const MAX_SHARES: usize = 500;

/// The most responses the server lists at once, which each page of
/// `--all` asks for.
const MAX_RESPONSES: usize = 100;

/// A client of the saved login's server with its token.
///
/// # Errors
///
/// Returns [`SharingFailed::LoginNeeded`] when no login is saved or the
/// Keychain holds no token for it.
pub(crate) fn session(env: &Snapshot) -> Result<Api, Failed> {
    let saved = saved::required(env)?;
    let token = keychain::read(env, &saved.server)?.ok_or_else(|| SharingFailed::LoginNeeded {
        server: Some(saved.server.clone()),
    })?;
    Api::new(&saved.server, Some(token))
}

/// The answer, or a bad-answer failure when the server sent no body.
pub(crate) fn body<T>(api: &Api, answer: Option<T>) -> Result<T, Failed> {
    answer.ok_or_else(|| {
        SharingFailed::BadAnswer {
            server: api.origin().to_owned(),
            problem: "the answer has no body".to_owned(),
        }
        .into()
    })
}

/// The share id that `text` names; anything else is a missing share, as
/// the server would answer.
pub(crate) fn share_id(api: &Api, text: &str) -> Result<ShareId, Failed> {
    ShareId::try_from(text)
        .map_err(|_| Refused::new(ApiCode::ShareNotFound, None, api.origin()).into())
}

fn shared_out(shared: Shared) -> SharedOut {
    SharedOut {
        id: shared.share.id,
        title: shared.share.title,
        granted: shared.granted,
        responds_to: shared.responds_to,
    }
}

/// The file's format, by its ending.
fn format(path: &Path) -> Result<Format, Failed> {
    match path.extension().and_then(|ending| ending.to_str()) {
        Some("md" | "markdown") => Ok(Format::Markdown),
        Some("html" | "htm") => Ok(Format::Html),
        _ => Err(SharingFailed::UnknownFormat {
            path: path.display().to_string(),
        }
        .into()),
    }
}

pub(crate) fn read_file(path: &Path, limit: u64) -> Result<String, Failed> {
    let unreadable = |problem: String| -> Failed {
        SharingFailed::FileUnreadable {
            path: path.display().to_string(),
            problem,
        }
        .into()
    };
    let file = std::fs::File::open(path).map_err(|error| unreadable(error.to_string()))?;
    let mut text = String::new();
    file.take(limit + 1)
        .read_to_string(&mut text)
        .map_err(|error| unreadable(error.to_string()))?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > limit {
        return Err(unreadable(format!("it is longer than {limit} bytes")));
    }
    Ok(text)
}

/// Runs `flare share`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn share(args: &ShareArgs, env: &Snapshot) -> Result<SharedOut, Failed> {
    let format = format(&args.file)?;
    let api = session(env)?;
    let content = read_file(&args.file, MAX_FILE)?;
    let title = args.title.clone().unwrap_or_else(|| {
        args.file
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
    });
    let request = ShareRequest {
        title,
        format,
        content,
        with: args.with.clone(),
        re: args.re.clone(),
    };
    let answer = api
        .send(Method::POST, "/api/shares", Some(&request))
        .await?;
    Ok(shared_out(body(&api, answer)?))
}

/// Runs `flare grant`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn grant(args: &GrantArgs, env: &Snapshot) -> Result<SharedOut, Failed> {
    let api = session(env)?;
    let id = share_id(&api, &args.id)?;
    let request = GrantRequest {
        with: args.with.clone(),
        re: args.re.clone(),
    };
    let target = format!("/api/grants/{}", id.as_str());
    let answer = api.send(Method::POST, &target, Some(&request)).await?;
    Ok(shared_out(body(&api, answer)?))
}

/// Runs `flare shares`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn shares(args: &SharesArgs, env: &Snapshot) -> Result<Listing<ShareRow>, Failed> {
    let api = session(env)?;
    let scope = if args.with_me { "with-me" } else { "mine" };
    let limit = args.page.limit().unwrap_or(MAX_SHARES);
    let target = format!("/api/shares?scope={scope}&limit={limit}");
    let answer: Option<ShareList> = api.send(Method::GET, &target, None::<&()>).await?;
    let list = body(&api, answer)?;
    let rows = list
        .items
        .into_iter()
        .map(|share| ShareRow {
            id: share.id,
            title: share.title,
            owner: share.owner,
            role: match share.role {
                Role::Owner => "owner",
                Role::Admin => "admin",
                Role::Reader => "reader",
            }
            .to_owned(),
            version: share.version,
            created: share.created,
        })
        .collect();
    Ok(Listing::uncounted(rows, list.more).described("shares"))
}

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
    for flare in &args.flares {
        let id = share_id(&api, flare)?;
        query.push_str("flare=");
        query.push_str(id.as_str());
        query.push('&');
    }
    let name = args
        .since_last
        .as_deref()
        .map(|text| cursor_name(&api, text))
        .transpose()?;
    let given = match &args.since {
        Some(text) => Some(
            Cursor::try_from(text.as_str())
                .map_err(|_| Refused::new(ApiCode::BadCursor, None, api.origin()))?,
        ),
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
        let answer: Option<ActivityPage> = api.send(Method::GET, &target, None::<&()>).await?;
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
            let listing = Listing::uncounted(rows, more)
                .described("activity items")
                .summary(next);
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
