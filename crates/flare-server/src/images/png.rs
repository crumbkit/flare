//! Strips a PNG by allowlist of chunk types with img-parts. Every kept
//! chunk keeps its bytes and its CRC, and img-parts stops at IEND, so
//! nothing after it survives.

use super::exif;
use img_parts::png::Png;
use img_parts::{Bytes, ImageEXIF};

/// The chunk types a stripped PNG keeps.
const KEPT: [&[u8; 4]; 17] = [
    b"IHDR", b"PLTE", b"IDAT", b"IEND", b"tRNS", b"cHRM", b"gAMA", b"iCCP", b"sBIT", b"sRGB",
    b"cICP", b"mDCV", b"cLLI", b"pHYs", b"acTL", b"fcTL", b"fdAT",
];

/// `bytes` with every chunk but the allowlist's dropped and a new eXIf
/// chunk holding only the orientation when the original held one; `None`
/// when it does not parse.
pub(super) fn strip(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut png = Png::from_bytes(Bytes::copy_from_slice(bytes)).ok()?;
    let orientation = png.exif().and_then(|tiff| exif::orientation(&tiff));
    png.chunks_mut()
        .retain(|chunk| KEPT.iter().any(|kind| **kind == chunk.kind()));
    png.set_exif(orientation.map(|value| Bytes::from(exif::minimal(value))));
    Some(png.encoder().bytes().to_vec())
}
