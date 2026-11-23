//! `flare share`, `grant`, `revoke`, `grants`, `shares` and `activity`:
//! each reads the saved
//! login and its token from the Keychain, and calls the server's share API.

use crate::cli::{ActivityArgs, GrantArgs, GrantsArgs, RevokeArgs, ShareArgs, SharesArgs};
use crate::client::Api;
use crate::codes::{Failed, Refused, SharingFailed};
use crate::documents::{ActivityRow, GrantRow, Next, RevokedOut, ShareRow, SharedOut};
use crate::env::Snapshot;
use crate::keychain;
use crate::login::saved;
use crumbkit::Listing;
use flare_contract::{
    ActivityItem, ActivityPage, ApiCode, Cursor, Format, GrantList, GrantRequest, GrantState,
    GranteeKind, RevokeRequest, Revoked, Role, ShareList, ShareRequest, ShareSummary, Shared,
    VersionRequest,
};
use flare_core::ShareId;
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
fn share_id(api: &Api, text: &str) -> Result<ShareId, Failed> {
    ShareId::try_from(text)
        .map_err(|_| Refused::new(ApiCode::ShareNotFound, None, api.origin()).into())
}

fn shared_out(shared: Shared) -> SharedOut {
    SharedOut {
        id: shared.share.id,
        title: shared.share.title,
        version: shared.share.version,
        granted: Some(shared.granted),
        responds_to: Some(shared.responds_to),
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

fn read_file(path: &Path) -> Result<String, Failed> {
    let unreadable = |problem: String| -> Failed {
        SharingFailed::FileUnreadable {
            path: path.display().to_string(),
            problem,
        }
        .into()
    };
    let file = std::fs::File::open(path).map_err(|error| unreadable(error.to_string()))?;
    let mut text = String::new();
    file.take(MAX_FILE + 1)
        .read_to_string(&mut text)
        .map_err(|error| unreadable(error.to_string()))?;
    if u64::try_from(text.len()).unwrap_or(u64::MAX) > MAX_FILE {
        return Err(unreadable(format!("it is longer than {MAX_FILE} bytes")));
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
    let content = read_file(&args.file)?;
    if let Some(id) = &args.update {
        return update(&api, id, VersionRequest { format, content }).await;
    }
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

/// Publishes `request` as the next version of the share `id`. The server
/// answers with the share's summary, whose version is the new number.
async fn update(api: &Api, id: &str, request: VersionRequest) -> Result<SharedOut, Failed> {
    let id = share_id(api, id)?;
    let target = format!("/api/shares/{}/versions", id.as_str());
    let answer: Option<ShareSummary> = api.send(Method::POST, &target, Some(&request)).await?;
    let share = body(api, answer)?;
    Ok(SharedOut {
        id: share.id,
        title: share.title,
        version: share.version,
        granted: None,
        responds_to: None,
    })
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
        everyone: args.everyone,
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
    let everyone = if args.everyone { "&everyone=true" } else { "" };
    let target = format!("/api/shares?scope={scope}&limit={limit}{everyone}");
    let answer: Option<ShareList> = api.send(Method::GET, &target, None::<&()>).await?;
    let list = body(&api, answer)?;
    let rows = list
        .items
        .into_iter()
        .map(|share| ShareRow {
            id: share.id,
            title: share.title,
            owner: share.owner,
            role: role_name(share.role).to_owned(),
            version: share.version,
            created: share.created,
        })
        .collect();
    Ok(Listing::uncounted(rows, list.more).described("shares"))
}

fn role_name(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Reader => "reader",
    }
}

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

fn activity_row(item: ActivityItem) -> Option<ActivityRow> {
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
            flare,
            share,
            title,
            by,
            version,
            at,
            also: also.into_iter().map(|other| other.id).collect(),
        }),
        // An item of a kind a newer server sends is left out.
        _ => None,
    }
}

/// Runs `flare activity`. With `--all` it follows the cursor until the
/// server has no more.
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
    let mut since = match &args.since {
        Some(text) => Some(
            Cursor::try_from(text.as_str())
                .map_err(|_| Refused::new(ApiCode::BadCursor, None, api.origin()))?
                .to_string(),
        ),
        None => None,
    };
    let limit = args.page.limit().unwrap_or(MAX_RESPONSES);
    let mut rows = Vec::new();
    loop {
        let after = since
            .as_ref()
            .map_or_else(String::new, |cursor| format!("since={cursor}&"));
        let target = format!("/api/activity?{query}{after}limit={limit}");
        let answer: Option<ActivityPage> = api.send(Method::GET, &target, None::<&()>).await?;
        let page = body(&api, answer)?;
        rows.extend(page.items.into_iter().filter_map(activity_row));
        let next = Next {
            cursor: page.cursor,
            more: page.more,
        };
        if args.page.limit().is_some() || !next.more {
            let more = next.more;
            return Ok(Listing::uncounted(rows, more)
                .described("responses")
                .summary(next));
        }
        since = Some(next.cursor);
    }
}
