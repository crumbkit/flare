//! Strips a WebP by allowlist of chunk ids with img-parts, and writes the
//! VP8X flag byte itself, because img-parts refreshes only the ICC and
//! EXIF flags and would leave the XMP flag set after its chunk is gone.

use super::exif;
use img_parts::riff::RiffContent;
use img_parts::webp::{CHUNK_VP8X, WebP};
use img_parts::{Bytes, ImageEXIF};

/// The chunk ids a stripped WebP keeps.
const KEPT: [&[u8; 4]; 7] = [
    b"VP8 ", b"VP8L", b"VP8X", b"ALPH", b"ANIM", b"ANMF", b"ICCP",
];

/// The VP8X flag bits of an EXIF chunk and an XMP chunk.
const EXIF_FLAG: u8 = 0b0000_1000;
const XMP_FLAG: u8 = 0b0000_0100;

/// `bytes` with every chunk but the allowlist's dropped, a new EXIF chunk
/// holding only the orientation when the original held one, and the VP8X
/// flags saying so; `None` when it does not parse.
pub(super) fn strip(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut webp = WebP::from_bytes(Bytes::copy_from_slice(bytes)).ok()?;
    let orientation = webp.exif().and_then(|tiff| exif::orientation(&tiff));
    webp.chunks_mut()
        .retain(|chunk| KEPT.iter().any(|id| **id == chunk.id()));
    webp.set_exif(orientation.map(|value| Bytes::from(exif::minimal(value))));
    let exif_flag = if orientation.is_some() { EXIF_FLAG } else { 0 };
    for chunk in webp.chunks_mut() {
        if chunk.id() != CHUNK_VP8X {
            continue;
        }
        if let RiffContent::Data(data) = chunk.content_mut() {
            let mut flags = data.to_vec();
            if let Some(first) = flags.first_mut() {
                *first = (*first & !(EXIF_FLAG | XMP_FLAG)) | exif_flag;
            }
            *data = Bytes::from(flags);
        }
    }
    Some(webp.encoder().bytes().to_vec())
}
