//! What the session and sealing key stores show on a durable table, beyond
//! the contract suites: a second store on the same table sees every write,
//! each item carries its expiry in the table's TTL attribute, an end of all
//! of a person's sessions leaves no item behind, sign-ins that run together
//! keep the person at 8 sessions, and a lost answer or a dropped request
//! behaves as the stores promise.
#![cfg(test)]

#[path = "support/stand_in.rs"]
#[expect(
    clippy::missing_panics_doc,
    reason = "the stand-in support is test support: it panics on a broken fixture"
)]
pub mod stand_in;

use flare_aws::{Fault, Stores};
use flare_core::{
    Digest, Email, KEY_GRACE, KeyBytes, KeyId, NewSession, Principal, SESSION_IDLE, SealingKeys,
    SessionRule, SessionStore, SessionStoreError, Timestamp,
};
use stand_in::{dynamodb_client, endpoints, fresh_config};

const START: u64 = 2_000_000;

fn person(address: &str) -> Principal {
    let email = Email::try_from(address).unwrap();
    Principal::with_email(email.principal_id(), email)
}

fn digest(tag: u8) -> Digest {
    Digest::from_bytes([tag; 32])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

const RULE: SessionRule = SessionRule::new(SESSION_IDLE);

fn rule() -> SessionRule {
    RULE
}

/// The `(pk, sk, expires_at)` of every item that has a TTL attribute.
async fn expiring(table: &str) -> Vec<(String, String, String)> {
    let scanned = dynamodb_client()
        .scan()
        .table_name(table)
        .send()
        .await
        .unwrap();
    let mut found: Vec<_> = scanned
        .items()
        .iter()
        .filter_map(|item| {
            let ttl = item.get(stand_in::TTL_ATTRIBUTE)?.as_n().ok()?.clone();
            let pk = item.get("pk")?.as_s().ok()?.clone();
            let sk = item.get("sk")?.as_s().ok()?.clone();
            Some((pk, sk, ttl))
        })
        .collect();
    found.sort();
    found
}

async fn items_of(table: &str, prefix: &str) -> usize {
    let scanned = dynamodb_client()
        .scan()
        .table_name(table)
        .send()
        .await
        .unwrap();
    scanned
        .items()
        .iter()
        .filter(|item| {
            item.get("pk")
                .and_then(|value| value.as_s().ok())
                .is_some_and(|pk| pk.starts_with(prefix))
        })
        .count()
}

#[tokio::test(flavor = "multi_thread")]
async fn a_second_store_on_the_same_table_sees_a_session_and_its_removal() {
    let config = fresh_config().await;
    let first = Stores::connect_at(&config, &endpoints());
    let second = Stores::connect_at(&config, &endpoints());
    let ana = person("ana@example.com");
    first
        .sessions
        .create(
            NewSession::new(digest(1), ana.clone()),
            None,
            &rule(),
            at(START),
        )
        .await
        .unwrap();
    let seen = second
        .sessions
        .principal(&digest(1), &rule(), at(START))
        .await;
    assert_eq!(seen, Ok(Some(ana.clone())));
    assert_eq!(second.sessions.holds(ana.id(), at(START)).await, Ok(true));
    second.sessions.remove(&digest(1)).await.unwrap();
    assert_eq!(
        first
            .sessions
            .principal(&digest(1), &rule(), at(START))
            .await,
        Ok(None)
    );
    assert_eq!(items_of(config.table().as_str(), "SESS#").await, 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_and_its_link_carry_the_expiry_in_the_tables_ttl_attribute() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let ana = person("ana@example.com");
    let expires = stores
        .sessions
        .create(
            NewSession::new(digest(1), ana.clone()),
            None,
            &rule(),
            at(START),
        )
        .await
        .unwrap();
    assert_eq!(expires, at(START + SESSION_IDLE));
    let found = expiring(config.table().as_str()).await;
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .all(|(_, _, ttl)| *ttl == expires.as_secs().to_string())
    );
    assert!(found.iter().any(|(pk, _, _)| pk.starts_with("SESS#")));
    assert!(found.iter().any(|(pk, _, _)| pk.starts_with("SESSOF#")));
    // A later request moves the TTL attribute and the link with the expiry.
    let later = at(START + SESSION_IDLE - 60);
    stores
        .sessions
        .principal(&digest(1), &rule(), later)
        .await
        .unwrap();
    let moved = stores
        .sessions
        .expiry(&digest(1), later)
        .await
        .unwrap()
        .unwrap();
    assert!(moved > expires);
    let found = expiring(config.table().as_str()).await;
    assert_eq!(found.len(), 2, "{found:?}");
    assert!(
        found
            .iter()
            .all(|(_, _, ttl)| *ttl == moved.as_secs().to_string())
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_every_session_of_a_person_leaves_no_session_or_link_item() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let ana = person("ana@example.com");
    let ben = person("ben@example.com");
    for tag in 1..=8_u8 {
        let now = at(START + u64::from(tag));
        stores
            .sessions
            .create(
                NewSession::new(digest(tag), ana.clone()),
                None,
                &rule(),
                now,
            )
            .await
            .unwrap();
    }
    stores
        .sessions
        .create(
            NewSession::new(digest(100), ben.clone()),
            None,
            &rule(),
            at(START),
        )
        .await
        .unwrap();
    // A session that expired and that the TTL has not deleted yet goes too.
    let expired = at(START + 2 * SESSION_IDLE);
    stores
        .sessions
        .create(
            NewSession::new(digest(9), ana.clone()),
            None,
            &rule(),
            expired,
        )
        .await
        .unwrap();
    stores.sessions.remove_all_of(ana.id()).await.unwrap();
    assert_eq!(stores.sessions.holds(ana.id(), expired).await, Ok(false));
    for tag in 1..=9_u8 {
        let read = stores.sessions.expiry(&digest(tag), at(START)).await;
        assert_eq!(read, Ok(None), "session {tag}");
    }
    assert_eq!(
        stores
            .sessions
            .principal(&digest(100), &rule(), at(START))
            .await,
        Ok(Some(ben))
    );
    let table = config.table().as_str();
    assert_eq!(
        items_of(table, "SESSOF#").await,
        3,
        "ben's link, ben's guard and ana's guard"
    );
    assert_eq!(items_of(table, "SESS#").await, 1, "ben's session");
}

#[tokio::test(flavor = "multi_thread")]
async fn twelve_sign_ins_of_one_person_that_run_together_leave_eight_sessions() {
    let config = fresh_config().await;
    let stores = Stores::connect_at(&config, &endpoints());
    let ana = person("ana@example.com");
    let sign_in = |tag: u8| {
        stores.sessions.create(
            NewSession::new(digest(tag), ana.clone()),
            None,
            &RULE,
            at(START + u64::from(tag)),
        )
    };
    let answers = tokio::join!(
        sign_in(1),
        sign_in(2),
        sign_in(3),
        sign_in(4),
        sign_in(5),
        sign_in(6),
        sign_in(7),
        sign_in(8),
        sign_in(9),
        sign_in(10),
        sign_in(11),
        sign_in(12),
    );
    let answers = [
        answers.0, answers.1, answers.2, answers.3, answers.4, answers.5, answers.6, answers.7,
        answers.8, answers.9, answers.10, answers.11,
    ];
    assert!(answers.iter().all(Result::is_ok), "{answers:?}");
    let mut live = 0;
    for tag in 1..=12_u8 {
        if stores
            .sessions
            .expiry(&digest(tag), at(START + 20))
            .await
            .unwrap()
            .is_some()
        {
            live += 1;
        }
    }
    assert_eq!(live, 8);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_sign_in_whose_answer_is_lost_answers_success_and_one_that_never_arrives_does_not() {
    let config = fresh_config().await;
    let healthy = Stores::connect_at(&config, &endpoints());
    let lossy = Stores::connect_at_with_fault(&config, &endpoints(), Fault::LoseEveryAnswer);
    let dropped = Stores::connect_at_with_fault(&config, &endpoints(), Fault::DropEveryRequest);
    let ana = person("ana@example.com");
    let created = lossy
        .sessions
        .create(
            NewSession::new(digest(1), ana.clone()),
            None,
            &rule(),
            at(START),
        )
        .await;
    assert_eq!(created, Ok(at(START + SESSION_IDLE)));
    assert_eq!(
        healthy
            .sessions
            .principal(&digest(1), &rule(), at(START))
            .await,
        Ok(Some(ana.clone()))
    );
    let never = dropped
        .sessions
        .create(NewSession::new(digest(2), ana), None, &rule(), at(START))
        .await;
    assert_eq!(never, Err(SessionStoreError::Unavailable));
    assert_eq!(
        healthy.sessions.expiry(&digest(2), at(START)).await,
        Ok(None)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_request_whose_extension_fails_still_answers_the_principal_and_leaves_the_expiry() {
    let config = fresh_config().await;
    let healthy = Stores::connect_at(&config, &endpoints());
    let dropped = Stores::connect_at_with_fault(&config, &endpoints(), Fault::DropEveryRequest);
    let ana = person("ana@example.com");
    let expires = healthy
        .sessions
        .create(
            NewSession::new(digest(1), ana.clone()),
            None,
            &rule(),
            at(START),
        )
        .await
        .unwrap();
    let later = at(START + SESSION_IDLE - 60);
    let read = dropped.sessions.principal(&digest(1), &rule(), later).await;
    assert_eq!(read, Ok(Some(ana)));
    assert_eq!(
        healthy.sessions.expiry(&digest(1), later).await,
        Ok(Some(expires))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_rotation_gives_the_old_key_a_ttl_that_ends_its_grace_and_a_second_store_reads_both() {
    let config = fresh_config().await;
    let first = Stores::connect_at(&config, &endpoints());
    let second = Stores::connect_at(&config, &endpoints());
    let one = first
        .keys
        .current(KeyBytes::new([1; 32]), at(START))
        .await
        .unwrap();
    let later = at(START + 31 * 24 * 60 * 60);
    let two = first
        .keys
        .rotate(KeyBytes::new([2; 32]), later)
        .await
        .unwrap();
    assert_eq!(two.id, KeyId::FIRST.next().unwrap());
    assert_eq!(second.keys.get(KeyId::FIRST, later).await, Ok(Some(one)));
    assert_eq!(second.keys.get(two.id, later).await, Ok(Some(two)));
    let found = expiring(config.table().as_str()).await;
    let ends = (later.as_secs() + KEY_GRACE).to_string();
    assert_eq!(
        found,
        vec![("KEY#SIGNIN".to_owned(), "V#0000000001".to_owned(), ends)]
    );
}
