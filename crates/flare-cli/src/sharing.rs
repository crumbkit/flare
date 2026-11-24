//! `flare share`, `show`, `grant`, `shares` and `activity`: each reads the
//! saved login and its token from the Keychain, and calls the server's
//! share API. `flare share --dry-run` is the exception: `dry_run.rs` checks
//! and renders the file and reads neither the login nor the Keychain.

mod dry_run;
mod shares;

pub(crate) use shares::shares;

use crate::cli::{ActivityArgs, GrantArgs, ShareArgs, ShowArgs};
use crate::client::Api;
use crate::codes::sharing::activity_call;
use crate::codes::{Failed, NextRun, Refused, SharingFailed};
use crate::documents::{ActivityRow, Next, ShareOut, SharedOut, Shown};
use crate::env::Snapshot;
use crate::keychain;
use crate::login::saved;
use crumbkit::{Listing, Step};
use flare_contract::{
    ActivityItem, ActivityPage, ApiCode, Cursor, Format, GrantRequest, OpenedShare, Role,
    ShareRequest, ShareSummary, Shared, VersionRequest,
};
use flare_core::ShareId;
use reqwest::Method;
use std::io::Read as _;
use std::path::Path;

/// The largest file `flare share` reads, in bytes. The server takes far
/// less, and refuses a larger document with `flare::shares::too_large`.
const MAX_FILE: u64 = 4 * 1024 * 1024;

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

/// The answer, or a bad-answer failure when the server sent a success
/// status with no body.
pub(crate) fn body<T>(api: &Api, answer: Option<T>) -> Result<T, Failed> {
    answer.ok_or_else(|| {
        SharingFailed::BadAnswer {
            server: api.origin().to_owned(),
            problem: "the answer has no body".to_owned(),
            next: NextRun::MaybeDone,
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

/// What `flare share` sends: a new share, or the next version of one.
enum Outgoing {
    /// `POST /api/shares`.
    Create(ShareRequest),
    /// `POST /api/shares/{id}/versions`.
    Update(String, VersionRequest),
}

/// The request that `args` describes, read from its file.
fn outgoing(args: &ShareArgs) -> Result<Outgoing, Failed> {
    let format = format(&args.file)?;
    let content = read_file(&args.file)?;
    if let Some(id) = &args.update {
        return Ok(Outgoing::Update(
            id.clone(),
            VersionRequest { format, content },
        ));
    }
    let title = args.title.clone().unwrap_or_else(|| {
        args.file
            .file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
    });
    Ok(Outgoing::Create(ShareRequest {
        title,
        format,
        content,
        with: args.with.clone(),
        re: args.re.clone(),
    }))
}

/// Runs `flare share`. A dry run checks and renders the file, and reads
/// neither the saved login nor the Keychain.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn share(
    args: &ShareArgs,
    dry_run: bool,
    env: &Snapshot,
) -> Result<ShareOut, Failed> {
    let outgoing = outgoing(args)?;
    if dry_run {
        return dry_run::check(args, outgoing);
    }
    let api = session(env)?;
    match outgoing {
        Outgoing::Update(id, request) => update(&api, &id, request).await,
        Outgoing::Create(request) => {
            let shared: Shared = api
                .send(Method::POST, "/api/shares", Some(&request))
                .await
                .and_then(|answer| body(&api, answer))
                .map_err(Failed::after_a_write)?;
            Ok(ShareOut {
                id: Some(shared.share.id),
                title: Some(shared.share.title),
                version: Some(shared.share.version),
                granted: Some(shared.granted),
                responds_to: Some(shared.responds_to),
                checks: Vec::new(),
            })
        }
    }
}

/// Publishes `request` as the next version of the share `id`. The server
/// answers with the share's summary, whose version is the new number.
async fn update(api: &Api, id: &str, request: VersionRequest) -> Result<ShareOut, Failed> {
    let id = share_id(api, id)?;
    let target = format!("/api/shares/{}/versions", id.as_str());
    let share: ShareSummary = api
        .send(Method::POST, &target, Some(&request))
        .await
        .and_then(|answer| body(api, answer))
        .map_err(Failed::after_a_write)?;
    Ok(ShareOut {
        id: Some(share.id),
        title: Some(share.title),
        version: Some(share.version),
        granted: None,
        responds_to: None,
        checks: Vec::new(),
    })
}

/// Runs `flare show`: one version of a share, or its latest.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn show(args: &ShowArgs, env: &Snapshot) -> Result<Shown, Failed> {
    let api = session(env)?;
    let id = share_id(&api, &args.id)?;
    let query = args
        .version
        .map_or_else(String::new, |number| format!("?version={number}"));
    let target = format!("/api/shares/{}{query}", id.as_str());
    let answer: Option<OpenedShare> = api.send(Method::GET, &target, None::<&()>).await?;
    let opened = body(&api, answer)?;
    Ok(Shown {
        id: opened.share.id,
        title: opened.share.title,
        owner: opened.share.owner,
        role: role_name(opened.share.role).to_owned(),
        version: opened.version,
        latest: opened.share.version,
        content: opened.content,
    })
}

/// The name of `role` as the commands print it.
pub(crate) fn role_name(role: Role) -> &'static str {
    match role {
        Role::Owner => "owner",
        Role::Admin => "admin",
        Role::Reader => "reader",
    }
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

/// The step that reads the responses after `next`: `flare activity` with
/// each of `flares`, in the order given, and the cursor of the last page.
fn next_step(flares: &[ShareId], next: &Next) -> Step {
    let sentence = if next.more {
        "Read the responses that wait past this page."
    } else {
        "Read the responses that arrive after these."
    };
    Step::new(sentence).call(activity_call(flares).value("since", next.cursor.clone()))
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
    let mut flares = Vec::with_capacity(args.flares.len());
    for flare in &args.flares {
        let id = share_id(&api, flare)?;
        query.push_str("flare=");
        query.push_str(id.as_str());
        query.push('&');
        flares.push(id);
    }
    let mut since = match &args.since {
        Some(text) => Some(
            Cursor::try_from(text.as_str())
                .map_err(|_| {
                    Refused::new(ApiCode::BadCursor, None, api.origin()).for_flares(&flares)
                })?
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
        let answer: Option<ActivityPage> = api
            .send(Method::GET, &target, None::<&()>)
            .await
            .map_err(|failed| failed.for_flares(&flares))?;
        let page = body(&api, answer)?;
        rows.extend(page.items.into_iter().filter_map(activity_row));
        let next = Next {
            cursor: page.cursor,
            more: page.more,
        };
        if args.page.limit().is_some() || !next.more {
            let more = next.more;
            let step = next_step(&flares, &next);
            return Ok(Listing::uncounted(rows, more)
                .described("responses")
                .summary(next)
                .step(step));
        }
        since = Some(next.cursor);
    }
}
