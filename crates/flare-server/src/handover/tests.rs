use super::{Handed, Handover, Redeemed};
use flare_core::Credential;
use secrecy::{ExposeSecret as _, SecretString};
use std::time::{Duration, Instant};

fn handed() -> Handed {
    Handed {
        session: Credential::Session(SecretString::from("a-session-for-the-test")),
        return_to: "/flare/".to_owned(),
        lasts: Duration::from_secs(60 * 60),
    }
}

#[test]
fn a_code_works_once() {
    let table = Handover::new(4, Duration::from_secs(60));
    let now = Instant::now();
    let issued = table.issue(handed(), now).unwrap();
    let code = issued.code.expose_secret();
    assert!(matches!(table.redeem(code, now), Redeemed::Handed(_)));
    assert!(matches!(table.redeem(code, now), Redeemed::Unknown));
}

#[test]
fn a_table_that_a_panic_poisoned_answers_unusable() {
    let table = Handover::new(4, Duration::from_secs(60));
    let now = Instant::now();
    let issued = table.issue(handed(), now).unwrap();
    let poisoned = std::panic::catch_unwind(|| {
        let _rows = table.rows.lock().unwrap();
        panic!("a panic while the table is locked");
    });
    assert!(poisoned.is_err());
    assert!(matches!(
        table.redeem(issued.code.expose_secret(), now),
        Redeemed::Unusable
    ));
}
