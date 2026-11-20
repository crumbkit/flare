//! Unit tests of the share types.

use super::{DocumentText, ShareContent, ShareId, ShareKind, ShareTitle, ShareVersion};
use super::{NewShare, NewVersion, Share, SharedVersion};
use crate::error::{ParseError, Subject};
use crate::name::PrincipalId;
use crate::time::Timestamp;

fn owner() -> PrincipalId {
    PrincipalId::try_from("alice@example.com").unwrap()
}

fn document(text: &str) -> ShareContent {
    ShareContent::Document(DocumentText::try_from(text).unwrap())
}

#[test]
fn a_share_id_from_random_bytes_is_32_lower_case_hexadecimal_characters() {
    let id = ShareId::from_random([0xAB; 16]);
    assert_eq!(id.as_str(), "ab".repeat(16));
    assert_eq!(ShareId::try_from(id.as_str()).unwrap(), id);
}

#[test]
fn a_share_id_parses_only_32_lower_case_hexadecimal_characters() {
    let good = "0123456789abcdef0123456789abcdef";
    assert!(ShareId::try_from(good).is_ok());
    for text in [
        "",
        "0123456789abcdef0123456789abcde",
        "0123456789abcdef0123456789abcdef0",
        "0123456789ABCDEF0123456789abcdef",
        "0123456789abcdeg0123456789abcdef",
        "../3456789abcdef0123456789abcdef",
    ] {
        assert!(
            matches!(
                ShareId::try_from(text),
                Err(ParseError::Malformed {
                    subject: Subject::ShareId,
                    ..
                })
            ),
            "{text:?}"
        );
    }
}

#[test]
fn a_share_version_has_no_zero_and_next_refuses_an_overflow() {
    assert!(matches!(
        ShareVersion::try_from(0),
        Err(ParseError::Malformed {
            subject: Subject::ShareVersion,
            ..
        })
    ));
    let first = ShareVersion::first();
    assert_eq!(first.get(), 1);
    assert_eq!(ShareVersion::try_from(1).unwrap(), first);
    assert_eq!(first.next().map(ShareVersion::get), Some(2));
    let last = ShareVersion::try_from(u32::MAX).unwrap();
    assert_eq!(last.next(), None);
}

#[test]
fn a_share_title_refuses_an_empty_title_a_control_character_and_201_bytes_and_admits_200() {
    assert!(matches!(
        ShareTitle::try_from(""),
        Err(ParseError::Empty {
            subject: Subject::ShareTitle
        })
    ));
    assert!(matches!(
        ShareTitle::try_from("a\u{7}b"),
        Err(ParseError::Malformed {
            subject: Subject::ShareTitle,
            ..
        })
    ));
    let longest = "t".repeat(200);
    assert_eq!(
        ShareTitle::try_from(longest.as_str()).unwrap().as_str(),
        longest
    );
    let longer = "t".repeat(201);
    assert!(matches!(
        ShareTitle::try_from(longer.as_str()),
        Err(ParseError::TooLong {
            subject: Subject::ShareTitle,
            max: 200
        })
    ));
}

#[test]
fn a_document_refuses_an_empty_text_a_nul_and_one_byte_over_256_kib_and_admits_256_kib() {
    assert!(matches!(
        DocumentText::try_from(""),
        Err(ParseError::Empty {
            subject: Subject::DocumentText
        })
    ));
    assert!(matches!(
        DocumentText::try_from("a\0b"),
        Err(ParseError::Malformed {
            subject: Subject::DocumentText,
            ..
        })
    ));
    let longest = "m".repeat(262_144);
    assert!(DocumentText::try_from(longest.as_str()).is_ok());
    let longer = "m".repeat(262_145);
    assert!(matches!(
        DocumentText::try_from(longer.as_str()),
        Err(ParseError::TooLong {
            subject: Subject::DocumentText,
            max: 262_144
        })
    ));
}

#[test]
fn a_document_content_is_of_the_document_kind() {
    assert_eq!(document("# Notes\n").kind(), ShareKind::Document);
}

#[test]
fn the_debug_output_of_a_share_and_its_versions_holds_no_address_and_no_text() {
    let id = ShareId::from_random([1; 16]);
    let title = ShareTitle::try_from("Notes for alice@example.com").unwrap();
    let at = Timestamp::from_secs(1_700_000_000);
    let share = Share::new(
        id.clone(),
        owner(),
        ShareKind::Document,
        title.clone(),
        at,
        ShareVersion::first(),
    );
    let new = NewShare::new(
        id.clone(),
        owner(),
        title,
        document("alice@example.com"),
        at,
    );
    let version = NewVersion::new(document("alice@example.com"), owner(), at);
    let shared = SharedVersion::new(
        id,
        ShareVersion::first(),
        document("alice@example.com"),
        owner(),
        at,
    );
    for debug in [
        format!("{share:?}"),
        format!("{new:?}"),
        format!("{version:?}"),
        format!("{shared:?}"),
    ] {
        assert!(!debug.contains("alice"), "{debug}");
        assert!(debug.contains("<principal>"), "{debug}");
    }
}
