use super::{SESSION_CAP, SESSION_IDLE, SessionRule};
use crate::Timestamp;

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

const HOUR: u64 = 3600;
const MINUTE: u64 = 60;

#[test]
fn a_new_session_expires_twelve_hours_after_sign_in() {
    let rule = SessionRule::default();
    assert_eq!(rule.first_expiry(at(1_000)), at(1_000 + SESSION_IDLE));
}

#[test]
fn a_request_inside_ten_minutes_of_the_last_extension_changes_nothing() {
    let rule = SessionRule::default();
    let expires = at(1_000 + 12 * HOUR);
    assert_eq!(
        rule.extended(at(1_000), expires, at(1_000 + 9 * MINUTE)),
        None
    );
}

#[test]
fn a_request_after_ten_minutes_extends_to_twelve_hours_from_it() {
    let rule = SessionRule::default();
    let expires = at(1_000 + 12 * HOUR);
    let now = at(1_000 + 11 * MINUTE);
    assert_eq!(
        rule.extended(at(1_000), expires, now),
        Some(at(1_000 + 11 * MINUTE + 12 * HOUR))
    );
}

#[test]
fn an_extension_stops_at_seven_days_after_sign_in() {
    let rule = SessionRule::default();
    let created = at(1_000);
    let now = at(1_000 + 6 * 24 * HOUR + 20 * HOUR);
    let expires = at(1_000 + 6 * 24 * HOUR + 10 * HOUR);
    assert_eq!(
        rule.extended(created, expires, now),
        Some(at(1_000 + SESSION_CAP))
    );
}

#[test]
fn a_session_at_its_cap_is_never_extended() {
    let rule = SessionRule::default();
    let created = at(1_000);
    let expires = at(1_000 + SESSION_CAP);
    assert_eq!(
        rule.extended(created, expires, at(1_000 + 6 * 24 * HOUR + 23 * HOUR)),
        None
    );
}

#[test]
fn an_idle_life_shorter_than_the_refresh_never_extends() {
    let rule = SessionRule::new(5 * MINUTE);
    let expires = rule.first_expiry(at(0));
    assert_eq!(rule.extended(at(0), expires, at(4 * MINUTE)), None);
}
