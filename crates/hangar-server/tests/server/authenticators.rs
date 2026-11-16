//! Plan test 11: `authorize` reaches only the side that authenticated, and
//! no decision outlives a session.

use crate::fake_provider::{Fake, HOME, PRINCIPAL, connect, sign_in};
use crate::oidc;
use hangar_auth_loopback::{Loopback, Served};
use hangar_core::{
    Access, AuthError, Credential, Decision, PageName, Principal, PrincipalId, RecordPath, Resource,
};
use hangar_server::{Gate, Side};
use secrecy::ExposeSecret as _;
use std::time::Duration;

/// A gate whose loopback side and OpenID Connect side both name the
/// principal `ada`. Loopback serves every page and only records under
/// `/tasks`; the allow-list reaches only the page `home`.
async fn two_sided_gate(fake: &Fake) -> (Gate<hangar_auth_oidc::TestClock>, Loopback, Credential) {
    let (authenticator, _clock) = connect(fake).await;
    let signed_in = sign_in(&authenticator, fake).await.unwrap();
    let ada = Principal::new(PrincipalId::try_from(PRINCIPAL).unwrap());
    let loopback = Loopback::new(
        1,
        ada,
        Served::RecordsUnder(vec![RecordPath::try_from("/tasks").unwrap()]),
    )
    .unwrap();
    let session = signed_in.session().clone();
    (
        Gate::new(loopback.clone(), Some(authenticator)),
        loopback,
        session,
    )
}

#[tokio::test]
async fn a_loopback_credential_is_authorized_by_loopback_alone() {
    let fake = Fake::start().await;
    let (gate, loopback, _) = two_sided_gate(&fake).await;
    let token = Credential::bearer(loopback.server_record().token().expose_secret());
    let admitted = gate.authenticate(&token).await.unwrap();
    assert_eq!(admitted.by, Side::Loopback);
    assert_eq!(admitted.principal.id().as_str(), PRINCIPAL);
    let elsewhere = Resource::Page(PageName::try_from("elsewhere").unwrap());
    assert_eq!(
        gate.authorize(&admitted, Access::Read, &elsewhere)
            .await
            .unwrap(),
        Decision::Allowed,
        "loopback serves every page; the allow-list would deny this one"
    );
}

#[tokio::test]
async fn an_oidc_session_is_authorized_by_the_allow_list_alone() {
    let fake = Fake::start().await;
    let (gate, _, session) = two_sided_gate(&fake).await;
    let admitted = gate.authenticate(&session).await.unwrap();
    assert_eq!(admitted.by, Side::Oidc);
    assert_eq!(admitted.principal.id().as_str(), PRINCIPAL);
    let elsewhere = Resource::Page(PageName::try_from("elsewhere").unwrap());
    assert_eq!(
        gate.authorize(&admitted, Access::Read, &elsewhere)
            .await
            .unwrap(),
        Decision::Denied,
        "the allow-list denies this page; loopback would allow it"
    );
    let home = Resource::Page(PageName::try_from(HOME).unwrap());
    assert_eq!(
        gate.authorize(&admitted, Access::Read, &home)
            .await
            .unwrap(),
        Decision::Allowed
    );
}

#[tokio::test]
async fn a_credential_neither_side_accepts_is_rejected() {
    let fake = Fake::start().await;
    let (gate, _, _) = two_sided_gate(&fake).await;
    for credential in [
        Credential::bearer("not-the-token"),
        Credential::session("no-such-session"),
    ] {
        assert_eq!(
            gate.authenticate(&credential).await,
            Err(AuthError::Rejected)
        );
    }
}

#[tokio::test]
async fn a_request_after_the_oidc_session_ends_gets_401() {
    let (server, fake, clock) = oidc::oidc_server().await;
    let session = oidc::sign_in(&server, &fake).await;
    let page = format!("/pages/{HOME}");
    assert_eq!(server.get(&page).cookie(&session).send().await.status, 200);
    clock.advance(Duration::from_secs(12 * 60 * 60 + 1));
    let after = server.get(&page).cookie(&session).send().await;
    assert_eq!(after.status, 401, "{}", after.body);
}
