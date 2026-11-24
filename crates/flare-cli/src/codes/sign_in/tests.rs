use crate::codes::{Failed, SYSTEM_UNAVAILABLE, SharingFailed};
use crumbkit::{Call, Failure as _, Recovery, Then};

const SERVER: &str = "https://flare.example.org";

fn login() -> Call {
    Call::new(&["login"]).value("server", SERVER)
}

fn system() -> Failed {
    Failed::System {
        problem: "the request could not be written".to_owned(),
    }
}

#[test]
fn a_failure_that_retries_hands_the_next_sign_in_to_a_person_in_a_sign_in() {
    assert_eq!(system().recovery(), Recovery::Retry);
    let failed = system().in_a_sign_in(SERVER);
    assert_eq!(failed.code(), SYSTEM_UNAVAILABLE);
    assert_eq!(failed.recovery(), Recovery::Terminal(login()));
}

#[test]
fn a_repair_before_a_retry_comes_before_the_sign_in_in_a_sign_in() {
    let failed = Failed::from(SharingFailed::BadAnswer {
        server: SERVER.to_owned(),
        problem: "the answer was cut short".to_owned(),
        next: crate::codes::NextRun::Clean,
    })
    .in_a_sign_in(SERVER);
    assert_eq!(
        failed.recovery(),
        Recovery::after(
            "check that the server runs flare with the share API",
            Then::Terminal(login()),
        )
    );
}

#[test]
fn a_hint_stays_a_hint_in_a_sign_in() {
    let failed = Failed::from(SharingFailed::InvalidServer {
        server: "ftp://x".to_owned(),
        problem: "not https".to_owned(),
    });
    let hint = failed.recovery();
    assert!(matches!(hint, Recovery::Hint { .. }), "{hint:?}");
    assert_eq!(failed.in_a_sign_in(SERVER).recovery(), hint);
}
