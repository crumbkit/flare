//! Unit tests of the rule that `FileSecret` applies to the open file's
//! type, owner and mode. The owner cases live here because a test that
//! does not run as root cannot make a file that another user owns.

use super::{Kind, check};
use crate::refusal::SecretError;

const ME: u32 = 501;

#[test]
fn a_file_owned_by_another_user_is_refused_and_one_owned_by_the_running_user_or_root_is_admitted() {
    assert_eq!(
        check(Kind::File, 502, 0o600, ME),
        Err(SecretError::FileOwner)
    );
    assert_eq!(check(Kind::File, 0, 0o600, 0), Ok(()));
    assert_eq!(check(Kind::File, ME, 0o600, ME), Ok(()));
    assert_eq!(check(Kind::File, 0, 0o400, ME), Ok(()));
}

#[test]
fn a_group_or_world_writable_file_is_refused_and_a_world_readable_one_is_admitted() {
    for mode in [0o620, 0o602, 0o622, 0o777] {
        assert_eq!(
            check(Kind::File, ME, mode, ME),
            Err(SecretError::FileWritable),
            "{mode:o}"
        );
    }
    for mode in [0o400, 0o600, 0o640, 0o644, 0o444] {
        assert_eq!(check(Kind::File, ME, mode, ME), Ok(()), "{mode:o}");
    }
}

#[test]
fn anything_but_a_regular_file_is_refused_and_a_regular_file_is_admitted() {
    assert_eq!(
        check(Kind::Other, ME, 0o600, ME),
        Err(SecretError::NotAFile)
    );
    assert_eq!(check(Kind::File, ME, 0o600, ME), Ok(()));
}
