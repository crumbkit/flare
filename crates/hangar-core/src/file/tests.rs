//! Unit tests of the file types.

use super::{FileBytes, FileId, MAX_FILE_BYTES, MediaType, NewFile, StoredFile};
use crate::error::{ParseError, Subject};
use crate::share::ShareId;

#[test]
fn a_file_id_parses_only_32_lower_case_hexadecimal_characters() {
    let id = FileId::from_random([0xCD; 16]);
    assert_eq!(id.as_str(), "cd".repeat(16));
    assert_eq!(FileId::try_from(id.as_str()).unwrap(), id);
    for text in ["", "CD", &"cd".repeat(17), &"zz".repeat(16)] {
        assert!(
            matches!(
                FileId::try_from(text),
                Err(ParseError::Malformed {
                    subject: Subject::FileId,
                    ..
                })
            ),
            "{text:?}"
        );
    }
}

#[test]
fn a_media_type_is_held_in_lower_case_and_admits_100_bytes() {
    assert_eq!(
        MediaType::try_from("Image/PNG").unwrap().as_str(),
        "image/png"
    );
    let longest = format!("image/{}", "x".repeat(94));
    assert_eq!(longest.len(), 100);
    assert!(MediaType::try_from(longest.as_str()).is_ok());
    for text in ["image/svg+xml", "application/vnd.a-b.c_d"] {
        assert!(MediaType::try_from(text).is_ok(), "{text:?}");
    }
}

#[test]
fn a_media_type_without_one_slash_between_two_names_or_over_100_bytes_is_refused() {
    for text in [
        "image",
        "/png",
        "image/",
        "image/png/x",
        "image /png",
        "image/p\u{e9}g",
        "image/png;charset=x",
    ] {
        assert!(
            matches!(
                MediaType::try_from(text),
                Err(ParseError::Malformed {
                    subject: Subject::MediaType,
                    ..
                })
            ),
            "{text:?}"
        );
    }
    assert!(matches!(
        MediaType::try_from(""),
        Err(ParseError::Empty {
            subject: Subject::MediaType
        })
    ));
    let longer = format!("image/{}", "x".repeat(95));
    assert!(matches!(
        MediaType::try_from(longer.as_str()),
        Err(ParseError::TooLong {
            subject: Subject::MediaType,
            max: 100
        })
    ));
}

#[test]
fn file_bytes_admit_10_mib_and_refuse_one_byte_more() {
    assert_eq!(MAX_FILE_BYTES, 10 * 1024 * 1024);
    let largest = FileBytes::try_from(vec![0; MAX_FILE_BYTES]).unwrap();
    assert_eq!(largest.as_bytes().len(), MAX_FILE_BYTES);
    assert!(matches!(
        FileBytes::try_from(vec![0; MAX_FILE_BYTES + 1]),
        Err(ParseError::TooLong {
            subject: Subject::FileBytes,
            max: MAX_FILE_BYTES
        })
    ));
}

#[test]
fn the_debug_output_of_a_file_gives_its_length_and_not_its_bytes() {
    let file = NewFile::new(
        ShareId::from_random([1; 16]),
        FileId::from_random([2; 16]),
        MediaType::try_from("image/png").unwrap(),
        FileBytes::try_from(b"alice@example.com".to_vec()).unwrap(),
    );
    let stored = StoredFile::from(file.clone());
    for debug in [format!("{file:?}"), format!("{stored:?}")] {
        assert!(!debug.contains("alice"), "{debug}");
        assert!(debug.contains("17 bytes"), "{debug}");
    }
}
