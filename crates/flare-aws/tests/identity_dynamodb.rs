//! How a table names people: the marker that records the identifier kind
//! on first start and refuses a changed kind, items written before
//! usernames reading back as the same person, and a username's items
//! keeping the verified email beside it.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use aws_sdk_dynamodb::types::AttributeValue;
use flare_aws::{IdentityError, Stores};
use flare_core::{
    Digest, Email, IdentityKind, LoginCode, NewSession, NewToken, Person, Principal, PrincipalId,
    SESSION_IDLE, SessionRule, SessionStore, Team, TeamName, TeamState, TeamStore, Timestamp,
    TokenId, TokenLabel, TokenStore,
};
use stand_in::{dynamodb_client, endpoints, fresh_config};
use std::collections::HashMap;

fn text(value: &str) -> AttributeValue {
    AttributeValue::S(value.to_owned())
}

fn number(value: u64) -> AttributeValue {
    AttributeValue::N(value.to_string())
}

/// Every item of `table`, keyed by its `(pk, sk)`.
async fn items(table: &str) -> HashMap<(String, String), HashMap<String, AttributeValue>> {
    let scanned = dynamodb_client()
        .scan()
        .table_name(table)
        .send()
        .await
        .unwrap();
    scanned
        .items()
        .iter()
        .map(|item| {
            let pk = item["pk"].as_s().unwrap().clone();
            let sk = item["sk"].as_s().unwrap().clone();
            ((pk, sk), item.clone())
        })
        .collect()
}

async fn put(table: &str, item: Vec<(&str, AttributeValue)>) {
    let mut request = dynamodb_client().put_item().table_name(table);
    for (name, value) in item {
        request = request.item(name, value);
    }
    request.send().await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_table_records_the_kind_of_its_first_start_and_refuses_another() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.identity(IdentityKind::Username).await, Ok(()));
    let marker =
        &items(config.table().as_str()).await[&("IDENTITY".to_owned(), "IDENTITY".to_owned())];
    assert_eq!(marker["kind"].as_s().unwrap(), "username");
    assert_eq!(stores.identity(IdentityKind::Username).await, Ok(()));
    assert_eq!(
        stores.identity(IdentityKind::Email).await,
        Err(IdentityError::Changed {
            recorded: IdentityKind::Username
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_new_table_started_under_the_email_kind_refuses_a_later_username_start() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    assert_eq!(stores.identity(IdentityKind::Email).await, Ok(()));
    assert_eq!(
        stores.identity(IdentityKind::Username).await,
        Err(IdentityError::Changed {
            recorded: IdentityKind::Email
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_table_written_before_the_marker_is_recorded_as_email_and_refuses_usernames() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let ana = Person::try_from("ana@example.com").unwrap();
    let team = Team::new(
        TeamName::try_from("design").unwrap(),
        ana,
        Timestamp::from_secs(1),
        TeamState::Active,
        1,
    );
    stores.teams.create(&team).await.unwrap();
    assert_eq!(
        stores.identity(IdentityKind::Username).await,
        Err(IdentityError::Changed {
            recorded: IdentityKind::Email
        })
    );
    assert_eq!(stores.identity(IdentityKind::Email).await, Ok(()));
}

#[tokio::test(flavor = "multi_thread")]
async fn two_first_starts_together_agree_on_one_kind() {
    let config = fresh_config().await;
    let first = Stores::connect_at(&config, &endpoints());
    let second = Stores::connect_at(&config, &endpoints());
    let (one, two) = tokio::join!(
        first.identity(IdentityKind::Username),
        second.identity(IdentityKind::Username)
    );
    assert_eq!((one, two), (Ok(()), Ok(())));
    assert_eq!(
        first.identity(IdentityKind::Email).await,
        Err(IdentityError::Changed {
            recorded: IdentityKind::Username
        })
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_token_and_a_session_written_before_usernames_read_back_as_the_same_person() {
    let config = fresh_config().await;
    let table = config.table().as_str();
    let stores = Stores::connect_at(&config, &endpoints());
    let id = TokenId::from_random([1; 16]);
    put(
        table,
        vec![
            ("pk", text(&format!("TOKEN#{}", id.as_str()))),
            ("sk", text("TOKEN")),
            ("digest", AttributeValue::B(vec![2; 32].into())),
            ("owner", text("ana@example.com")),
            ("label", text("laptop")),
            ("created", number(10)),
            ("expires", number(1_000_000)),
        ],
    )
    .await;
    let token = stores.tokens.get(&id).await.unwrap();
    let ana = Email::try_from("ana@example.com").unwrap();
    assert_eq!(token.owner(), &Person::Email(ana.clone()));
    assert_eq!(token.email(), Some(&ana));
    let digest = Digest::from_bytes([3; 32]);
    put(
        table,
        vec![
            ("pk", text(&format!("SESS#{}", digest.to_hex()))),
            ("sk", text("SESS")),
            ("principal", text("ana@example.com")),
            ("email", text("ana@example.com")),
            ("created", number(1_000)),
            ("expires", number(2_000_000)),
            ("expires_at", number(2_000_000)),
        ],
    )
    .await;
    let rule = SessionRule::new(SESSION_IDLE);
    let principal = stores
        .sessions
        .principal(&digest, &rule, Timestamp::from_secs(1_001))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(principal.person(), Some(&Person::Email(ana.clone())));
    assert_eq!(principal, Principal::with_email(ana.principal_id(), ana));
}

#[tokio::test(flavor = "multi_thread")]
async fn under_the_email_kind_a_token_code_and_session_write_no_new_attribute() {
    let config = fresh_config().await;
    let table = config.table().as_str();
    let stores = Stores::connect_at(&config, &endpoints());
    let ana = Email::try_from("ana@example.com").unwrap();
    let token = NewToken::new(
        TokenId::from_random([4; 16]),
        Digest::from_bytes([4; 32]),
        Person::Email(ana.clone()),
        TokenLabel::try_from("laptop").unwrap(),
        Timestamp::from_secs(10),
    );
    stores.tokens.create(token).await.unwrap();
    let code = LoginCode::new(
        Digest::from_bytes([5; 32]),
        Person::Email(ana.clone()),
        4000,
        Timestamp::from_secs(10),
    );
    stores.tokens.issue_code(code).await.unwrap();
    let principal = Principal::with_email(ana.principal_id(), ana);
    let session = NewSession::new(Digest::from_bytes([6; 32]), principal);
    let rule = SessionRule::new(SESSION_IDLE);
    stores
        .sessions
        .create(session, None, &rule, Timestamp::from_secs(10))
        .await
        .unwrap();
    for item in items(table).await.values() {
        assert!(!item.contains_key("person"), "{item:?}");
        let pk = item["pk"].as_s().unwrap();
        if pk.starts_with("TOKEN#") || pk.starts_with("CODE#") {
            assert!(!item.contains_key("email"), "{item:?}");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_username_token_code_and_session_keep_the_verified_email_beside_the_username() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let ana = Person::try_from("ana").unwrap();
    let email = Email::try_from("ana@example.com").unwrap();
    let id = TokenId::from_random([7; 16]);
    let token = NewToken::new(
        id.clone(),
        Digest::from_bytes([7; 32]),
        ana.clone(),
        TokenLabel::try_from("laptop").unwrap(),
        Timestamp::from_secs(10),
    )
    .with_email(Some(email.clone()));
    stores.tokens.create(token.clone()).await.unwrap();
    assert_eq!(stores.tokens.get(&id).await, Ok(token.stored()));
    let code = LoginCode::new(
        Digest::from_bytes([8; 32]),
        ana.clone(),
        4000,
        Timestamp::from_secs(10),
    )
    .with_email(Some(email.clone()));
    stores.tokens.issue_code(code.clone()).await.unwrap();
    let used = stores
        .tokens
        .use_code(code.digest(), Timestamp::from_secs(11))
        .await
        .unwrap();
    assert_eq!(used, code.used());
    assert_eq!(used.email, Some(email.clone()));
    let principal = Principal::with_person(PrincipalId::try_from("ana").unwrap(), ana, Some(email));
    let digest = Digest::from_bytes([9; 32]);
    let rule = SessionRule::new(SESSION_IDLE);
    stores
        .sessions
        .create(
            NewSession::new(digest, principal.clone()),
            None,
            &rule,
            Timestamp::from_secs(1_000),
        )
        .await
        .unwrap();
    let read = stores
        .sessions
        .principal(&digest, &rule, Timestamp::from_secs(1_001))
        .await
        .unwrap();
    assert_eq!(read, Some(principal));
}
