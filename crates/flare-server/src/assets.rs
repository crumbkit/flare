//! The reading page's script files, which the build script embeds from
//! `ui/flare/dist/assets`, served at `/flare/assets/<file>`: the reading
//! entry, which colours code and draws diagrams, the comments entry, and
//! the chunks they import. Each name
//! holds a hash of the file's content, so a reply is cached for a year.
//! A build with no bundle embeds no file, and the reading page then loads
//! no script. The same route serves the two fonts that the stylesheet
//! names, IBM Plex Sans and JetBrains Mono, vendored under `fonts/` with
//! their licences, so no page asks a third-party host for a font.

use crate::problem::{Code, Problem};
use crate::reply::Reply;
use http::StatusCode;

mod embedded {
    include!(concat!(env!("OUT_DIR"), "/ui_assets.rs"));
}

/// The reading entry script's file name, when the build embedded the
/// bundle.
pub(crate) fn entry() -> Option<&'static str> {
    embedded::ENTRY
}

/// The comments entry script's file name, when the build embedded the
/// bundle.
pub(crate) fn comments_entry() -> Option<&'static str> {
    embedded::COMMENTS
}

/// The embedded files, by name.
pub(crate) fn files() -> &'static [(&'static str, &'static [u8])] {
    embedded::FILES
}

/// The vendored fonts, by name: the Latin subset of each family's variable
/// font, from the `@fontsource-variable` packages at version 5.3.0. Each
/// name ends with the first 8 hex digits of the file's SHA-256, so a
/// changed file has a new name and a year of caching is safe.
pub(crate) const FONTS: &[(&str, &[u8])] = &[
    (
        "ibm-plex-sans-latin-e2291e84.woff2",
        include_bytes!("../fonts/ibm-plex-sans-latin-e2291e84.woff2"),
    ),
    (
        "jetbrains-mono-latin-18be4527.woff2",
        include_bytes!("../fonts/jetbrains-mono-latin-18be4527.woff2"),
    ),
];

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
        .chain(FONTS)
        .find(|(file, _)| *file == name)
        .ok_or(Code::NotFound)?;
    Ok(Reply::with_body(StatusCode::OK, media_type(name), *bytes).immutable())
}

#[cfg(test)]
mod tests;
