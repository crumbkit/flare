//! What `flare share` sends: the request that its file describes, and for an
//! HTML file the file itself as the part that carries it.

use super::{LocalPart, MAX_FILE, Part, format, read_bytes, read_file};
use crate::cli::ShareArgs;
use crate::codes::{Failed, Refused};
use flare_contract::{Format, ShareRequest, VersionRequest};
use flare_core::{MAX_HTML, ShareId};
use flare_server::{check_html_share, check_html_version};
use std::path::Path;

/// What `flare share` sends: a new share, or the next version of one.
pub(super) enum Outgoing {
    /// `POST /api/shares`.
    Create(ShareRequest),
    /// `POST /api/shares/{id}/versions`.
    Update(String, VersionRequest),
}

/// The request that `args` describes, read from its file, and for an HTML
/// file the file itself as the part that carries it. An HTML file is
/// checked here with the checks the server applies, so a dry run and a
/// real run refuse it alike, before any request.
pub(super) fn outgoing(args: &ShareArgs) -> Result<(Outgoing, Option<LocalPart>), Failed> {
    let format = format(&args.file)?;
    if format == Format::Html {
        return outgoing_html(args);
    }
    let content = read_file(&args.file, MAX_FILE)?;
    if let Some(id) = &args.update {
        return Ok((
            Outgoing::Update(id.clone(), VersionRequest { format, content }),
            None,
        ));
    }
    let title = args
        .title
        .clone()
        .unwrap_or_else(|| title_of(&args.file, &content));
    Ok((
        Outgoing::Create(ShareRequest {
            title,
            format,
            content,
            with: args.with.clone(),
            re: args.re.clone(),
        }),
        None,
    ))
}

/// The request for the HTML file of `args`: the share part names the format
/// and holds no content, and the file travels as the `html` part.
fn outgoing_html(args: &ShareArgs) -> Result<(Outgoing, Option<LocalPart>), Failed> {
    let bytes = read_bytes(&args.file, MAX_HTML)?;
    let name = args
        .file
        .file_name()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned());
    let refused = |refused: flare_server::BodyRefused| -> Failed {
        Refused::new(refused.code, refused.detail, "").into()
    };
    let outgoing = if let Some(id) = &args.update {
        let request = VersionRequest {
            format: Format::Html,
            content: String::new(),
        };
        // The server parses the id before it checks the body, so a
        // malformed id answers not_found here as it does there.
        if ShareId::try_from(id.as_str()).is_ok() {
            check_html_version(&request, &bytes).map_err(refused)?;
        }
        Outgoing::Update(id.clone(), request)
    } else {
        let request = ShareRequest {
            title: args
                .title
                .clone()
                .unwrap_or_else(|| title_of(&args.file, "")),
            format: Format::Html,
            content: String::new(),
            with: args.with.clone(),
            re: args.re.clone(),
        };
        check_html_share(&request, &bytes).map_err(refused)?;
        Outgoing::Create(request)
    };
    let file = LocalPart {
        part: Part::Html,
        path: name,
        bytes,
    };
    Ok((outgoing, Some(file)))
}

/// The title of a new share given no `--title`: the text of a Markdown
/// file's first level-one heading, or else the file name without its
/// ending. An HTML file passes no content and always takes its file name.
fn title_of(path: &Path, content: &str) -> String {
    flare_render::first_heading(content).unwrap_or_else(|| {
        path.file_stem()
            .map_or_else(String::new, |stem| stem.to_string_lossy().into_owned())
    })
}
