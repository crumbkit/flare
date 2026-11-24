//! `GET /api/shares/{id}/versions/{n}/files/{file}`: one image of a
//! version, for a caller who may open that version, when the version's
//! image table lists the file. A share the caller cannot open, a missing
//! version, an unlisted file and a missing file all answer the same 404
//! `flare::shares::not_found`.

use crate::images::Format;
use crate::reply::Reply;
use crate::sharing::{Api, ReadRefusal, Refusal};
use flare_contract::ApiCode;
use flare_core::Principal;
use http::StatusCode;
use hyper::body::Bytes;

/// The share id, version and file id of the route's name, which is the
/// path from the share id on.
fn named(name: &str) -> Option<(&str, &str, &str)> {
    let mut segments = name.split('/');
    let id = segments.next()?;
    let version = segments
        .next()
        .filter(|word| *word == "versions")
        .and(segments.next())?;
    let file = segments
        .next()
        .filter(|word| *word == "files")
        .and(segments.next())?;
    segments.next().is_none().then_some((id, version, file))
}

/// The file the route names, as a reply that a browser shows inline in a
/// sandbox.
pub(super) async fn serve(api: &dyn Api, who: &Principal, name: &str) -> Result<Reply, Refusal> {
    let (id, version, file) = named(name).ok_or(ApiCode::ShareNotFound)?;
    let stored = match api.file(who, id, version, file).await {
        Ok(stored) => stored,
        Err(ReadRefusal::NotFound) => return Err(ApiCode::ShareNotFound.into()),
        Err(ReadRefusal::Unavailable) => return Err(ApiCode::SharesUnavailable.into()),
    };
    let media = Format::from_media_type(stored.media_type().as_str())
        .ok_or(ApiCode::ShareNotFound)?
        .media_type();
    Ok(Reply::file(
        StatusCode::OK,
        media,
        Bytes::copy_from_slice(stored.bytes().as_bytes()),
    ))
}
