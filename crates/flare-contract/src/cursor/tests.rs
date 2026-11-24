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
