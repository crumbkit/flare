//! A share or a version sent as `multipart/form-data` with its images: one
//! `share` part holding the JSON body that `application/json` would carry,
//! and one `image` part per image, whose filename is the path as the
//! Markdown wrote it.

use super::super::Ctx;
use super::super::edit::read_limited;
use super::ApiRoute;
use super::multipart::{self, Part};
use crate::metrics::Event;
use crate::problem::{Code, Problem};
use crate::sharing::{Answer, Api, ImagePart, Refusal, Upload};
use crate::sign_in::HostedSignIn;
use flare_auth_oidc::Clock as OidcClock;
use flare_contract::ApiCode;
use flare_core::{Principal, Timestamp};
use http::header::CONTENT_TYPE;
use hyper::body::{Bytes, Incoming};
use serde::de::DeserializeOwned;

/// The boundary of a create or a publish sent as a multipart form, or
/// `None` for any other request.
pub(super) fn boundary_of<P, R, S, K: OidcClock, H: HostedSignIn<K>>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    route: ApiRoute,
) -> Option<String> {
    if !matches!(route, ApiRoute::CreateShare | ApiRoute::PublishVersion) {
        return None;
    }
    let value = ctx.headers.get(CONTENT_TYPE)?.to_str().ok()?;
    multipart::boundary(value)
}

/// The bytes a form may hold beyond `Limits::upload_bytes` for an HTML
/// file: the boundary lines, the share part and the part headers.
const HTML_FRAMING: usize = 64 * 1024;

/// A refusal of a form that this server does not read.
fn bad_form(detail: &str) -> Refusal {
    Refusal {
        code: ApiCode::BadBody,
        detail: Some(detail.to_owned()),
        share: None,
        errors: Vec::new(),
    }
}

/// The JSON body of the `share` part.
fn json_of<T: DeserializeOwned>(bytes: &Bytes) -> Result<T, Refusal> {
    serde_json::from_slice(bytes).map_err(|error| Refusal {
        code: ApiCode::BadBody,
        detail: Some(format!(
            "share part line {}, column {}",
            error.line(),
            error.column()
        )),
        share: None,
        errors: Vec::new(),
    })
}

/// Whether `part` is the form's `html` part, which holds the file of an
/// HTML share under any filename.
fn is_html(part: &Part) -> bool {
    part.name == "html"
}

/// The upload that the form `parts` of `route` holds.
pub(super) fn upload_of(route: ApiRoute, name: &str, parts: Vec<Part>) -> Result<Upload, Refusal> {
    let mut share = None;
    let mut images = Vec::new();
    let mut html = None;
    for part in parts {
        if is_html(&part) && html.is_none() {
            html = Some(part.data);
            continue;
        }
        match (part.name.as_str(), part.filename) {
            ("share", None) if share.is_none() => share = Some(part.data),
            ("image", Some(path)) => images.push(ImagePart {
                path,
                bytes: part.data,
            }),
            _ => {
                return Err(bad_form(
                    "a form holds one share part and image parts with filenames",
                ));
            }
        }
    }
    let share = share.ok_or_else(|| bad_form("the form has no share part"))?;
    if let Some(file) = html {
        if !images.is_empty() {
            return Err(bad_form(
                "a form holds an html part or image parts, not both",
            ));
        }
        return match route {
            ApiRoute::PublishVersion => {
                Ok(Upload::PublishHtml(name.to_owned(), json_of(&share)?, file))
            }
            _ => Ok(Upload::CreateHtml(json_of(&share)?, file)),
        };
    }
    match route {
        ApiRoute::PublishVersion => Ok(Upload::Publish(name.to_owned(), json_of(&share)?, images)),
        _ => Ok(Upload::Create(json_of(&share)?, images)),
    }
}

/// Reads the form of `route` on share `name`, runs its upload, and counts
/// a delete that failed.
pub(super) async fn run<P, R, S, K: OidcClock, H: HostedSignIn<K>>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    api: &dyn Api,
    who: &Principal,
    (route, name): (ApiRoute, &str),
    boundary: &str,
    body: Incoming,
    now: Timestamp,
) -> Result<Answer, Problem> {
    // An HTML file may be as large as `MAX_HTML`, and its form adds framing
    // beyond `upload_bytes`, so the read allows `HTML_FRAMING` more and a
    // form with no HTML file is held to `upload_bytes` after it is read.
    let most = ctx.shared.limits.upload_bytes;
    let limit = (most.saturating_add(HTML_FRAMING), Code::UploadTooLarge);
    let bytes = read_limited(ctx, body, limit).await?;
    let Some(parts) = multipart::parse(&bytes, boundary) else {
        return Ok(Err(bad_form("the body is not a form this server reads")));
    };
    let upload = match upload_of(route, name, parts) {
        Ok(upload) => upload,
        Err(refusal) => return Ok(Err(refusal)),
    };
    let html = matches!(upload, Upload::CreateHtml(..) | Upload::PublishHtml(..));
    if html && ctx.shared.content.is_none() {
        return Ok(Err(ApiCode::HtmlNoContentHost.into()));
    }
    if !html && bytes.len() > most {
        return Err(Code::UploadTooLarge.into());
    }
    let uploaded = api
        .upload(who, upload, now, ctx.shared.clock.as_ref())
        .await;
    if uploaded.cleanup_failed {
        ctx.shared.count(Event::FileCleanupFailed);
    }
    Ok(uploaded.answer)
}

#[cfg(test)]
mod tests;
