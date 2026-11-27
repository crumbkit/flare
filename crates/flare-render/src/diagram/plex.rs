//! IBM Plex Sans advance widths, which limu measures a diagram's text with,
//! so a drawing's boxes fit the font flare-server serves.

use limu::scene::text::{Face, Measurer};

/// The advance widths of the regular face, in thousandths of the font size,
/// of the printable ASCII characters from the space (U+0020) to the tilde
/// (U+007E). /evals/flare-diagram-renderers measured them from the IBM Plex
/// Sans file that flare-server serves.
const REGULAR: [u16; 95] = [
    236, 284, 419, 713, 598, 927, 694, 242, 335, 335, 450, 600, 272, 399, 272, 383, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 292, 292, 600, 600, 600, 477, 891, 641, 653, 621, 671, 583,
    559, 695, 707, 400, 510, 634, 501, 812, 707, 708, 606, 708, 640, 581, 572, 678, 609, 891, 613,
    593, 580, 317, 383, 317, 600, 565, 600, 534, 580, 503, 580, 549, 324, 528, 568, 250, 250, 527,
    272, 873, 568, 560, 580, 580, 367, 487, 351, 568, 492, 768, 507, 499, 464, 343, 314, 343, 600,
];

/// The advance widths of the bold face, as [`REGULAR`] gives the regular
/// face's.
const BOLD: [u16; 95] = [
    236, 320, 493, 632, 601, 974, 721, 268, 338, 338, 601, 600, 310, 403, 310, 460, 600, 600, 600,
    600, 600, 600, 600, 600, 600, 600, 330, 330, 600, 600, 600, 500, 903, 685, 667, 651, 697, 607,
    585, 719, 724, 432, 559, 696, 530, 819, 724, 714, 656, 714, 674, 624, 584, 694, 650, 973, 673,
    649, 607, 334, 460, 334, 600, 556, 600, 569, 608, 517, 608, 562, 361, 552, 596, 286, 286, 577,
    303, 894, 596, 564, 608, 608, 404, 504, 383, 596, 538, 841, 560, 534, 518, 372, 402, 372, 600,
];

/// The width of a character outside printable ASCII in the regular face:
/// the average advance width of the font's other characters.
const REGULAR_OTHER: u16 = 549;

/// The width of a character outside printable ASCII in the bold face.
const BOLD_OTHER: u16 = 562;

/// Measures text as IBM Plex Sans, regular or bold.
///
/// This table is smaller than the font: it gives each printable ASCII
/// character its own width, gives every other character one average width,
/// and measures an italic or monospace span as its upright face. It applies
/// no kerning. Text outside ASCII, such as accented or CJK labels, is
/// measured roughly, so its box can be a little too wide or too narrow.
/// Reading the advance widths from the font's hmtx table would replace it.
#[derive(Debug, Clone, Copy)]
pub(super) struct Plex;

impl Measurer for Plex {
    fn width(&self, text: &str, size: f64, face: Face) -> f64 {
        let (table, other) = if face.contains(Face::BOLD) {
            (&BOLD, BOLD_OTHER)
        } else {
            (&REGULAR, REGULAR_OTHER)
        };
        let thousandths: u32 = text
            .chars()
            .map(|character| advance(table, other, character))
            .map(u32::from)
            .sum();
        f64::from(thousandths) * size / 1000.0
    }
}

/// The advance width of `character` in `table`: none for a control
/// character, its own for printable ASCII, and `other` for the rest.
fn advance(table: &[u16; 95], other: u16, character: char) -> u16 {
    let code = u32::from(character);
    if code < 0x20 {
        return 0;
    }
    code.checked_sub(0x20)
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| table.get(index))
        .copied()
        .unwrap_or(other)
}
