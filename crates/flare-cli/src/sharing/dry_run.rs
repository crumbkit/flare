//! `flare share --dry-run`: checks a file with the checks that the server
//! applies to a create or a new version, renders it with flare-render, and
//! writes the HTML with `--out`. It makes no request and reads neither the
//! saved login nor the Keychain.

use super::outgoing::Outgoing;
use super::{LocalPart, MAX_FILE};
use crate::cli::ShareArgs;
use crate::codes::{Failed, Refused, SharingFailed};
use crate::documents::ShareOut;
use crumbkit_durable::{Mode, write_atomically};
use flare_core::{
    ContentFormat, MAX_DOCUMENT, MAX_HTML, MAX_TITLE, Page, PageContent, PageName, ShareId, Slots,
};
use flare_server::{BodyRefused, check_share, check_version};

/// The page name that the rendered HTML's `<title>` shows. flare-render
/// titles a document with its page's name, and a share has no page name,
/// so every preview carries this one. The command prints the share's own
/// title in its result.
const PREVIEW_NAME: &str = "share";

/// Checks and renders what `args` would send, and writes the HTML to
/// `--out` when it is given. For an HTML share, whose file `html` holds,
/// the file already passed the server's checks when it was read, and
/// `--out` writes it unchanged, as the server stores it.
///
/// # Errors
///
/// Returns the refusal the server would send, with its code, or the
/// failure to render or to write the HTML.
pub(super) fn check(
    args: &ShareArgs,
    outgoing: Outgoing,
    images: usize,
    html: Option<&LocalPart>,
) -> Result<ShareOut, Failed> {
    if let Some(file) = html {
        return check_html(args, outgoing, file);
    }
    let ending = args
        .file
        .extension()
        .map_or_else(String::new, |ending| ending.to_string_lossy().into_owned());
    let mut checks = vec![
        format!("the ending .{ending} names Markdown, which the server serves"),
        format!("the file is at most {MAX_FILE} bytes, the most flare share reads"),
    ];
    let (id, title, content) = match outgoing {
        Outgoing::Create(request) => {
            let (title, text) = check_share(&request).map_err(refused)?;
            checks.push(format!(
                "the title \"{}\" is 1 to {MAX_TITLE} bytes with no control character",
                title.as_str()
            ));
            for (named, what) in [
                (!request.with.is_empty(), "the addresses in --with"),
                (!request.re.is_empty(), "the flares in --re"),
            ] {
                if named {
                    checks.push(format!(
                        "not checked: {what}, which only the server resolves"
                    ));
                }
            }
            (None, Some(title.as_str().to_owned()), text)
        }
        Outgoing::Update(id, request) => {
            // The server parses the id before it checks the body, so a
            // malformed id answers not_found here as it does there.
            let id = ShareId::try_from(id.as_str())
                .map_err(|_| Refused::new(flare_contract::ApiCode::ShareNotFound, None, ""))?;
            let text = check_version(&request).map_err(refused)?;
            checks.push(format!(
                "not checked: whether you own the share {}, which only the server knows",
                id.as_str()
            ));
            (Some(id.as_str().to_owned()), None, text)
        }
    };
    checks.push(format!(
        "the content is {} bytes of text with no NUL, within the {MAX_DOCUMENT} a share holds",
        content.as_str().len()
    ));
    if images > 0 {
        checks.push(format!(
            "the {images} images are in the file's folder, each a plain relative path, at most 5 MiB and a PNG, JPEG, GIF or WebP file by its first bytes"
        ));
        checks.push(
            "not checked: whether each image decodes, which only the server checks".to_owned(),
        );
    }
    let html = render(content.as_str())?;
    checks.push(format!(
        "rendered to {} bytes of HTML with flare-render",
        html.len()
    ));
    if let Some(path) = &args.out {
        write_atomically(path, html.as_bytes(), Mode::Umask).map_err(|error| {
            SharingFailed::FileUnwritable {
                path: path.display().to_string(),
                problem: error.to_string(),
            }
        })?;
        checks.push(format!("wrote the HTML to {}", path.display()));
    }
    Ok(ShareOut {
        id,
        title,
        version: None,
        granted: None,
        responds_to: None,
        images,
        checks,
    })
}

/// The refusal the server would send for the same body.
fn refused(refused: BodyRefused) -> Failed {
    Refused::new(refused.code, refused.detail, "").into()
}

/// Renders `text` as a Markdown page with flare-render, the same function
/// that renders a published Markdown page.
fn render(text: &str) -> Result<String, Failed> {
    let invalid = |problem: String| Failed::PageInvalid { problem };
    let name = PageName::try_from(PREVIEW_NAME).map_err(|error| invalid(error.to_string()))?;
    let content = PageContent::new(ContentFormat::Markdown, text)
        .map_err(|error| invalid(error.to_string()))?;
    let slots = Slots::new(Vec::new()).map_err(|error| invalid(error.to_string()))?;
    let page = Page::new(name, content, slots);
    flare_render::render_page(&page, &[])
        .map(flare_render::PageDocument::into_string)
        .map_err(|error| invalid(error.to_string()))
}

/// The dry run of an HTML share: its file was checked as the server checks
/// it when the command read it, so this reports the checks and writes the
/// file with `--out`.
fn check_html(args: &ShareArgs, outgoing: Outgoing, file: &LocalPart) -> Result<ShareOut, Failed> {
    let (id, title) = match outgoing {
        Outgoing::Create(request) => (None, Some(request.title)),
        Outgoing::Update(id, _) => {
            let id = ShareId::try_from(id.as_str())
                .map_err(|_| Refused::new(flare_contract::ApiCode::ShareNotFound, None, ""))?;
            (Some(id.as_str().to_owned()), None)
        }
    };
    let mut checks = vec![
        "the ending names HTML, which the server serves from its content host".to_owned(),
        format!(
            "the file is {} bytes of UTF-8 text, within the {MAX_HTML} an HTML share holds",
            file.bytes.len()
        ),
        "not checked: whether the server has a content host, which only the server knows"
            .to_owned(),
    ];
    if let Some(path) = &args.out {
        write_atomically(path, &file.bytes, Mode::Umask).map_err(|error| {
            SharingFailed::FileUnwritable {
                path: path.display().to_string(),
                problem: error.to_string(),
            }
        })?;
        checks.push(format!("wrote the file to {}", path.display()));
    }
    Ok(ShareOut {
        id,
        title,
        version: None,
        granted: None,
        responds_to: None,
        images: 0,
        checks,
    })
}
