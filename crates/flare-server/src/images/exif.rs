//! The one EXIF value an image keeps: its orientation.
//!
//! [`orientation`] reads the Orientation tag (0x0112) from the first IFD
//! of a TIFF-structured EXIF block, and [`minimal`] writes a new block
//! that holds only that tag. No byte of the original block is copied, so
//! every other field, GPS included, is gone.
//!
//! Shortcut: the reader follows only IFD0 and reads only a SHORT value
//! held inline, which is how every camera writes Orientation. A block that
//! stores it any other way reads as having none, and the photo shows
//! unrotated. A full EXIF crate would read more, and needs its own eval.

/// The Orientation tag.
const ORIENTATION: u16 = 0x0112;

/// The TIFF type SHORT.
const SHORT: u16 = 3;

/// The two bytes at `at`, in the block's byte order.
fn u16_at(tiff: &[u8], at: usize, big: bool) -> Option<u16> {
    let bytes: [u8; 2] = tiff.get(at..at.checked_add(2)?)?.try_into().ok()?;
    Some(if big {
        u16::from_be_bytes(bytes)
    } else {
        u16::from_le_bytes(bytes)
    })
}

/// The four bytes at `at`, in the block's byte order.
fn u32_at(tiff: &[u8], at: usize, big: bool) -> Option<u32> {
    let bytes: [u8; 4] = tiff.get(at..at.checked_add(4)?)?.try_into().ok()?;
    Some(if big {
        u32::from_be_bytes(bytes)
    } else {
        u32::from_le_bytes(bytes)
    })
}

/// The orientation, 1 to 8, that the EXIF block `tiff` holds, or `None`
/// when it holds none or an invalid one.
pub(super) fn orientation(tiff: &[u8]) -> Option<u16> {
    let big = match tiff.get(..4)? {
        b"MM\0*" => true,
        b"II*\0" => false,
        _ => return None,
    };
    let ifd = usize::try_from(u32_at(tiff, 4, big)?).ok()?;
    let count = u16_at(tiff, ifd, big)?;
    (0..usize::from(count)).find_map(|index| {
        let entry = ifd.checked_add(2)?.checked_add(index.checked_mul(12)?)?;
        let tag = u16_at(tiff, entry, big)?;
        let kind = u16_at(tiff, entry.checked_add(2)?, big)?;
        let values = u32_at(tiff, entry.checked_add(4)?, big)?;
        if tag != ORIENTATION || kind != SHORT || values != 1 {
            return None;
        }
        u16_at(tiff, entry.checked_add(8)?, big).filter(|value| (1..=8).contains(value))
    })
}

/// A new big-endian EXIF block holding only the Orientation tag with
/// `value`: the TIFF header, one IFD with one entry, and no next IFD.
pub(super) fn minimal(value: u16) -> Vec<u8> {
    let mut tiff = Vec::with_capacity(26);
    tiff.extend_from_slice(b"MM\0*");
    tiff.extend_from_slice(&8_u32.to_be_bytes());
    tiff.extend_from_slice(&1_u16.to_be_bytes());
    tiff.extend_from_slice(&ORIENTATION.to_be_bytes());
    tiff.extend_from_slice(&SHORT.to_be_bytes());
    tiff.extend_from_slice(&1_u32.to_be_bytes());
    tiff.extend_from_slice(&value.to_be_bytes());
    tiff.extend_from_slice(&[0, 0]);
    tiff.extend_from_slice(&0_u32.to_be_bytes());
    tiff
}
