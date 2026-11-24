//! Unit tests of the image path and the image table.

use super::{ImagePath, MAX_IMAGE_PATH, MAX_IMAGES, VersionImage, VersionImages};
use crate::error::{ParseError, Subject};
use crate::file::{FileId, MediaType};

fn row(path: &str, file: u8) -> VersionImage {
    VersionImage::new(
        ImagePath::try_from(path).unwrap(),
        FileId::from_random([file; 16]),
        MediaType::try_from("image/png").unwrap(),
        10,
    )
}

#[test]
fn an_image_path_admits_relative_segments_up_to_255_bytes() {
    for text in [
        "a.png",
        "img/chart.png",
        "deep/er/x.y.webp",
        "with space.png",
        &"a".repeat(MAX_IMAGE_PATH),
    ] {
        assert_eq!(ImagePath::try_from(text).unwrap().as_str(), text);
    }
}

#[test]
fn an_image_path_refuses_absolute_parent_dot_empty_backslash_colon_and_control_paths() {
    for text in [
        "/etc/a.png",
        "../a.png",
        "img/../a.png",
        "./a.png",
        "img//a.png",
        "img/",
        "img\\a.png",
        "c:a.png",
        "a\0.png",
        "a\n.png",
        "a\u{7f}.png",
    ] {
        assert!(
            matches!(
                ImagePath::try_from(text),
                Err(ParseError::Malformed {
                    subject: Subject::ImagePath,
                    ..
                })
            ),
            "{text:?}"
        );
    }
    assert!(matches!(
        ImagePath::try_from(""),
        Err(ParseError::Empty { .. })
    ));
    assert!(matches!(
        ImagePath::try_from("a".repeat(MAX_IMAGE_PATH + 1).as_str()),
        Err(ParseError::TooLong { .. })
    ));
}

#[test]
fn an_image_paths_debug_output_gives_only_its_length() {
    let path = ImagePath::try_from("ana-at-home.png").unwrap();
    assert_eq!(format!("{path:?}"), "ImagePath(<15 bytes>)");
}

#[test]
fn an_image_table_admits_ten_rows_and_refuses_eleven() {
    let ten: Vec<_> = (0..10_u8)
        .map(|index| row(&format!("{index}.png"), index))
        .collect();
    assert_eq!(
        VersionImages::try_from(ten).unwrap().rows().len(),
        MAX_IMAGES
    );
    let eleven: Vec<_> = (0..11_u8)
        .map(|index| row(&format!("{index}.png"), index))
        .collect();
    assert!(matches!(
        VersionImages::try_from(eleven),
        Err(ParseError::TooMany { .. })
    ));
}

#[test]
fn an_image_table_refuses_a_path_or_a_file_named_twice_and_admits_distinct_rows() {
    let same_path = vec![row("a.png", 1), row("a.png", 2)];
    let same_file = vec![row("a.png", 1), row("b.png", 1)];
    for rows in [same_path, same_file] {
        assert!(matches!(
            VersionImages::try_from(rows),
            Err(ParseError::Duplicate { .. })
        ));
    }
    let table = VersionImages::try_from(vec![row("a.png", 1), row("b.png", 2)]).unwrap();
    assert!(table.names(&FileId::from_random([2; 16])));
    assert!(!table.names(&FileId::from_random([3; 16])));
}
