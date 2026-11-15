//! What the loopback authenticator accepts and what it lets the local
//! principal reach.
#![cfg(test)]

#[path = "support/fixture.rs"]
#[expect(
    clippy::missing_panics_doc,
    clippy::must_use_candidate,
    reason = "the fixture is test support: it panics on a broken fixture, and each test file uses part of it"
)]
pub mod fixture;

use fixture::{bearer, loopback, loopback_serving, session, tasks_only};
use hangar_auth_loopback::{Served, local_principal};
use hangar_core::contract::block_on;
use hangar_core::{
    Access, AuthError, Authenticator as _, BindingName, Credential, Decision, PageName, Principal,
    PrincipalId, RecordPath, Resource,
};
use secrecy::ExposeSecret as _;
use std::time::Instant;

fn record(path: &str) -> Resource {
    Resource::Record(RecordPath::try_from(path).unwrap())
}

fn decide(served: Served, principal: &Principal, resource: &Resource) -> [Decision; 2] {
    let loopback = loopback_serving(served);
    [Access::Read, Access::Write]
        .map(|access| block_on(loopback.authorize(principal, access, resource)).unwrap())
}

#[test]
fn the_local_principal_id_is_local_and_the_name() {
    let principal = local_principal("ada").unwrap();
    assert_eq!(principal.id().as_str(), "local:ada");
}

#[test]
fn a_name_with_a_space_is_not_a_local_principal() {
    assert!(local_principal("ada lovelace").is_err());
    assert!(local_principal("").is_err());
}

#[test]
fn the_right_token_and_a_live_session_give_the_local_principal() {
    let loopback = loopback();
    let local = local_principal("ada").unwrap();
    let token = bearer(&loopback);
    let session = session(&loopback, Instant::now());
    assert_eq!(block_on(loopback.authenticate(&token)), Ok(local.clone()));
    assert_eq!(block_on(loopback.authenticate(&session)), Ok(local));
}

#[test]
fn a_wrong_token_and_an_unknown_session_secret_are_rejected() {
    let loopback = loopback();
    let wrong_token = Credential::bearer("a".repeat(64));
    let unknown_session = Credential::session("b".repeat(64));
    assert_eq!(
        block_on(loopback.authenticate(&wrong_token)),
        Err(AuthError::Rejected)
    );
    assert_eq!(
        block_on(loopback.authenticate(&unknown_session)),
        Err(AuthError::Rejected)
    );
}

#[test]
fn the_token_presented_as_a_session_and_a_session_presented_as_a_token_are_rejected() {
    let loopback = loopback();
    let token = loopback.server_record().token().clone();
    let Credential::Session(secret) = session(&loopback, Instant::now()) else {
        panic!("sign_in gives a session credential");
    };
    assert_eq!(
        block_on(loopback.authenticate(&Credential::Session(token))),
        Err(AuthError::Rejected)
    );
    assert_eq!(
        block_on(loopback.authenticate(&Credential::bearer(secret.expose_secret()))),
        Err(AuthError::Rejected)
    );
}

#[test]
fn a_server_that_serves_everything_lets_the_local_principal_read_and_write_everything() {
    let local = local_principal("ada").unwrap();
    for resource in [
        Resource::Page(PageName::try_from("home").unwrap()),
        Resource::Binding(BindingName::try_from("current").unwrap()),
        record("/orders/a"),
        record("/tasks/a"),
    ] {
        assert_eq!(
            decide(Served::Everything, &local, &resource),
            [Decision::Allowed; 2],
            "{resource:?}"
        );
    }
}

#[test]
fn another_principal_reaches_nothing() {
    let other = Principal::new(PrincipalId::try_from("local:bob").unwrap());
    for resource in [
        Resource::Page(PageName::try_from("home").unwrap()),
        Resource::Binding(BindingName::try_from("current").unwrap()),
        record("/tasks/a"),
    ] {
        assert_eq!(
            decide(Served::Everything, &other, &resource),
            [Decision::Denied; 2],
            "{resource:?}"
        );
    }
}

#[test]
fn records_under_a_path_allow_that_path_and_its_children_only() {
    let local = local_principal("ada").unwrap();
    for (path, decision) in [
        ("/tasks", Decision::Allowed),
        ("/tasks/a", Decision::Allowed),
        ("/tasks/a/b", Decision::Allowed),
        ("/tasksx", Decision::Denied),
        ("/orders/a", Decision::Denied),
    ] {
        assert_eq!(
            decide(tasks_only(), &local, &record(path)),
            [decision; 2],
            "{path}"
        );
    }
}

#[test]
fn records_under_a_path_still_allow_pages_and_bindings() {
    let local = local_principal("ada").unwrap();
    let page = Resource::Page(PageName::try_from("home").unwrap());
    assert_eq!(decide(tasks_only(), &local, &page), [Decision::Allowed; 2]);
}
