//! `flare share`, `show`, `grant`, `revoke`, `grants` and `shares`: each
//! reads the saved login and its token from the Keychain, and calls the
//! server's share API. `flare share --dry-run` is the exception:
//! `dry_run.rs` checks and renders the file and reads neither the login nor
//! the Keychain. `flare activity` is in `activity.rs`.

mod dry_run;
mod grants;
mod shares;

pub(crate) use grants::{grants, revoke};

pub(crate) use shares::shares;

use crate::cli::{GrantArgs, ShareArgs, ShowArgs};
use crate::client::Api;
use crate::codes::{Failed, NextRun, Refused, SharingFailed};
use crate::documents::{ShareOut, SharedOut, Shown};
use crate::env::Snapshot;
use crate::keychain;
use crate::login::saved;
use flare_contract::{
    ApiCode, Format, GrantRequest, OpenedShare, Role, ShareRequest, ShareSummary, Shared,
    VersionRequest,
};
use flare_core::ShareId;
use reqwest::Method;
use std::io::Read as _;
use std::path::Path;

/// The largest file `flare share` reads, in bytes. The server takes far
/// less, and refuses a larger document with `flare::shares::too_large`.
const MAX_FILE: u64 = 4 * 1024 * 1024;

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
pub(crate) fn share_id(api: &Api, text: &str) -> Result<ShareId, Failed> {
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
    let content = read_file(&args.file, MAX_FILE)?;
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
        everyone: args.everyone,
        re: args.re.clone(),
    };
    let target = format!("/api/grants/{}", id.as_str());
    let answer = api.send(Method::POST, &target, Some(&request)).await?;
    Ok(shared_out(body(&api, answer)?))
}
