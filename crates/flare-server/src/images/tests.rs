//! Unit tests of naming, stripping and decoding an uploaded image.

mod fixtures;

use super::exif::minimal;
use super::{Format, Refused, clean};
use fixtures::{
    CAMERA_WORD, COMMENT_WORD, DATE_WORD, GPS_WORD, ICC, IPTC_WORD, MPF_WORD, TEXT_WORD,
    TRAILER_WORD, XMP_WORD, camera_jpeg, chunk, edited_gif, edited_png, edited_webp, holds,
    plain_gif, plain_jpeg, plain_png, plain_webp, png_chunk, scan_of, segment, webp_chunks,
};
use image::codecs::gif::GifDecoder;
use image::codecs::png::PngEncoder;
use image::{AnimationDecoder, ImageEncoder, ImageFormat};
use std::io::Cursor;

/// Every word a metadata fixture holds that a cleaned image must not.
const DROPPED: [&[u8]; 9] = [
    GPS_WORD,
    DATE_WORD,
    CAMERA_WORD,
    XMP_WORD,
    COMMENT_WORD,
    IPTC_WORD,
    MPF_WORD,
    TRAILER_WORD,
    TEXT_WORD,
];

fn assert_dropped(bytes: &[u8]) {
    for word in DROPPED {
        assert!(!holds(bytes, word), "{}", String::from_utf8_lossy(word));
    }
}

/// The decoded pixels of `bytes`, as RGBA.
fn pixels(bytes: &[u8], format: ImageFormat) -> Vec<u8> {
    image::load_from_memory_with_format(bytes, format)
        .unwrap()
        .to_rgba8()
        .into_raw()
}

/// A JPEG APP1 segment that holds `tiff` as its EXIF.
fn exif_segment(tiff: &[u8]) -> Vec<u8> {
    segment(0xE1, &[b"Exif\0\0".as_slice(), tiff].concat())
}

#[test]
fn a_camera_jpeg_keeps_orientation_6_its_icc_profile_and_its_scan_byte_for_byte_and_loses_every_other_field()
 {
    let original = camera_jpeg(Some(6));
    let cleaned = clean(&original).unwrap();
    assert_eq!(cleaned.format, Format::Jpeg);
    assert_dropped(&cleaned.bytes);
    assert!(holds(&cleaned.bytes, &exif_segment(&minimal(6))));
    assert!(holds(&cleaned.bytes, ICC));
    assert_eq!(scan_of(&cleaned.bytes), scan_of(&original));
    assert_eq!(
        pixels(&cleaned.bytes, ImageFormat::Jpeg),
        pixels(&original, ImageFormat::Jpeg)
    );
}

#[test]
fn a_jpeg_with_no_orientation_keeps_no_exif_block() {
    let cleaned = clean(&camera_jpeg(None)).unwrap();
    assert_dropped(&cleaned.bytes);
    assert!(!holds(&cleaned.bytes, b"Exif\0\0"));
}

#[test]
fn a_jpeg_ends_at_its_first_end_marker_so_trailing_bytes_and_an_appended_jpeg_are_removed() {
    let plain = plain_jpeg();
    let followed = [plain.as_slice(), TRAILER_WORD, &camera_jpeg(Some(6))].concat();
    let cleaned = clean(&followed).unwrap();
    assert_dropped(&cleaned.bytes);
    assert!(cleaned.bytes.ends_with(&[0xFF, 0xD9]));
    assert_eq!(
        cleaned
            .bytes
            .windows(2)
            .filter(|pair| *pair == [0xFF, 0xD9])
            .count(),
        1
    );
}

#[test]
fn an_app1_segment_after_a_scan_is_removed_and_the_scan_stays() {
    let plain = plain_jpeg();
    let (body, end) = plain.split_at(plain.len() - 2);
    let gps = exif_segment(&fixtures::camera_tiff(Some(3)));
    let hidden = [body, &gps, &segment(0xFE, COMMENT_WORD), end].concat();
    let cleaned = clean(&hidden).unwrap();
    assert_dropped(&cleaned.bytes);
    assert_eq!(scan_of(&cleaned.bytes), scan_of(&plain));
}

#[test]
fn an_edited_png_keeps_orientation_and_its_icc_profile_and_loses_text_xmp_time_and_trailing_bytes()
{
    let original = edited_png(Some(6));
    let cleaned = clean(&original).unwrap();
    assert_eq!(cleaned.format, Format::Png);
    assert_dropped(&cleaned.bytes);
    assert_eq!(png_chunk(&cleaned.bytes, b"eXIf"), Some(minimal(6)));
    assert_eq!(
        png_chunk(&cleaned.bytes, b"iCCP"),
        png_chunk(&original, b"iCCP")
    );
    assert_eq!(png_chunk(&cleaned.bytes, b"tIME"), None);
    assert!(cleaned.bytes.ends_with(&chunk(b"IEND", &[])));
    assert_eq!(
        pixels(&cleaned.bytes, ImageFormat::Png),
        pixels(&plain_png(), ImageFormat::Png)
    );
}

#[test]
fn a_png_with_no_orientation_keeps_no_exif_chunk() {
    let cleaned = clean(&edited_png(None)).unwrap();
    assert_dropped(&cleaned.bytes);
    assert_eq!(png_chunk(&cleaned.bytes, b"eXIf"), None);
}

/// The VP8X flags byte of a WebP.
fn vp8x_flags(webp: &[u8]) -> u8 {
    let chunks = webp_chunks(webp);
    let (_, data) = chunks.iter().find(|(id, _)| id == b"VP8X").unwrap();
    data[0]
}

#[test]
fn an_edited_webp_keeps_orientation_and_its_icc_profile_and_its_flags_name_only_what_it_holds() {
    let original = edited_webp(Some(6));
    let cleaned = clean(&original).unwrap();
    assert_eq!(cleaned.format, Format::WebP);
    assert_dropped(&cleaned.bytes);
    let chunks = webp_chunks(&cleaned.bytes);
    let ids: Vec<&[u8; 4]> = chunks.iter().map(|(id, _)| id).collect();
    assert!(!ids.contains(&b"XMP "));
    let exif = chunks.iter().find(|(id, _)| id == b"EXIF").unwrap();
    assert_eq!(exif.1, minimal(6));
    let icc = chunks.iter().find(|(id, _)| id == b"ICCP").unwrap();
    assert_eq!(icc.1, ICC);
    assert_eq!(vp8x_flags(&cleaned.bytes), 0b0010_1000);
    assert_eq!(
        pixels(&cleaned.bytes, ImageFormat::WebP),
        pixels(&plain_webp().0, ImageFormat::WebP)
    );
}

#[test]
fn a_webp_with_no_orientation_has_no_exif_chunk_and_clears_its_exif_flag() {
    let cleaned = clean(&edited_webp(None)).unwrap();
    assert_dropped(&cleaned.bytes);
    assert!(
        webp_chunks(&cleaned.bytes)
            .iter()
            .all(|(id, _)| id != b"EXIF")
    );
    assert_eq!(vp8x_flags(&cleaned.bytes), 0b0010_0000);
}

/// The number of frames a GIF holds.
fn frames(gif: &[u8]) -> usize {
    GifDecoder::new(Cursor::new(gif))
        .unwrap()
        .into_frames()
        .collect_frames()
        .unwrap()
        .len()
}

#[test]
fn an_edited_gif_keeps_both_frames_and_its_loop_and_loses_its_comment_xmp_and_trailing_bytes() {
    let original = edited_gif();
    let cleaned = clean(&original).unwrap();
    assert_eq!(cleaned.format, Format::Gif);
    assert_dropped(&cleaned.bytes);
    assert!(holds(&cleaned.bytes, b"NETSCAPE2.0"));
    assert!(cleaned.bytes.ends_with(&[0x3B]));
    assert_eq!(frames(&cleaned.bytes), 2);
    assert_eq!(cleaned.bytes, plain_gif());
}

#[test]
fn an_svg_named_as_a_png_and_other_text_is_refused_and_each_admitted_format_is_named() {
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"><script>alert(1)</script></svg>";
    for refused in [&svg[..], b"", b"%PDF-1.7", b"BM\0\0", b"RIFF\0\0\0\0WAVE"] {
        assert_eq!(clean(refused), Err(Refused::UnknownFormat));
    }
    for (bytes, format) in [
        (plain_png(), Format::Png),
        (plain_jpeg(), Format::Jpeg),
        (plain_gif(), Format::Gif),
        (plain_webp().0, Format::WebP),
    ] {
        assert_eq!(clean(&bytes).unwrap().format, format);
    }
}

#[test]
fn a_file_whose_first_bytes_name_a_format_but_whose_body_is_not_that_format_is_refused() {
    let png = [b"\x89PNG\r\n\x1a\n".as_slice(), b"not a chunk at all"].concat();
    let jpeg = [&[0xFF, 0xD8, 0xFF, 0xE0][..], b"rubbish"].concat();
    let gif = b"GIF89a\x01".to_vec();
    let webp = [b"RIFF\x04\0\0\0WEBP".as_slice()].concat();
    for bytes in [png, jpeg, gif, webp] {
        assert!(
            matches!(
                clean(&bytes),
                Err(Refused::Unreadable | Refused::NotAnImage)
            ),
            "{bytes:?}"
        );
    }
}

/// A PNG `width` pixels wide and one high.
fn wide_png(width: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let row = vec![0_u8; usize::try_from(width).unwrap()];
    PngEncoder::new(&mut bytes)
        .write_image(&row, width, 1, image::ExtendedColorType::L8)
        .unwrap();
    bytes
}

#[test]
fn an_image_16385_pixels_wide_is_refused_and_one_16384_wide_is_admitted() {
    assert_eq!(clean(&wide_png(16_385)), Err(Refused::TooManyPixels));
    assert_eq!(clean(&wide_png(16_384)).unwrap().format, Format::Png);
}

#[test]
fn a_media_type_names_the_format_it_was_written_from() {
    for format in [Format::Png, Format::Jpeg, Format::Gif, Format::WebP] {
        assert_eq!(Format::from_media_type(format.media_type()), Some(format));
    }
    assert_eq!(Format::from_media_type("image/svg+xml"), None);
}
