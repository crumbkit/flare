use super::super::{Api, Call, Sharing};
use flare_auth_oidc::{Admit, Reach};
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemoryShareStore,
    MemoryTokenStore,
};
use flare_core::{
    AddressesOnly, AllAdmitted, DocumentText, Email, Flares, MAX_LISTED, NewFlare, Principal,
    ShareContent, ShareId, ShareTitle, Timestamp,
};
use serde_json::Value;

type Memory = Sharing<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    MemoryTokenStore,
    AddressesOnly,
>;

fn person(address: &str) -> Principal {
    let email = Email::try_from(address).unwrap();
    Principal::with_email(email.principal_id(), email)
}

/// The share API over memory stores in which ana and jo shared `count`
/// flares titled Plan with mei, alternately.
async fn granted_to_mei(count: usize) -> Memory {
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    let mei = Email::try_from("mei@example.org").unwrap();
    for number in 0..count {
        let owner = if number % 2 == 0 {
            "ana@example.org"
        } else {
            "jo@example.org"
        };
        let mut bytes = [0; 16];
        bytes[..8].copy_from_slice(&(u64::try_from(number).unwrap()).to_be_bytes());
        let flare = NewFlare::new(
            ShareId::from_random(bytes),
            ShareTitle::try_from("Plan").unwrap(),
            ShareContent::Document(DocumentText::try_from("# Plan\n").unwrap()),
            vec![mei.clone()],
            Vec::new(),
        );
        let at = Timestamp::from_secs(1_000 + u64::try_from(number).unwrap());
        flares.share(&person(owner), flare, at).await.unwrap();
    }
    let admit = Admit::Anyone(Reach::Everything);
    Sharing::new(flares, MemoryTokenStore::default(), AddressesOnly, admit)
}

async fn search(api: &Memory, text: &str, limit: usize) -> Value {
    let call = Call::Search {
        text: text.to_owned(),
        limit,
        everyone: false,
    };
    let now = Timestamp::from_secs(9_000);
    let (status, body) = api
        .run(&person("mei@example.org"), call, now)
        .await
        .unwrap();
    assert_eq!(status, 200);
    serde_json::from_str(&body.unwrap()).unwrap()
}

#[tokio::test]
async fn a_search_is_partial_when_the_shares_granted_to_the_caller_pass_500() {
    let api = granted_to_mei(MAX_LISTED + 1).await;
    let answer = search(&api, "plan", 500).await;
    assert_eq!(answer["partial"], true);
    assert_eq!(answer["results"].as_array().unwrap().len(), 500);
    assert_eq!(answer["next"], Value::Null);
}

#[tokio::test]
async fn a_search_of_500_granted_shares_is_whole() {
    let api = granted_to_mei(MAX_LISTED).await;
    let answer = search(&api, "plan", 500).await;
    assert_eq!(answer["partial"], false);
    assert_eq!(answer["results"].as_array().unwrap().len(), 500);
}

#[tokio::test]
async fn an_invalid_query_reads_no_store_and_lists_every_error() {
    let api = granted_to_mei(1).await;
    let call = Call::Search {
        text: "sort:new -".to_owned(),
        limit: 50,
        everyone: false,
    };
    let now = Timestamp::from_secs(9_000);
    let refusal = api
        .run(&person("mei@example.org"), call, now)
        .await
        .unwrap_err();
    assert_eq!(refusal.code.text(), "flare::search::invalid");
    let codes: Vec<&str> = refusal
        .errors
        .iter()
        .map(|error| error.code.as_str())
        .collect();
    assert_eq!(
        codes,
        [
            "flare::search::unknown_field",
            "flare::search::nothing_to_exclude"
        ]
    );
}
