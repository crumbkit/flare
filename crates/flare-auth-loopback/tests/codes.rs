//! Launch codes and sessions: one use, 60 seconds, the limits, the cookie
//! and the target.
#![cfg(test)]

#[path = "support/fixture.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fixture is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod fixture;

use fixture::{PORT, loopback};
use flare_auth_loopback::{
    CodeError, KeepOpenerError, LaunchCode, Loopback, ReturnPath, SignInError,
};
use flare_core::contract::block_on;
use flare_core::{AuthError, Authenticator as _, Credential};
use secrecy::ExposeSecret as _;
use std::path::PathBuf;
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

fn target() -> ReturnPath {
    ReturnPath::try_from("/b/home").unwrap()
}

fn issue(loopback: &Loopback, now: Instant) -> LaunchCode {
    loopback.issue_code(target(), now).unwrap()
}

fn text(code: &LaunchCode) -> String {
    code.secret().expose_secret().to_owned()
}

#[test]
fn a_launch_code_is_64_hex_digits() {
    let code = text(&issue(&loopback(), Instant::now()));
    assert_eq!(code.len(), 64);
    assert!(
        code.bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
}

#[test]
fn a_code_signs_in_once_and_its_second_use_is_used() {
    let loopback = loopback();
    let now = Instant::now();
    let code = text(&issue(&loopback, now));
    assert!(loopback.sign_in(&code, now).is_ok());
    assert_eq!(loopback.sign_in(&code, now).unwrap_err(), SignInError::Used);
}

#[test]
fn a_code_works_at_60_seconds() {
    let loopback = loopback();
    let now = Instant::now();
    let code = text(&issue(&loopback, now));
    assert!(
        loopback
            .sign_in(&code, now + Duration::from_secs(60))
            .is_ok()
    );
}

#[test]
fn a_code_is_expired_at_61_seconds() {
    let loopback = loopback();
    let now = Instant::now();
    let code = text(&issue(&loopback, now));
    assert_eq!(
        loopback
            .sign_in(&code, now + Duration::from_secs(61))
            .unwrap_err(),
        SignInError::Expired
    );
}

#[test]
fn a_code_never_issued_is_unknown() {
    let loopback = loopback();
    for code in ["0".repeat(64), "not hex".to_owned(), String::new()] {
        assert_eq!(
            loopback.sign_in(&code, Instant::now()).unwrap_err(),
            SignInError::Unknown,
            "{code}"
        );
    }
}

#[test]
fn a_used_code_is_unknown_after_10_minutes() {
    let loopback = loopback();
    let now = Instant::now();
    let code = text(&issue(&loopback, now));
    loopback.sign_in(&code, now).unwrap();
    let later = now + Duration::from_secs(10 * 60 + 1);
    let _openers = loopback.expire(later);
    assert_eq!(
        loopback.sign_in(&code, later).unwrap_err(),
        SignInError::Unknown
    );
}

#[test]
fn a_used_code_is_still_used_just_before_10_minutes() {
    let loopback = loopback();
    let now = Instant::now();
    let code = text(&issue(&loopback, now));
    loopback.sign_in(&code, now).unwrap();
    let later = now + Duration::from_secs(10 * 60 - 1);
    let _openers = loopback.expire(later);
    assert_eq!(
        loopback.sign_in(&code, later).unwrap_err(),
        SignInError::Used
    );
}

#[test]
fn of_eight_threads_presenting_one_code_exactly_one_signs_in() {
    let loopback = loopback();
    let now = Instant::now();
    let code = Arc::new(text(&issue(&loopback, now)));
    let barrier = Arc::new(Barrier::new(8));
    let threads: Vec<_> = (0..8)
        .map(|_| {
            let loopback = loopback.clone();
            let code = Arc::clone(&code);
            let barrier = Arc::clone(&barrier);
            #[expect(
                clippy::disallowed_methods,
                reason = "the test joins each signing-in thread"
            )]
            std::thread::spawn(move || {
                barrier.wait();
                loopback.sign_in(&code, now).map(|_| ())
            })
        })
        .collect();
    let answers: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(answers.iter().filter(|answer| answer.is_ok()).count(), 1);
    assert_eq!(
        answers
            .iter()
            .filter(|answer| **answer == Err(SignInError::Used))
            .count(),
        7
    );
}

#[test]
fn the_65th_outstanding_code_is_refused() {
    let loopback = loopback();
    let now = Instant::now();
    for _ in 0..64 {
        issue(&loopback, now);
    }
    assert_eq!(
        loopback.issue_code(target(), now).unwrap_err(),
        CodeError::TooMany
    );
}

#[test]
fn a_65th_code_is_issued_once_one_code_expires() {
    let loopback = loopback();
    let now = Instant::now();
    issue(&loopback, now);
    let later = now + Duration::from_secs(1);
    for _ in 0..63 {
        issue(&loopback, later);
    }
    assert!(
        loopback
            .issue_code(target(), now + Duration::from_secs(61))
            .is_ok()
    );
}

#[test]
fn a_used_code_frees_its_place() {
    let loopback = loopback();
    let now = Instant::now();
    let first = text(&issue(&loopback, now));
    for _ in 0..63 {
        issue(&loopback, now);
    }
    loopback.sign_in(&first, now).unwrap();
    assert!(loopback.issue_code(target(), now).is_ok());
}

#[test]
fn the_65th_session_replaces_the_oldest() {
    let loopback = loopback();
    let now = Instant::now();
    let sessions: Vec<Credential> = (0..65)
        .map(|_| {
            let code = text(&issue(&loopback, now));
            loopback.sign_in(&code, now).unwrap().session().clone()
        })
        .collect();
    let (oldest, rest) = sessions.split_first().unwrap();
    assert_eq!(
        block_on(loopback.authenticate(oldest)),
        Err(AuthError::Rejected)
    );
    for session in rest {
        assert!(block_on(loopback.authenticate(session)).is_ok());
    }
}

#[test]
fn the_cookie_is_host_only_http_only_and_strict() {
    let loopback = loopback();
    let now = Instant::now();
    let code = text(&issue(&loopback, now));
    let signed_in = loopback.sign_in(&code, now).unwrap();
    let cookie = signed_in.set_cookie().expose_secret().to_owned();
    let value = cookie
        .strip_prefix("flare_session=")
        .and_then(|rest| rest.strip_suffix("; Path=/; HttpOnly; SameSite=Strict"))
        .unwrap();
    assert_eq!(value.len(), 64);
    assert!(
        value
            .bytes()
            .all(|byte| matches!(byte, b'0'..=b'9' | b'a'..=b'f'))
    );
    let Credential::Session(secret) = signed_in.session() else {
        panic!("a session credential");
    };
    assert_eq!(secret.expose_secret(), value);
}

#[test]
fn signed_in_carries_the_target_given_to_issue_code() {
    let loopback = loopback();
    let now = Instant::now();
    let wanted = ReturnPath::try_from("/b/name?x=1").unwrap();
    let code = loopback.issue_code(wanted.clone(), now).unwrap();
    let signed_in = loopback.sign_in(&text(&code), now).unwrap();
    assert_eq!(signed_in.target(), &wanted);
    assert_eq!(signed_in.target().as_str(), "/b/name?x=1");
}

#[test]
fn a_return_path_that_leaves_the_server_is_refused() {
    for path in [
        "//host", "/\\host", "http://x", "/a\nb", "/a\u{7f}", "", "a/b",
    ] {
        assert!(ReturnPath::try_from(path).is_err(), "{path:?}");
    }
}

#[test]
fn a_return_path_on_the_server_passes() {
    for path in ["/", "/b/name?x=1", "/b/name#part"] {
        assert!(ReturnPath::try_from(path).is_ok(), "{path:?}");
    }
}

#[test]
fn the_launch_url_names_the_launch_host_and_the_code() {
    let loopback = loopback();
    let code = issue(&loopback, Instant::now());
    let url = loopback.launch_url(&code);
    assert_eq!(
        url.expose_secret(),
        format!(
            "http://{}:{PORT}/launch?code={}",
            loopback.launch_host().name(),
            text(&code)
        )
    );
}

#[test]
fn the_opener_refreshes_to_the_launch_url() {
    let loopback = loopback();
    let code = issue(&loopback, Instant::now());
    let url = loopback.launch_url(&code).expose_secret().to_owned();
    let page = loopback.opener_page(&code).expose_secret().to_owned();
    assert!(
        page.contains(&format!(
            "<meta http-equiv=\"refresh\" content=\"0;url={url}\">"
        )),
        "{page}"
    );
    assert!(page.contains("<meta name=\"referrer\" content=\"no-referrer\">"));
}

#[test]
fn the_debug_output_of_each_secret_type_hides_the_secret() {
    let loopback = loopback();
    let now = Instant::now();
    let code = issue(&loopback, now);
    let code_text = text(&code);
    let signed_in = loopback.sign_in(&code_text, now).unwrap();
    let cookie = signed_in.set_cookie().expose_secret().to_owned();
    let record = loopback.server_record();
    let token = record.token().expose_secret().to_owned();
    let shown = format!("{code:?} {signed_in:?} {record:?} {loopback:?}");
    for secret in [code_text.as_str(), &cookie[15..79], token.as_str()] {
        assert!(!shown.contains(secret), "{secret} appears in {shown}");
    }
}

#[test]
fn a_kept_opener_comes_back_with_sign_in() {
    let loopback = loopback();
    let now = Instant::now();
    let code = issue(&loopback, now);
    let opener = PathBuf::from("/state/open-0123456789abcdef.html");
    loopback.keep_opener(&code, opener.clone()).unwrap();
    let signed_in = loopback.sign_in(&text(&code), now).unwrap();
    assert_eq!(signed_in.opener(), Some(opener.as_path()));
}

#[test]
fn expire_returns_the_openers_of_expired_codes_once() {
    let loopback = loopback();
    let now = Instant::now();
    let old = issue(&loopback, now);
    let fresh = issue(&loopback, now + Duration::from_secs(30));
    loopback
        .keep_opener(&old, PathBuf::from("/state/open-old.html"))
        .unwrap();
    loopback
        .keep_opener(&fresh, PathBuf::from("/state/open-fresh.html"))
        .unwrap();
    let later = now + Duration::from_secs(61);
    assert_eq!(
        loopback.expire(later),
        vec![PathBuf::from("/state/open-old.html")]
    );
    assert!(loopback.expire(later).is_empty());
}

#[test]
fn an_opener_kept_after_sign_in_comes_back_from_expire() {
    let loopback = loopback();
    let now = Instant::now();
    let code = issue(&loopback, now);
    loopback.sign_in(&text(&code), now).unwrap();
    loopback
        .keep_opener(&code, PathBuf::from("/state/open-late.html"))
        .unwrap();
    assert_eq!(
        loopback.expire(now + Duration::from_secs(61)),
        vec![PathBuf::from("/state/open-late.html")]
    );
}

#[test]
fn keep_opener_reports_a_code_the_server_has_forgotten() {
    let loopback = loopback();
    let now = Instant::now();
    let code = issue(&loopback, now);
    let _openers = loopback.expire(now + Duration::from_secs(10 * 60 + 1));
    assert_eq!(
        loopback
            .keep_opener(&code, PathBuf::from("/state/open-x.html"))
            .unwrap_err(),
        KeepOpenerError::UnknownCode
    );
}

#[test]
fn the_opener_of_a_code_that_issue_code_forgets_comes_back_from_the_next_expire() {
    let loopback = loopback();
    let now = Instant::now();
    let old = issue(&loopback, now);
    loopback
        .keep_opener(&old, PathBuf::from("/state/open-old.html"))
        .unwrap();
    let later = now + Duration::from_secs(10 * 60 + 1);
    issue(&loopback, later);
    assert_eq!(
        loopback.expire(later),
        vec![PathBuf::from("/state/open-old.html")]
    );
}

#[test]
fn the_debug_output_hides_the_launch_host_name() {
    let loopback = loopback();
    let record = loopback.server_record();
    let name = loopback.launch_host().name().to_owned();
    let shown = format!("{loopback:?} {record:?} {:?}", loopback.launch_host());
    assert!(!shown.contains(&name), "{name} appears in {shown}");
    assert!(shown.contains(&PORT.to_string()), "{shown}");
}
