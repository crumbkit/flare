//! Paging the shares granted to a caller with everyone grants included,
//! when some everyone grants were written under an admit rule that no
//! longer opens them to the caller.
#![cfg(test)]

use flare_core::contract::block_on;
use flare_core::memory::{MemoryGrantStore, MemoryShareStore};
use flare_core::{
    Admission, DocumentText, Email, Group, MailDomain, Principal, PrincipalId, Role, ShareContent,
    ShareId, ShareTitle, Shares, Timestamp,
};

fn email(text: &str) -> Email {
    Email::try_from(text).unwrap()
}

fn person(address: &str) -> Principal {
    Principal::with_email(PrincipalId::try_from(address).unwrap(), email(address))
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn domain() -> Admission {
    Admission::Domain(MailDomain::try_from("example.com").unwrap())
}

#[test]
fn a_walk_of_shares_with_me_lists_every_open_everyone_grant_when_an_unopened_one_fills_a_read() {
    let listed =
        Shares::new(MemoryShareStore::new(), MemoryGrantStore::new()).admitting(Admission::Listed);
    let alice = person("alice@example.com");
    let content = || ShareContent::Document(DocumentText::try_from("# S\n").unwrap());
    let title = || ShareTitle::try_from("Notes").unwrap();
    for byte in 1..=4 {
        block_on(listed.create(&alice, id(byte), title(), content(), at(1))).unwrap();
    }
    // Share 1 opened to everyone the email list admitted, newest of all.
    // After the deployment moves to the domain rule, that grant opens it
    // to nobody.
    block_on(listed.grant_group(&alice, &id(1), &Group::Everyone, at(60))).unwrap();
    let domain_wide = listed.admitting(domain());
    block_on(domain_wide.grant_group(&alice, &id(2), &Group::Everyone, at(50))).unwrap();
    block_on(domain_wide.grant_group(&alice, &id(3), &Group::Everyone, at(40))).unwrap();
    block_on(domain_wide.grant(
        &alice,
        &id(4),
        &email("bob@example.com"),
        Role::Reader,
        at(10),
    ))
    .unwrap();

    let bob = person("bob@example.com");
    let mut seen = Vec::new();
    let mut after = None;
    for _ in 0..10 {
        let page =
            block_on(domain_wide.shared_with_me_page(&bob, true, after.as_ref(), 2)).unwrap();
        seen.extend(page.items.iter().map(|entry| entry.share.id().clone()));
        match page.next {
            Some(key) => after = Some(key),
            None => break,
        }
    }
    assert_eq!(seen, [id(2), id(3), id(4)]);
}
