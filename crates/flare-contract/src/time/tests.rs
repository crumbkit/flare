use super::*;

#[test]
fn a_moment_prints_as_an_rfc_3339_time_in_utc() {
    assert_eq!(utc(0), "1970-01-01T00:00:00Z");
    assert_eq!(utc(1_795_396_324), "2026-11-23T01:12:04Z");
    assert_eq!(utc(951_782_400), "2000-02-29T00:00:00Z");
    assert_eq!(utc(4_102_444_799), "2099-12-31T23:59:59Z");
}
