//! Strips a GIF with a bounded walk over its blocks: img-parts has no GIF
//! module.
//!
//! The walk keeps the header, the logical screen descriptor, the colour
//! tables, each image's descriptor and data, graphic control extensions and
//! the NETSCAPE2.0 looping extension, and drops comment, plain text and
//! every other application extension, which is where XMP lives. It stops at
//! the trailer, so nothing after it survives. Each step reads a length the
//! file gives and checks it against the bytes left.

/// The block introducers and labels.
const IMAGE: u8 = 0x2C;
const EXTENSION: u8 = 0x21;
const TRAILER: u8 = 0x3B;
const GRAPHIC_CONTROL: u8 = 0xF9;
const APPLICATION: u8 = 0xFF;

/// The application identifier of the looping extension.
const NETSCAPE: &[u8] = b"NETSCAPE2.0";

/// A reader over the file that copies what the walk keeps.
struct Walk<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Walk<'a> {
    /// The next `count` bytes.
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(count)?;
        let taken = self.bytes.get(self.at..end)?;
        self.at = end;
        Some(taken)
    }

    /// The next byte.
    fn byte(&mut self) -> Option<u8> {
        self.take(1)?.first().copied()
    }

    /// A run of data sub-blocks, through its zero-length terminator.
    fn sub_blocks(&mut self) -> Option<&'a [u8]> {
        let start = self.at;
        loop {
            let length = self.byte()?;
            if length == 0 {
                return self.bytes.get(start..self.at);
            }
            self.take(usize::from(length))?;
        }
    }
}

/// The size of a colour table that a packed field announces, or zero.
fn table_size(packed: u8) -> usize {
    if packed & 0x80 == 0 {
        0
    } else {
        3_usize << ((packed & 0x07) + 1)
    }
}

/// `bytes` with every block but the allowlist's dropped and nothing after
/// the trailer; `None` when the walk cannot read it.
pub(super) fn strip(bytes: &[u8]) -> Option<Vec<u8>> {
    let mut walk = Walk { bytes, at: 0 };
    let mut kept = Vec::with_capacity(bytes.len());
    let header = walk.take(6)?;
    if header != b"GIF87a" && header != b"GIF89a" {
        return None;
    }
    kept.extend_from_slice(header);
    let screen = walk.take(7)?;
    kept.extend_from_slice(screen);
    kept.extend_from_slice(walk.take(table_size(*screen.get(4)?))?);
    loop {
        match walk.byte()? {
            TRAILER => {
                kept.push(TRAILER);
                return Some(kept);
            }
            IMAGE => {
                let descriptor = walk.take(9)?;
                kept.push(IMAGE);
                kept.extend_from_slice(descriptor);
                kept.extend_from_slice(walk.take(table_size(*descriptor.get(8)?))?);
                kept.push(walk.byte()?);
                kept.extend_from_slice(walk.sub_blocks()?);
            }
            EXTENSION => {
                let label = walk.byte()?;
                let start = walk.at;
                let blocks = walk.sub_blocks()?;
                let looping = label == APPLICATION
                    && bytes.get(start.checked_add(1)?..start.checked_add(12)?) == Some(NETSCAPE);
                if label == GRAPHIC_CONTROL || looping {
                    kept.extend_from_slice(&[EXTENSION, label]);
                    kept.extend_from_slice(blocks);
                }
            }
            _ => return None,
        }
    }
}
