//! `flare share`, `show`, `grant`, `revoke`, `grants` and `shares`: each
//! reads the saved login and its token from the Keychain, and calls the
//! server's share API. `flare share --dry-run` is the exception:
//! `dry_run.rs` checks and renders the file and reads neither the login nor
//! the Keychain. `flare activity` is in `activity.rs`.
//!
//! A Markdown file's relative images are read from the file's folder and
//! sent with it as a form. `images.rs` checks them first, and an image the
//! server would refuse, or whose file is missing or outside the folder,
//! stops the command before any request, a dry run included.

mod dry_run;
mod grants;
mod images;
mod outgoing;
mod shares;

pub(crate) use grants::{grants, revoke};

pub(crate) use images::{LocalPart, Part};
use outgoing::{Outgoing, outgoing};
pub(crate) use shares::{row, shares};

use crate::cli::{GrantArgs, ShareArgs, ShowArgs};
use crate::client::{self, Api};
use crate::codes::{Failed, NextRun, Refused, SharingFailed};
use crate::documents::{ShareOut, SharedOut, Shown};
use crate::env::Snapshot;
use crate::login::saved;
use crate::{config, keychain, token_command};
use flare_contract::{
    ApiCode, Format, GrantRequest, OpenedShare, Role, ShareSummary, Shared, VersionRequest,
};
use flare_core::{MAX_UPLOAD_BYTES, ShareId};
use reqwest::Method;
use std::io::Read as _;
use std::path::Path;

/// The largest file `flare share` reads, in bytes. The server takes far
/// less, and refuses a larger document with `flare::shares::too_large`.
const MAX_FILE: u64 = 4 * 1024 * 1024;

/// The images of what `args` sends, refused when the form that carries
/// them would be larger than the server reads.
fn images_of(args: &ShareArgs, outgoing: &Outgoing) -> Result<Vec<LocalPart>, Failed> {
    let (format, content) = match outgoing {
        Outgoing::Create(request) => (request.format, &request.content),
        Outgoing::Update(_, request) => (request.format, &request.content),
    };
    let images = images::images(&args.file, format, content)?;
    fits(outgoing, &images)?;
    Ok(images)
}

/// Refuses `images` when the form that sends them with `outgoing` is
/// larger than [`MAX_UPLOAD_BYTES`], the most the server reads, naming an
/// image that alone passes it, or else the largest image. No images go
/// as JSON, which this does not bound.
fn fits(outgoing: &Outgoing, images: &[LocalPart]) -> Result<(), Failed> {
    let Some(largest) = images.iter().max_by_key(|image| image.bytes.len()) else {
        return Ok(());
    };
    let limit = format!("{} MiB", MAX_UPLOAD_BYTES / (1024 * 1024));
    if largest.bytes.len() > MAX_UPLOAD_BYTES {
        return Err(SharingFailed::ImageOverUpload {
            image: largest.path.clone(),
            bytes: largest.bytes.len(),
            limit,
        }
        .into());
    }
    let share = match outgoing {
        Outgoing::Create(request) => serde_json::to_vec(request),
        Outgoing::Update(_, request) => serde_json::to_vec(request),
    }
    .map_err(|error| Failed::System {
        problem: format!("the request could not be written: {error}"),
    })?;
    let bytes = client::form(&client::boundary(&share, images), &share, images).len();
    if bytes > MAX_UPLOAD_BYTES {
        return Err(SharingFailed::UploadTooLarge {
            image: largest.path.clone(),
            bytes,
            limit,
        }
        .into());
    }
    Ok(())
}

/// A client of the server with its token. When the config's `client`
/// section names a token command, the server is that section's and the
/// token is the command's, and neither the saved login nor the Keychain is
/// read. Otherwise they are the saved login's.
///
/// # Errors
///
/// Returns the token command's failure, the config's, or
/// [`SharingFailed::LoginNeeded`] when no login is saved or the Keychain
/// holds no token for it.
pub(crate) fn session(env: &Snapshot) -> Result<Api, Failed> {
    if let Some(client) = config::load(None, env)?.client {
        let token = token_command::token(&client, env)?;
        return Ok(Api::new(&client.server, Some(token))?.with_helper(&client.command.program));
    }
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

/// The bytes of the file at `path`, at most `limit` and one more, so that the
/// caller sees a file over its limit as over it.
fn read_bytes(path: &Path, limit: usize) -> Result<Vec<u8>, Failed> {
    let unreadable = |error: std::io::Error| -> Failed {
        SharingFailed::FileUnreadable {
            path: path.display().to_string(),
            problem: error.to_string(),
        }
        .into()
    };
    let file = std::fs::File::open(path).map_err(unreadable)?;
    let mut bytes = Vec::new();
    file.take(u64::try_from(limit).unwrap_or(u64::MAX) + 1)
        .read_to_end(&mut bytes)
        .map_err(unreadable)?;
    Ok(bytes)
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
    let (outgoing, html) = outgoing(args)?;
    let images = match html {
        Some(file) => vec![file],
        None => images_of(args, &outgoing)?,
    };
    let sent_images = image_count(&images);
    if dry_run {
        return dry_run::check(
            args,
            outgoing,
            sent_images,
            images.first().filter(|file| file.part == Part::Html),
        );
    }
    let api = session(env)?;
    match outgoing {
        Outgoing::Update(id, request) => update(&api, &id, request, &images).await,
        Outgoing::Create(request) => {
            let shared: Shared = sent(&api, "/api/shares", &request, &images)
                .await
                .and_then(|answer| body(&api, answer))
                .map_err(Failed::after_a_write)?;
            Ok(ShareOut {
                id: Some(shared.share.id),
                title: Some(shared.share.title),
                version: Some(shared.share.version),
                granted: Some(shared.granted),
                responds_to: Some(shared.responds_to),
                images: sent_images,
                checks: Vec::new(),
            })
        }
    }
}

/// How many of `files` are images, which the command reports; the HTML file
/// of an HTML share is not one.
fn image_count(files: &[LocalPart]) -> usize {
    files.iter().filter(|file| file.part == Part::Image).count()
}

/// Posts `request` to `target`: as JSON when there are no images, and as
/// a form with them otherwise.
async fn sent<T: serde::de::DeserializeOwned>(
    api: &Api,
    target: &str,
    request: &impl serde::Serialize,
    images: &[LocalPart],
) -> Result<Option<T>, Failed> {
    if images.is_empty() {
        api.send(Method::POST, target, Some(request)).await
    } else {
        api.send_form(Method::POST, target, request, images).await
    }
}

/// Publishes `request` as the next version of the share `id`. The server
/// answers with the share's summary, whose version is the new number.
async fn update(
    api: &Api,
    id: &str,
    request: VersionRequest,
    images: &[LocalPart],
) -> Result<ShareOut, Failed> {
    let id = share_id(api, id)?;
    let target = format!("/api/shares/{}/versions", id.as_str());
    let share: ShareSummary = sent(api, &target, &request, images)
        .await
        .and_then(|answer| body(api, answer))
        .map_err(Failed::after_a_write)?;
    Ok(ShareOut {
        id: Some(share.id),
        title: Some(share.title),
        version: Some(share.version),
        granted: None,
        responds_to: None,
        images: image_count(images),
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
        format: format_name(opened.share.format).to_owned(),
        content: opened.content,
    })
}

/// The name of `format` as `flare show` prints it.
const fn format_name(format: Format) -> &'static str {
    match format {
        Format::Markdown => "markdown",
        Format::Html => "html",
    }
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
