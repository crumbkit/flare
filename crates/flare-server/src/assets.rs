//! The reading page's script files, which the build script embeds from
//! `ui/flare/dist/assets`, served at `/flare/assets/<file>`. Each name
//! holds a hash of the file's content, so a reply is cached for a year.
//! A build with no bundle embeds no file, and the reading page then loads
//! no script.

use crate::problem::{Code, Problem};
use crate::reply::Reply;
use http::StatusCode;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

/// The entry script's file name, when the build embedded the bundle.
pub(crate) fn entry() -> Option<&'static str> {
    embedded::ENTRY
}

/// The embedded files, by name.
pub(crate) fn files() -> &'static [(&'static str, &'static [u8])] {
    embedded::FILES
}

/// The media type of a file, by its extension.
fn media_type(name: &str) -> &'static str {
    match name.rsplit_once('.').map(|(_, extension)| extension) {
        Some("js" | "mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("json") => "application/json",
        Some("svg") => "image/svg+xml",
        Some("woff2") => "font/woff2",
        _ => "application/octet-stream",
    }
}

/// `GET /flare/assets/<name>`: the embedded file, or not found.
pub(crate) fn serve(name: &str) -> Result<Reply, Problem> {
    let (name, bytes) = files()
        .iter()
        .find(|(file, _)| *file == name)
        .ok_or(Code::NotFound)?;
    Ok(Reply::with_body(StatusCode::OK, media_type(name), *bytes).immutable())
}

#[cfg(test)]
mod tests;
