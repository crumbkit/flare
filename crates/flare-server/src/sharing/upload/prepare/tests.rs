//! Unit tests of the checks on an upload's images: each refusal, and an
//! admitted twin that differs only in what the check reads.

use super::super::ImagePart;
use super::{Prepared, prepare};
use crate::images::{Format, MAX_IMAGE_BYTES};
use flare_contract::ApiCode;
use hyper::body::Bytes;
use image::ImageEncoder;
use image::codecs::png::PngEncoder;

/// A grey PNG `width` pixels wide and one high.
pub(crate) fn png(width: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    let row = vec![128_u8; usize::try_from(width).unwrap()];
    PngEncoder::new(&mut bytes)
        .write_image(&row, width, 1, image::ExtendedColorType::L8)
        .unwrap();
    bytes
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
fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let crc = crc32(&[kind.as_slice(), data].concat());
    [
        &u32::try_from(data.len()).unwrap().to_be_bytes()[..],
        kind,
        data,
        &crc.to_be_bytes(),
    ]
    .concat()
}

fn part(path: &str, bytes: Vec<u8>) -> ImagePart {
    ImagePart {
        path: path.to_owned(),
        bytes: Bytes::from(bytes),
    }
}

/// A document that shows each of `paths`.
fn showing(paths: &[&str]) -> String {
    let mut text = String::from("# Notes\n\n");
    for path in paths {
        text.push_str(&format!("![an image]({path})\n\n"));
    }
    text
}

async fn code_of(markdown: &str, parts: Vec<ImagePart>) -> Option<ApiCode> {
    prepare(markdown, parts)
        .await
        .err()
        .map(|refusal| refusal.code)
}

#[tokio::test]
async fn an_image_over_5_mib_is_refused_and_one_of_exactly_5_mib_is_admitted() {
    let mut padded = png(4);
    padded.resize(MAX_IMAGE_BYTES, 0);
    let mut over = padded.clone();
    over.push(0);
    let doc = showing(&["a.png"]);
    assert_eq!(
        code_of(&doc, vec![part("a.png", over)]).await,
        Some(ApiCode::ImageTooLarge)
    );
    assert_eq!(code_of(&doc, vec![part("a.png", padded)]).await, None);
}

#[tokio::test]
async fn eleven_images_are_refused_and_ten_are_admitted() {
    let names: Vec<String> = (0..11).map(|index| format!("{index}.png")).collect();
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    let doc = showing(&paths);
    let parts = |count: usize| {
        paths[..count]
            .iter()
            .map(|path| part(path, png(2)))
            .collect::<Vec<_>>()
    };
    assert_eq!(code_of(&doc, parts(11)).await, Some(ApiCode::TooManyImages));
    assert_eq!(prepare(&doc, parts(10)).await.unwrap().len(), 10);
}

#[tokio::test]
async fn a_size_is_checked_before_the_count() {
    let names: Vec<String> = (0..11).map(|index| format!("{index}.png")).collect();
    let paths: Vec<&str> = names.iter().map(String::as_str).collect();
    let mut parts: Vec<ImagePart> = paths.iter().map(|path| part(path, png(2))).collect();
    parts[3] = part("3.png", vec![0; MAX_IMAGE_BYTES + 1]);
    assert_eq!(
        code_of(&showing(&paths), parts).await,
        Some(ApiCode::ImageTooLarge)
    );
}

#[tokio::test]
async fn a_path_that_is_not_safe_is_refused_and_a_safe_one_is_admitted() {
    for path in ["../a.png", "/a.png", "img//a.png", "a\\b.png"] {
        assert_eq!(
            code_of(&showing(&[path]), vec![part(path, png(2))]).await,
            Some(ApiCode::ImageBadPath),
            "{path}"
        );
    }
    assert_eq!(
        code_of(&showing(&["img/a.png"]), vec![part("img/a.png", png(2))]).await,
        None
    );
}

#[tokio::test]
async fn a_path_sent_twice_is_refused_and_two_paths_are_admitted() {
    let doc = showing(&["a.png", "b.png"]);
    assert_eq!(
        code_of(&doc, vec![part("a.png", png(2)), part("a.png", png(3))]).await,
        Some(ApiCode::ImageDuplicate)
    );
    assert_eq!(
        code_of(&doc, vec![part("a.png", png(2)), part("b.png", png(3))]).await,
        None
    );
}

#[tokio::test]
async fn an_image_the_markdown_does_not_show_is_refused_and_one_it_shows_is_admitted() {
    let doc = showing(&["a.png"]);
    assert_eq!(
        code_of(&doc, vec![part("c.png", png(2))]).await,
        Some(ApiCode::ImageUnused)
    );
    assert_eq!(code_of(&doc, vec![part("a.png", png(2))]).await, None);
}

#[tokio::test]
async fn an_svg_named_a_png_is_refused_by_its_first_bytes_and_a_png_is_admitted() {
    let doc = showing(&["a.png"]);
    let svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\"/>".to_vec();
    assert_eq!(
        code_of(&doc, vec![part("a.png", svg)]).await,
        Some(ApiCode::ImageFormatRefused)
    );
    let prepared = prepare(&doc, vec![part("a.png", png(2))]).await.unwrap();
    assert_eq!(prepared[0].format, Format::Png);
}

#[tokio::test]
async fn a_png_whose_chunks_do_not_parse_is_unreadable_and_one_whose_pixels_do_not_decode_is_not_an_image()
 {
    let doc = showing(&["a.png"]);
    let unreadable = [b"\x89PNG\r\n\x1a\n".as_slice(), b"\0\0\0\xffIHDR"].concat();
    assert_eq!(
        code_of(&doc, vec![part("a.png", unreadable)]).await,
        Some(ApiCode::ImageUnreadable)
    );
    let ihdr = [
        &8_u32.to_be_bytes()[..],
        &1_u32.to_be_bytes(),
        &[8, 0, 0, 0, 0],
    ]
    .concat();
    let corrupt = [
        b"\x89PNG\r\n\x1a\n".as_slice(),
        &chunk(b"IHDR", &ihdr),
        &chunk(b"IDAT", b"not a zlib stream"),
        &chunk(b"IEND", b""),
    ]
    .concat();
    assert_eq!(
        code_of(&doc, vec![part("a.png", corrupt)]).await,
        Some(ApiCode::NotAnImage)
    );
}

#[tokio::test]
async fn an_image_16385_pixels_wide_is_refused_and_one_16384_wide_is_admitted() {
    let doc = showing(&["a.png"]);
    assert_eq!(
        code_of(&doc, vec![part("a.png", png(16_385))]).await,
        Some(ApiCode::TooManyPixels)
    );
    assert_eq!(code_of(&doc, vec![part("a.png", png(16_384))]).await, None);
}

#[tokio::test]
async fn each_prepared_image_keeps_its_path_and_its_stripped_bytes() {
    let doc = showing(&["img/a.png"]);
    let original = png(5);
    let prepared = prepare(&doc, vec![part("img/a.png", original.clone())])
        .await
        .unwrap();
    let cleaned = crate::images::clean(&original).unwrap();
    assert_eq!(
        prepared,
        vec![Prepared {
            path: flare_core::ImagePath::try_from("img/a.png").unwrap(),
            format: Format::Png,
            bytes: cleaned.bytes,
        }]
    );
}
