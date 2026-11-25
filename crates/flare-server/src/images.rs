//! Each uploaded image proved to be a PNG, JPEG, GIF or WebP and stripped
//! of its metadata before it is stored.
//!
//! [`clean`] runs three steps in order. It names the format from the first
//! bytes alone, never from a file name or a declared type, so anything
//! else, SVG included, is refused. It strips the container by allowlist,
//! keeping the compressed pixels byte for byte, the ICC profile, and a new
//! EXIF block that holds only the orientation; every other EXIF field, GPS
//! included, XMP, comments and bytes after the end marker are dropped. It
//! then decodes the stripped bytes with the `image` crate, forcing the
//! format the first bytes named, under limits of 16,384 pixels a side, 40
//! million pixels in all and 256 MiB of allocation, so a file that is not
//! really an image, or that would take too much memory, is refused before
//! anything is stored.

mod exif;
mod gif;
mod jpeg;
mod png;
mod webp;

use flare_core::ImageKind;
use image::{DynamicImage, ImageDecoder as _, ImageError, ImageFormat, ImageReader, Limits};
use std::io::Cursor;

/// The largest image, in bytes: 5 MiB.
pub(crate) const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;

/// The most pixels on one side of an image.
const MAX_SIDE: u32 = 16_384;

/// The most pixels an image may hold: 40 million, which a 40-megapixel
/// phone photo fits. The `image` crate's allocation limit counts only the
/// decoded buffer, and a progressive JPEG decode also allocates a buffer of
/// coefficients twice that size, so this limit keeps one decode under
/// about 400 MiB.
const MAX_PIXELS: u64 = 40_000_000;

/// Whether an image of `width` by `height` pixels is within [`MAX_PIXELS`].
fn within_pixels(width: u32, height: u32) -> bool {
    u64::from(width) * u64::from(height) <= MAX_PIXELS
}

/// The most bytes a decode may allocate: 256 MiB.
const MAX_ALLOC: u64 = 256 * 1024 * 1024;

/// The image formats a share may carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl Format {
    /// The format that the first bytes of `bytes` name, if any, by
    /// flare-core's [`ImageKind::sniff`].
    pub(crate) fn sniff(bytes: &[u8]) -> Option<Self> {
        Some(match ImageKind::sniff(bytes)? {
            ImageKind::Png => Self::Png,
            ImageKind::Jpeg => Self::Jpeg,
            ImageKind::Gif => Self::Gif,
            ImageKind::WebP => Self::WebP,
            _ => return None,
        })
    }

    /// The media type a stored file of this format has.
    pub(crate) fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }

    /// The media type of a stored file, by its text, as a reply's
    /// `Content-Type` holds it.
    pub(crate) fn from_media_type(text: &str) -> Option<Self> {
        [Self::Png, Self::Jpeg, Self::Gif, Self::WebP]
            .into_iter()
            .find(|format| format.media_type() == text)
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Png => ImageFormat::Png,
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Gif => ImageFormat::Gif,
            Self::WebP => ImageFormat::WebP,
        }
    }
}

/// Why an image is refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Refused {
    /// The first bytes name no format a share may carry.
    UnknownFormat,
    /// The container does not parse.
    Unreadable,
    /// The stripped bytes do not decode as their format.
    NotAnImage,
    /// The decode would pass the pixel or allocation limits.
    TooManyPixels,
}

/// An image ready to store: its format and its stripped bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Cleaned {
    pub(crate) format: Format,
    pub(crate) bytes: Vec<u8>,
}

/// The stripped bytes of `bytes` in `format`.
fn strip(format: Format, bytes: &[u8]) -> Result<Vec<u8>, Refused> {
    let stripped = match format {
        Format::Png => png::strip(bytes),
        Format::Jpeg => jpeg::strip(bytes),
        Format::Gif => gif::strip(bytes),
        Format::WebP => webp::strip(bytes),
    };
    stripped.ok_or(Refused::Unreadable)
}

/// Decodes `bytes` as `format` under the limits, and drops the pixels.
/// The header is read first, and an image over [`MAX_PIXELS`] is refused
/// before the decode allocates anything for its pixels.
fn decode(format: Format, bytes: &[u8]) -> Result<(), Refused> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), format.image_format());
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_ALLOC);
    reader.limits(limits);
    let refused = |error: ImageError| match error {
        ImageError::Limits(_) => Refused::TooManyPixels,
        _ => Refused::NotAnImage,
    };
    let decoder = reader.into_decoder().map_err(refused)?;
    let (width, height) = decoder.dimensions();
    if !within_pixels(width, height) {
        return Err(Refused::TooManyPixels);
    }
    DynamicImage::from_decoder(decoder)
        .map(drop)
        .map_err(refused)
}

/// Names, strips and decodes one uploaded image. It runs on the calling
/// thread and may take a second on a large image, so the server calls it
/// on a blocking thread, one image at a time.
pub(crate) fn clean(bytes: &[u8]) -> Result<Cleaned, Refused> {
    let format = Format::sniff(bytes).ok_or(Refused::UnknownFormat)?;
    let stripped = strip(format, bytes)?;
    decode(format, &stripped)?;
    Ok(Cleaned {
        format,
        bytes: stripped,
    })
}

#[cfg(test)]
mod tests;
