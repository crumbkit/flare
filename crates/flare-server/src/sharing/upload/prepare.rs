//! The checks an upload's images pass before any store call, in order:
//! each image's size, their count, each path, each format by its first
//! bytes, and then each image stripped and decoded on a blocking thread,
//! one image at a time across the server.

use super::ImagePart;
use crate::images::{self, Format, MAX_IMAGE_BYTES, Refused};
use crate::sharing::Refusal;
use crate::tasks;
use flare_contract::ApiCode;
use flare_core::{ImagePath, MAX_IMAGES};
use tokio::sync::Semaphore;

/// One image ready to store: its path, format and stripped bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Prepared {
    pub(crate) path: ImagePath,
    pub(crate) format: Format,
    pub(crate) bytes: Vec<u8>,
}

/// Lets one image at a time be stripped and decoded, so a decode's
/// 256 MiB limit is the most the server spends on images at once.
static CLEANING: Semaphore = Semaphore::const_new(1);

/// The refusal `code` of the image at `index`, counted from 1.
fn refused(code: ApiCode, index: usize) -> Refusal {
    Refusal {
        code,
        detail: Some(format!("image {}", index + 1)),
        share: None,
    }
}

/// The refusal of an image that `images::clean` refused.
fn cleaning_refusal(refusal: Refused) -> ApiCode {
    match refusal {
        Refused::UnknownFormat => ApiCode::ImageFormatRefused,
        Refused::Unreadable => ApiCode::ImageUnreadable,
        Refused::NotAnImage => ApiCode::NotAnImage,
        Refused::TooManyPixels => ApiCode::TooManyPixels,
    }
}

/// The checks that need no decode: sizes, count, paths and formats. It
/// returns each part's path.
fn checked(markdown: &str, parts: &[ImagePart]) -> Result<Vec<ImagePath>, Refusal> {
    if let Some(index) = parts
        .iter()
        .position(|part| part.bytes.len() > MAX_IMAGE_BYTES)
    {
        return Err(refused(ApiCode::ImageTooLarge, index));
    }
    if parts.len() > MAX_IMAGES {
        return Err(Refusal {
            code: ApiCode::TooManyImages,
            detail: Some(format!("{} images", parts.len())),
            share: None,
        });
    }
    let shown = flare_render::image_paths(markdown);
    let mut paths: Vec<ImagePath> = Vec::with_capacity(parts.len());
    for (index, part) in parts.iter().enumerate() {
        let path = ImagePath::try_from(part.path.as_str())
            .map_err(|_| refused(ApiCode::ImageBadPath, index))?;
        if paths.contains(&path) {
            return Err(refused(ApiCode::ImageDuplicate, index));
        }
        if !shown.iter().any(|shown| shown == path.as_str()) {
            return Err(refused(ApiCode::ImageUnused, index));
        }
        paths.push(path);
    }
    if let Some(index) = parts
        .iter()
        .position(|part| Format::sniff(&part.bytes).is_none())
    {
        return Err(refused(ApiCode::ImageFormatRefused, index));
    }
    Ok(paths)
}

/// Checks every image of an upload whose Markdown is `markdown`, and
/// strips each.
///
/// # Errors
///
/// Returns the first refusal, in the order of the module's checks.
pub(crate) async fn prepare(
    markdown: &str,
    parts: Vec<ImagePart>,
) -> Result<Vec<Prepared>, Refusal> {
    let paths = checked(markdown, &parts)?;
    let mut prepared = Vec::with_capacity(parts.len());
    for (index, (path, part)) in paths.into_iter().zip(parts).enumerate() {
        let permit = CLEANING
            .acquire()
            .await
            .map_err(|_| Refusal::from(ApiCode::SharesUnavailable))?;
        let bytes = part.bytes;
        // The permit moves into the work, so a request dropped while it
        // waits still lets only one image be decoded at a time.
        let cleaned = tasks::blocking(move || {
            let cleaned = images::clean(&bytes);
            drop(permit);
            cleaned
        })
        .await
        .ok_or_else(|| Refusal::from(ApiCode::SharesUnavailable))?;
        let cleaned = cleaned.map_err(|refusal| refused(cleaning_refusal(refusal), index))?;
        prepared.push(Prepared {
            path,
            format: cleaned.format,
            bytes: cleaned.bytes,
        });
    }
    Ok(prepared)
}

#[cfg(test)]
pub(crate) mod tests;
