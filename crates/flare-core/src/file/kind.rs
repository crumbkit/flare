//! [`ImageKind`]: the image formats a share may carry, named by their
//! first bytes alone.

/// An image format a share may carry, which the first bytes of a file
/// name. Both the server and the `flare` command name a file's format this
/// way, and neither reads a file name or a declared type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ImageKind {
    /// PNG: `89 50 4E 47 0D 0A 1A 0A`.
    Png,
    /// JPEG: `FF D8 FF`.
    Jpeg,
    /// GIF: `GIF87a` or `GIF89a`.
    Gif,
    /// WebP: `RIFF`, a length and `WEBP`.
    WebP,
}

/// How many first bytes [`ImageKind::sniff`] reads at most.
pub const SNIFF_BYTES: usize = 12;

impl ImageKind {
    /// The format that the first bytes of `bytes` name, if any. It reads at
    /// most [`SNIFF_BYTES`] bytes. SVG and every other format name none.
    #[must_use]
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            Some(Self::Png)
        } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
            Some(Self::Jpeg)
        } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
            Some(Self::Gif)
        } else if bytes.starts_with(b"RIFF")
            && bytes.get(8..SNIFF_BYTES) == Some(b"WEBP".as_slice())
        {
            Some(Self::WebP)
        } else {
            None
        }
    }
}
