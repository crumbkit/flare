use super::parse;
use std::time::Duration;

#[test]
fn a_whole_number_with_a_unit_is_a_duration() {
    for (text, seconds) in [
        ("1s", 1),
        ("90s", 90),
        ("30m", 1800),
        ("8h", 28_800),
        ("2d", 172_800),
    ] {
        assert_eq!(parse(text), Ok(Duration::from_secs(seconds)), "{text}");
    }
}

#[test]
fn a_number_without_a_unit_a_zero_or_other_text_is_refused() {
    for text in [
        "",
        "8",
        "h",
        "0s",
        "-1h",
        "1.5h",
        "1w",
        "1 h",
        "99999999999999999999h",
    ] {
        assert!(parse(text).is_err(), "{text}");
    }
}
