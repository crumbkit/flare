use super::*;

#[test]
fn a_cursor_names_its_position_after_a_version_prefix() {
    assert_eq!(Cursor::new(47).to_string(), "fc1.AAAAAAAAAC8");
    assert_eq!(Cursor::try_from("fc1.AAAAAAAAAC8").unwrap().position(), 47);
}

#[test]
fn every_position_round_trips_through_its_text() {
    for position in [0, 1, 255, 256, u64::from(u32::MAX), u64::MAX] {
        let text = Cursor::new(position).to_string();
        assert_eq!(
            Cursor::try_from(text.as_str()).unwrap().position(),
            position
        );
    }
}

#[test]
fn a_cursor_with_another_prefix_or_length_is_refused_and_a_well_formed_one_is_admitted() {
    for bad in [
        "",
        "fc2.AAAAAAAAAC8",
        "AAAAAAAAAC8",
        "fc1.AAAAAAAAAC",
        "fc1.AAAAAAAAAC8A",
        "fc1.!!!!!!!!!!!",
    ] {
        assert_eq!(Cursor::try_from(bad), Err(CursorError), "{bad}");
    }
    assert!(Cursor::try_from("fc1.AAAAAAAAAAA").is_ok());
}

const SHARE: [u8; 16] = [
    0x3f, 0x9c, 0x0a, 0x17, 0xe2, 0xb4, 0x4d, 0x6f, 0x8a, 0x1c, 0x5b, 0x7e, 0x9d, 0x20, 0xf4, 0xa6,
];

#[test]
fn a_share_cursor_names_its_scope_time_and_share_after_a_version_prefix() {
    let cursor = ShareCursor::new(ListScope::WithMe, 47, SHARE);
    let text = cursor.to_string();
    assert!(text.starts_with("fs1."), "{text}");
    assert_eq!(text.len(), 4 + 34, "{text}");
    let parsed = ShareCursor::try_from(text.as_str()).unwrap();
    assert_eq!(parsed, cursor);
    assert_eq!(parsed.scope(), ListScope::WithMe);
    assert_eq!(parsed.seconds(), 47);
    assert_eq!(parsed.share_hex(), "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6");
}

#[test]
fn every_share_cursor_round_trips_through_its_text() {
    for scope in [ListScope::Mine, ListScope::WithMe] {
        for seconds in [0, 1, u64::from(u32::MAX), u64::MAX] {
            for share in [[0; 16], [0xff; 16], SHARE] {
                let cursor = ShareCursor::new(scope, seconds, share);
                let text = cursor.to_string();
                assert_eq!(ShareCursor::try_from(text.as_str()), Ok(cursor), "{text}");
            }
        }
    }
}

#[test]
fn a_share_cursor_is_built_from_a_share_ids_hex_and_refuses_other_text() {
    let cursor =
        ShareCursor::from_hex(ListScope::Mine, 9, "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6").unwrap();
    assert_eq!(cursor, ShareCursor::new(ListScope::Mine, 9, SHARE));
    for bad in [
        "",
        "3f9c0a17e2b44d6f8a1c5b7e9d20f4a",
        "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6a",
        "3F9C0A17E2B44D6F8A1C5B7E9D20F4A6",
        "zz9c0a17e2b44d6f8a1c5b7e9d20f4a6",
    ] {
        assert_eq!(
            ShareCursor::from_hex(ListScope::Mine, 9, bad),
            Err(CursorError),
            "{bad}"
        );
    }
}

#[test]
fn a_malformed_truncated_or_activity_cursor_is_refused_as_a_share_cursor_and_a_well_formed_one_is_admitted()
 {
    let good = ShareCursor::new(ListScope::Mine, 47, SHARE).to_string();
    let body = good.strip_prefix("fs1.").unwrap();
    let truncated = &good[..good.len() - 2];
    let longer = format!("{good}AA");
    let activity = format!("fc1.{body}");
    let next_version = format!("fs2.{body}");
    for bad in [
        "",
        "fs1.",
        "fs1.!!!!",
        "fc1.AAAAAAAAAC8",
        truncated,
        longer.as_str(),
        activity.as_str(),
        next_version.as_str(),
        body,
    ] {
        assert_eq!(ShareCursor::try_from(bad), Err(CursorError), "{bad}");
    }
    assert!(ShareCursor::try_from(good.as_str()).is_ok());
}

#[test]
fn a_share_cursor_with_a_scope_byte_other_than_mine_or_with_me_is_refused() {
    let mut bytes = [0_u8; 25];
    bytes[0] = 2;
    let text = format!("fs1.{}", URL_SAFE_NO_PAD.encode(bytes));
    assert_eq!(ShareCursor::try_from(text.as_str()), Err(CursorError));
    bytes[0] = 1;
    let text = format!("fs1.{}", URL_SAFE_NO_PAD.encode(bytes));
    assert_eq!(
        ShareCursor::try_from(text.as_str()).map(ShareCursor::scope),
        Ok(ListScope::WithMe)
    );
}

#[test]
fn a_share_cursor_is_refused_as_an_activity_cursor() {
    let text = ShareCursor::new(ListScope::Mine, 47, SHARE).to_string();
    assert_eq!(Cursor::try_from(text.as_str()), Err(CursorError));
}
