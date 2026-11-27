//! The frame that holds a rendered page: `<iframe sandbox srcdoc="...">`,
//! whose empty sandbox gives the page an opaque origin and no scripts.
//!
//! Safari sends the session cookie on a sandboxed frame's own requests
//! (/evals/flare-srcdoc-frame), so the frame document's head starts with
//! the frame policy, before any page markup, and the page can send no
//! request at all. Pages show only `data:` images.

use crate::docs::escape;
use flare_render::PageDocument;

/// The frame policy, the first element of the frame document's head.
pub(crate) const FRAME_POLICY: &str = "<meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; style-src 'unsafe-inline'; img-src data:\">";

/// The head that `flare_render::render_page` opens every document with.
const HEAD: &str = "<head>\n";

/// The frame markup for `document`, with what the caption says about it.
/// `None` when the document has no head, which `render_page` never gives.
pub(crate) fn frame(document: &PageDocument, caption: &str) -> Option<String> {
    let text = document.as_str();
    let at = text.find(HEAD)? + HEAD.len();
    let (head, rest) = (text.get(..at)?, text.get(at..)?);
    let framed = format!("{head}{FRAME_POLICY}\n{rest}");
    Some(format!(
        "<div class=\"mat\"><div class=\"cap\"><span>Page content, written by an agent</span><span>{}</span></div><iframe sandbox srcdoc=\"{}\" class=\"frame\" title=\"Page content\"></iframe></div>",
        escape(caption),
        escape(&framed)
    ))
}

/// The frame markup for the uploaded page that the content host serves at
/// `src`, with what the caption says about it. The sandbox grants scripts
/// and nothing else, so the page runs in an opaque origin that reaches
/// neither flare's storage nor its session, and it sends no referrer and
/// asks for no browser feature.
pub(crate) fn uploaded(src: &str, caption: &str) -> String {
    format!(
        "<div class=\"mat\"><div class=\"cap\"><span>Page content, uploaded by its owner</span><span>{}</span></div><iframe sandbox=\"allow-scripts\" src=\"{}\" referrerpolicy=\"no-referrer\" allow=\"\" class=\"frame\" title=\"Uploaded page\"></iframe></div>",
        escape(caption),
        escape(src)
    )
}
