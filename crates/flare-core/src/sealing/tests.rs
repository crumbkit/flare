use super::{KeyBytes, KeyId, SealingKey};
use crate::Timestamp;

#[test]
fn a_key_is_due_thirty_days_after_it_was_written() {
    let key = SealingKey::new(
        KeyId::FIRST,
        KeyBytes::new([1; 32]),
        Timestamp::from_secs(100),
    );
    let day = 24 * 60 * 60;
    assert!(!key.is_due(Timestamp::from_secs(100 + 30 * day - 1)));
    assert!(key.is_due(Timestamp::from_secs(100 + 30 * day)));
}

#[test]
fn key_bytes_hide_themselves_in_debug_output() {
    assert_eq!(
        format!("{:?}", KeyBytes::new([7; 32])),
        "KeyBytes(<hidden>)"
    );
}

#[test]
fn key_ids_count_from_one_and_stop_at_the_largest() {
    assert_eq!(KeyId::new(0), None);
    assert_eq!(KeyId::FIRST.next().map(KeyId::get), Some(2));
    assert_eq!(KeyId::new(u32::MAX).and_then(KeyId::next), None);
}
