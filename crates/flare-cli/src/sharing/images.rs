//! The images a Markdown document shows, read from the document's folder
//! and checked before any request.
//!
//! The command repeats the server's cheap checks so that a document the
//! server would refuse stops before its images are sent: at most
//! [`MAX_IMAGES`] images, each path one the server accepts, each file at
//! most 5 MiB by its length on disk, and first bytes that name PNG, JPEG,
//! GIF or WebP. Only the server strips and decodes an image. An image
//! source that looks like a local file but that the renderer does not
//! treat as relative, such as a path with a space, is refused, because the
//! reading page would show it as its alternative text.

use crate::codes::{Failed, Refused, SharingFailed};
use flare_contract::{ApiCode, Format};
use flare_core::{ImageKind, ImagePath, MAX_IMAGES, SNIFF_BYTES};
use std::io::Read as _;
use std::path::Path;

/// The form part a [`LocalImage`] travels as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Part {
    /// An image a Markdown document shows.
    Image,
    /// The file of an HTML share.
    Html,
}

impl Part {
    /// The part's `name` in the form.
    pub(crate) const fn name(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Html => "html",
        }
    }

    /// The part's `Content-Type`.
    pub(crate) const fn content_type(self) -> &'static str {
        match self {
            Self::Image => "application/octet-stream",
            Self::Html => "text/html",
        }
    }
}

/// One file that travels in a form with its share: an image a document
/// shows, named by its path as the Markdown wrote it, or the HTML file of an
/// HTML share, named by its file name.
pub(crate) struct LocalImage {
    pub(crate) part: Part,
    pub(crate) path: String,
    pub(crate) bytes: Vec<u8>,
}

/// The most bytes of one image that `flare share` sends. The server
/// refuses a larger image with `flare::images::too_large`.
const MAX_IMAGE: u64 = 5 * 1024 * 1024;

/// The refusal `code` that the server would send for `image`.
fn refused(code: ApiCode, image: &str, problem: &str) -> Failed {
    Refused::new(code, Some(format!("the image {image} {problem}")), "").into()
}

/// The images that the Markdown `content` of `file` shows, read from the
/// file's folder and checked. HTML carries none.
pub(super) fn images(
    file: &Path,
    format: Format,
    content: &str,
) -> Result<Vec<LocalImage>, Failed> {
    if format != Format::Markdown {
        return Ok(Vec::new());
    }
    if let Some(image) = flare_render::unsendable_image_paths(content).first() {
        return Err(refused(
            ApiCode::ImageBadPath,
            image,
            "is not a relative path the reading page shows; rename the file without spaces or backslashes",
        ));
    }
    let paths = flare_render::image_paths(content);
    if paths.len() > MAX_IMAGES {
        return Err(Refused::new(
            ApiCode::TooManyImages,
            Some(format!("the document shows {} images", paths.len())),
            "",
        )
        .into());
    }
    let unreadable = |path: &Path, error: std::io::Error| -> Failed {
        SharingFailed::FileUnreadable {
            path: path.display().to_string(),
            problem: error.to_string(),
        }
        .into()
    };
    let folder = file
        .parent()
        .filter(|folder| !folder.as_os_str().is_empty());
    let folder = folder.unwrap_or_else(|| Path::new("."));
    let root = folder
        .canonicalize()
        .map_err(|error| unreadable(folder, error))?;
    let mut images = Vec::with_capacity(paths.len());
    for path in paths {
        let joined = folder.join(&path);
        let real = match joined.canonicalize() {
            Ok(real) => real,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err(SharingFailed::ImageMissing { image: path }.into());
            }
            Err(error) => return Err(unreadable(&joined, error)),
        };
        if !real.starts_with(&root) {
            return Err(SharingFailed::ImageOutsideFolder { image: path }.into());
        }
        if ImagePath::try_from(path.as_str()).is_err() {
            return Err(refused(
                ApiCode::ImageBadPath,
                &path,
                "is not a plain relative path; write it with no ./, .. or leading /",
            ));
        }
        let length = std::fs::metadata(&real)
            .map_err(|error| unreadable(&joined, error))?
            .len();
        if length > MAX_IMAGE {
            return Err(refused(
                ApiCode::ImageTooLarge,
                &path,
                &format!("is {length} bytes"),
            ));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(&real)
            .and_then(|opened| opened.take(MAX_IMAGE).read_to_end(&mut bytes))
            .map_err(|error| unreadable(&joined, error))?;
        let head = bytes.get(..SNIFF_BYTES).unwrap_or(&bytes);
        if ImageKind::sniff(head).is_none() {
            return Err(refused(
                ApiCode::ImageFormatRefused,
                &path,
                "does not start as a PNG, JPEG, GIF or WebP file",
            ));
        }
        images.push(LocalImage {
            part: Part::Image,
            path,
            bytes,
        });
    }
    Ok(images)
}
