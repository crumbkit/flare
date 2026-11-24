//! Strips a JPEG by allowlist with img-parts, keeping the compressed scan
//! data byte for byte.
//!
//! img-parts parses the segments up to the first scan and keeps every byte
//! after the scan header as that segment's data, so this module walks the
//! scan data itself: it keeps scan bytes, restart markers and the tables
//! and scan headers of a progressive image, drops APPn and COM segments
//! between scans, and cuts the file after the first image's EOI, which
//! drops an appended MPF image and anything else after it.

use super::exif;
use img_parts::jpeg::{Jpeg, JpegSegment, markers};
use img_parts::{Bytes, ImageEXIF};

/// The prefix of an APP2 segment that holds an ICC profile.
const ICC_PROFILE: &[u8] = b"ICC_PROFILE\0";

/// Whether a marker is followed by a two-byte length: every marker but
/// the restart markers, SOI, EOI and TEM.
fn has_length(marker: u8) -> bool {
    !matches!(
        marker,
        markers::RST0..=markers::RST7 | markers::SOI | markers::EOI | 0x01
    )
}

/// Whether a segment before the scan is kept.
fn kept(segment: &JpegSegment) -> bool {
    match segment.marker() {
        markers::APP0 | markers::APP14 | markers::DQT | markers::DRI | markers::DNL => true,
        markers::APP2 => segment.contents().starts_with(ICC_PROFILE),
        marker @ markers::SOF0..=markers::SOF15 => marker != markers::JPG,
        _ => false,
    }
}

/// The scan data after the first scan header, through the first image's
/// EOI, with APPn and COM segments dropped; `None` when there is no EOI or
/// a marker the walk cannot read.
fn first_image(scan: &[u8]) -> Option<Vec<u8>> {
    let mut kept = Vec::with_capacity(scan.len());
    let mut at = 0;
    loop {
        let rest = scan.get(at..)?;
        let mark = rest.iter().position(|byte| *byte == markers::P)?;
        kept.extend_from_slice(rest.get(..mark)?);
        let at_mark = at.checked_add(mark)?;
        let code = *scan.get(at_mark.checked_add(1)?)?;
        match code {
            markers::Z | markers::RST0..=markers::RST7 => {
                kept.extend_from_slice(&[markers::P, code]);
                at = at_mark.checked_add(2)?;
            }
            // A fill byte before a marker.
            markers::P => at = at_mark.checked_add(1)?,
            markers::EOI => {
                kept.extend_from_slice(&[markers::P, markers::EOI]);
                return Some(kept);
            }
            _ if has_length(code) => {
                let length = scan.get(at_mark.checked_add(2)?..at_mark.checked_add(4)?)?;
                let length = usize::from(u16::from_be_bytes([*length.first()?, *length.get(1)?]));
                let end = at_mark.checked_add(2)?.checked_add(length)?;
                let segment = scan.get(at_mark..end)?;
                let dropped =
                    (markers::APP0..=markers::APP15).contains(&code) || code == markers::COM;
                if !dropped {
                    kept.extend_from_slice(segment);
                }
                at = end;
            }
            _ => return None,
        }
    }
}

/// `bytes` with every segment but the allowlist's dropped, a new EXIF
/// block holding only the orientation when the original held one, and
/// nothing after the first image's EOI; `None` when it does not parse.
pub(super) fn strip(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut jpeg = Jpeg::from_bytes(Bytes::copy_from_slice(bytes)).ok()?;
    let orientation = jpeg.exif().and_then(|tiff| exif::orientation(&tiff));
    let segments = jpeg.segments_mut();
    let scan_at = segments.iter().position(JpegSegment::has_entropy)?;
    let scan = segments.get(scan_at)?.clone();
    let header = scan.len();
    let whole = scan.clone().encoder().bytes();
    let cut = first_image(whole.get(header..)?)?;
    let rebuilt =
        JpegSegment::new_with_entropy(scan.marker(), scan.contents().clone(), Bytes::from(cut));
    *segments.get_mut(scan_at)? = rebuilt;
    segments.retain(|segment| segment.has_entropy() || kept(segment));
    jpeg.set_exif(orientation.map(|value| Bytes::from(exif::minimal(value))));
    Some(jpeg.encoder().bytes().to_vec())
}
