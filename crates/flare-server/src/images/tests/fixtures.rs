//! Images built in code, with the metadata a camera or an editor writes
//! put in by hand. Each piece of metadata holds a marker word, so a test
//! can show the word is gone.

use image::codecs::gif::{GifEncoder, Repeat};
use image::codecs::jpeg::JpegEncoder;
use image::codecs::png::PngEncoder;
use image::codecs::webp::WebPEncoder;
use image::{Delay, Frame, ImageEncoder, RgbImage, RgbaImage};

/// The words that the metadata holds.
pub(super) const GPS_WORD: &[u8] = b"GPSMARKER";
pub(super) const DATE_WORD: &[u8] = b"2026:11:24 10:00:00";
pub(super) const CAMERA_WORD: &[u8] = b"CAMERAMAKE";
pub(super) const XMP_WORD: &[u8] = b"XMPMARKER";
pub(super) const COMMENT_WORD: &[u8] = b"COMMENTMARKER";
pub(super) const IPTC_WORD: &[u8] = b"IPTCMARKER";
pub(super) const MPF_WORD: &[u8] = b"MPFMARKER";
pub(super) const TRAILER_WORD: &[u8] = b"TRAILMARKER";
pub(super) const TEXT_WORD: &[u8] = b"TEXTMARKER";
pub(super) const ICC: &[u8] = b"ICCPROFILEBYTES-0123456789";

/// The pixels every fixture holds: 16 by 12, a gradient.
pub(super) fn pixels() -> RgbImage {
    RgbImage::from_fn(16, 12, |x, y| {
        image::Rgb([
            u8::try_from(x * 15).unwrap(),
            u8::try_from(y * 20).unwrap(),
            128,
        ])
    })
}

/// Whether `haystack` holds `needle`.
pub(super) fn holds(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// An EXIF block, little-endian, with the camera's make, the date, an
/// orientation when given, and a GPS IFD holding [`GPS_WORD`].
pub(super) fn camera_tiff(orientation: Option<u16>) -> Vec<u8> {
    let mut entries: Vec<(u16, u16, u32, [u8; 4])> = Vec::new();
    let count = 3 + u16::from(orientation.is_some());
    let ifd_end = 8 + 2 + 12 * u32::from(count) + 4;
    let make_at = ifd_end;
    let make = [CAMERA_WORD, b"\0"].concat();
    let date_at = make_at + u32::try_from(make.len()).unwrap();
    let date = [DATE_WORD, b"\0"].concat();
    let gps_at = date_at + u32::try_from(date.len()).unwrap();
    entries.push((
        0x010F,
        2,
        u32::try_from(make.len()).unwrap(),
        make_at.to_le_bytes(),
    ));
    if let Some(value) = orientation {
        let [low, high] = value.to_le_bytes();
        entries.push((0x0112, 3, 1, [low, high, 0, 0]));
    }
    entries.push((
        0x0132,
        2,
        u32::try_from(date.len()).unwrap(),
        date_at.to_le_bytes(),
    ));
    entries.push((0x8825, 4, 1, gps_at.to_le_bytes()));
    let mut tiff = b"II*\0".to_vec();
    tiff.extend_from_slice(&8_u32.to_le_bytes());
    tiff.extend_from_slice(&count.to_le_bytes());
    for (tag, kind, values, value) in entries {
        tiff.extend_from_slice(&tag.to_le_bytes());
        tiff.extend_from_slice(&kind.to_le_bytes());
        tiff.extend_from_slice(&values.to_le_bytes());
        tiff.extend_from_slice(&value);
    }
    tiff.extend_from_slice(&0_u32.to_le_bytes());
    tiff.extend_from_slice(&make);
    tiff.extend_from_slice(&date);
    // The GPS IFD: one ASCII entry, GPSMapDatum, holding the word.
    let datum = [GPS_WORD, b"\0"].concat();
    let datum_at = gps_at + 2 + 12 + 4;
    tiff.extend_from_slice(&1_u16.to_le_bytes());
    tiff.extend_from_slice(&0x0012_u16.to_le_bytes());
    tiff.extend_from_slice(&2_u16.to_le_bytes());
    tiff.extend_from_slice(&u32::try_from(datum.len()).unwrap().to_le_bytes());
    tiff.extend_from_slice(&datum_at.to_le_bytes());
    tiff.extend_from_slice(&0_u32.to_le_bytes());
    tiff.extend_from_slice(&datum);
    tiff
}

/// A JPEG segment: its marker, length and contents.
pub(super) fn segment(marker: u8, contents: &[u8]) -> Vec<u8> {
    let length = u16::try_from(contents.len() + 2).unwrap();
    [&[0xFF, marker][..], &length.to_be_bytes(), contents].concat()
}

/// The pixels as a plain baseline JPEG.
pub(super) fn plain_jpeg() -> Vec<u8> {
    let mut bytes = Vec::new();
    let image = pixels();
    JpegEncoder::new_with_quality(&mut bytes, 90)
        .write_image(image.as_raw(), 16, 12, image::ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

/// A camera's JPEG: EXIF with GPS and an orientation when given, XMP, an
/// ICC profile, IPTC, a comment and an MPF segment after SOI.
pub(super) fn camera_jpeg(orientation: Option<u16>) -> Vec<u8> {
    let plain = plain_jpeg();
    let exif = [b"Exif\0\0".as_slice(), &camera_tiff(orientation)].concat();
    let xmp = [b"http://ns.adobe.com/xap/1.0/\0".as_slice(), XMP_WORD].concat();
    let icc = [b"ICC_PROFILE\0\x01\x01".as_slice(), ICC].concat();
    let iptc = [b"Photoshop 3.0\0".as_slice(), IPTC_WORD].concat();
    let mpf = [b"MPF\0".as_slice(), MPF_WORD].concat();
    let metadata = [
        segment(0xE1, &exif),
        segment(0xE1, &xmp),
        segment(0xE2, &icc),
        segment(0xED, &iptc),
        segment(0xFE, COMMENT_WORD),
        segment(0xE2, &mpf),
    ]
    .concat();
    [&plain[..2], &metadata, &plain[2..]].concat()
}

/// The bytes from the first scan header to the end of `jpeg`'s first
/// image.
pub(super) fn scan_of(jpeg: &[u8]) -> Vec<u8> {
    let start = jpeg
        .windows(2)
        .position(|pair| pair == [0xFF, 0xDA])
        .unwrap();
    let end = jpeg
        .windows(2)
        .position(|pair| pair == [0xFF, 0xD9])
        .unwrap();
    jpeg[start..end + 2].to_vec()
}

/// CRC-32 as PNG computes it.
fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xFFFF_FFFF_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xEDB8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

/// A PNG chunk with its length and CRC.
pub(super) fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let crc = crc32(&[kind.as_slice(), data].concat());
    [
        &u32::try_from(data.len()).unwrap().to_be_bytes()[..],
        kind,
        data,
        &crc.to_be_bytes(),
    ]
    .concat()
}

/// `data` as a zlib stream of one stored block.
pub(super) fn zlib_stored(data: &[u8]) -> Vec<u8> {
    let length = u16::try_from(data.len()).unwrap();
    let (mut a, mut b) = (1_u32, 0_u32);
    for byte in data {
        a = (a + u32::from(*byte)) % 65_521;
        b = (b + a) % 65_521;
    }
    [
        &[0x78, 0x01, 0x01][..],
        &length.to_le_bytes(),
        &(!length).to_le_bytes(),
        data,
        &((b << 16) | a).to_be_bytes(),
    ]
    .concat()
}

/// The pixels as a plain PNG.
pub(super) fn plain_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    let image = pixels();
    PngEncoder::new(&mut bytes)
        .write_image(image.as_raw(), 16, 12, image::ExtendedColorType::Rgb8)
        .unwrap();
    bytes
}

/// The chunk `kind`'s data in `png`, if it holds one.
pub(super) fn png_chunk(png: &[u8], kind: &[u8; 4]) -> Option<Vec<u8>> {
    let mut at = 8;
    while at + 8 <= png.len() {
        let length =
            usize::try_from(u32::from_be_bytes(png[at..at + 4].try_into().unwrap())).unwrap();
        if &png[at + 4..at + 8] == kind {
            return Some(png[at + 8..at + 8 + length].to_vec());
        }
        at += 12 + length;
    }
    None
}

/// An edited PNG: EXIF with GPS and an orientation when given, text, XMP,
/// a time and an ICC profile after IHDR, and bytes after IEND.
pub(super) fn edited_png(orientation: Option<u16>) -> Vec<u8> {
    let plain = plain_png();
    let ihdr_end = 8 + 25;
    let metadata = [
        chunk(b"eXIf", &camera_tiff(orientation)),
        chunk(
            b"iCCP",
            &[b"icc\0\0".as_slice(), &zlib_stored(ICC)].concat(),
        ),
        chunk(b"tEXt", &[b"Comment\0".as_slice(), TEXT_WORD].concat()),
        chunk(
            b"zTXt",
            &[b"Note\0\0".as_slice(), &zlib_stored(COMMENT_WORD)].concat(),
        ),
        chunk(
            b"iTXt",
            &[b"XML:com.adobe.xmp\0\0\0\0\0".as_slice(), XMP_WORD].concat(),
        ),
        chunk(b"tIME", &[0x07, 0xEA, 11, 24, 10, 0, 0]),
    ]
    .concat();
    [
        &plain[..ihdr_end],
        &metadata,
        &plain[ihdr_end..],
        TRAILER_WORD,
    ]
    .concat()
}

/// A RIFF chunk with its padding.
fn riff_chunk(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut bytes = [
        id.as_slice(),
        &u32::try_from(data.len()).unwrap().to_le_bytes(),
        data,
    ]
    .concat();
    if data.len() % 2 == 1 {
        bytes.push(0);
    }
    bytes
}

/// The pixels as a lossless WebP, and its VP8L chunk's data.
pub(super) fn plain_webp() -> (Vec<u8>, Vec<u8>) {
    let mut bytes = Vec::new();
    let image = RgbaImage::from_fn(16, 12, |x, y| {
        let pixel = pixels().get_pixel(x, y).0;
        image::Rgba([pixel[0], pixel[1], pixel[2], 255])
    });
    WebPEncoder::new_lossless(&mut bytes)
        .encode(image.as_raw(), 16, 12, image::ExtendedColorType::Rgba8)
        .unwrap();
    let length = usize::try_from(u32::from_le_bytes(bytes[16..20].try_into().unwrap())).unwrap();
    assert_eq!(&bytes[12..16], b"VP8L");
    let vp8l = bytes[20..20 + length].to_vec();
    (bytes, vp8l)
}

/// An edited WebP: VP8X with the ICC, EXIF and XMP flags, an ICC profile,
/// the image, EXIF with GPS and an orientation when given, and XMP.
pub(super) fn edited_webp(orientation: Option<u16>) -> Vec<u8> {
    let (_, vp8l) = plain_webp();
    let mut vp8x = vec![0b0010_1100, 0, 0, 0];
    vp8x.extend_from_slice(&15_u32.to_le_bytes()[..3]);
    vp8x.extend_from_slice(&11_u32.to_le_bytes()[..3]);
    let body = [
        riff_chunk(b"VP8X", &vp8x),
        riff_chunk(b"ICCP", ICC),
        riff_chunk(b"VP8L", &vp8l),
        riff_chunk(b"EXIF", &camera_tiff(orientation)),
        riff_chunk(b"XMP ", XMP_WORD),
    ]
    .concat();
    let size = u32::try_from(body.len() + 4).unwrap();
    [
        b"RIFF".as_slice(),
        &size.to_le_bytes(),
        b"WEBP",
        &body,
        TRAILER_WORD,
    ]
    .concat()
}

/// The chunks of a WebP, as ids and data.
pub(super) fn webp_chunks(webp: &[u8]) -> Vec<([u8; 4], Vec<u8>)> {
    let mut chunks = Vec::new();
    let mut at = 12;
    while at + 8 <= webp.len() {
        let id: [u8; 4] = webp[at..at + 4].try_into().unwrap();
        let length =
            usize::try_from(u32::from_le_bytes(webp[at + 4..at + 8].try_into().unwrap())).unwrap();
        chunks.push((id, webp[at + 8..at + 8 + length].to_vec()));
        at += 8 + length + length % 2;
    }
    chunks
}

/// The pixels as a looping two-frame GIF.
pub(super) fn plain_gif() -> Vec<u8> {
    let mut bytes = Vec::new();
    {
        let mut encoder = GifEncoder::new(&mut bytes);
        encoder.set_repeat(Repeat::Infinite).unwrap();
        let frame = |shift: u8| {
            let image = RgbaImage::from_fn(16, 12, |x, y| {
                let pixel = pixels().get_pixel(x, y).0;
                image::Rgba([pixel[0].wrapping_add(shift), pixel[1], pixel[2], 255])
            });
            Frame::from_parts(image, 0, 0, Delay::from_numer_denom_ms(100, 1))
        };
        encoder.encode_frames([frame(0), frame(40)]).unwrap();
    }
    bytes
}

/// An edited GIF: a comment and an XMP application extension before the
/// trailer, and bytes after it.
pub(super) fn edited_gif() -> Vec<u8> {
    let plain = plain_gif();
    let (body, trailer) = plain.split_at(plain.len() - 1);
    assert_eq!(trailer, [0x3B]);
    let comment = [
        &[0x21, 0xFE, u8::try_from(COMMENT_WORD.len()).unwrap()][..],
        COMMENT_WORD,
        &[0],
    ]
    .concat();
    let xmp = [
        &[0x21, 0xFF, 11][..],
        b"XMP DataXMP",
        &[u8::try_from(XMP_WORD.len()).unwrap()],
        XMP_WORD,
        &[0],
    ]
    .concat();
    [body, &comment, &xmp, trailer, TRAILER_WORD].concat()
}
