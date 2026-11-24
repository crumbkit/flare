//! The `Shares` service over the in-memory stores: who may do what to a
//! share, and what each refusal looks like.
#![cfg(test)]

#[path = "support/sharing_stores.rs"]
mod sharing_stores;

use flare_core::contract::block_on;
use flare_core::memory::{MemoryFileStore, MemoryGrantStore, MemoryShareStore};
use flare_core::{
    DocumentText, Email, FileBytes, FileId, FileStore, MAX_GRANTS, MAX_SHARES_PER_OWNER,
    MAX_VERSIONS, MediaType, NewFile, Principal, PrincipalId, Role, ShareContent, ShareError,
    ShareId, ShareTitle, ShareVersion, Shares, Standing, Timestamp,
};
use sharing_stores::{
    BrokenFiles, BrokenGrants, BrokenShares, InterleavingGrants, Shared, VersionlessShares,
};
use std::sync::Arc;

type Memory = Shares<MemoryShareStore, MemoryGrantStore>;

fn email(text: &str) -> Email {
    Email::try_from(text).unwrap()
}

/// A person who signed in with `address`, as sign-in builds them.
fn person(address: &str) -> Principal {
    Principal::with_email(PrincipalId::try_from(address).unwrap(), email(address))
}

fn alice() -> Principal {
    person("alice@example.com")
}

fn id(byte: u8) -> ShareId {
    ShareId::from_random([byte; 16])
}

fn doc(text: &str) -> ShareContent {
    ShareContent::Document(DocumentText::try_from(text).unwrap())
}

fn title() -> ShareTitle {
    ShareTitle::try_from("Notes").unwrap()
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

fn memory() -> Memory {
    Shares::new(MemoryShareStore::new(), MemoryGrantStore::new())
}

/// A service in which alice owns share 1 with one version.
fn with_share() -> Memory {
    let shares = memory();
    block_on(shares.create(&alice(), id(1), title(), doc("# One\n"), at(1))).unwrap();
    shares
}

fn grant(shares: &Memory, who: &Principal, grantee: &str, role: Role) {
    block_on(shares.grant(who, &id(1), &email(grantee), role, at(2))).unwrap();
}

/// Every request a person can make about `share`, as its answer or its
/// refusal.
fn every_request(shares: &Memory, who: &Principal, share: &ShareId) -> Vec<Option<ShareError>> {
    let files = MemoryFileStore::new();
    let file = FileId::from_random([9; 16]);
    let owner = email("alice@example.com");
    let other = email("zed@example.com");
    let own = who.email().cloned().unwrap();
    block_on(async {
        vec![
            shares.open(who, share, None).await.err(),
            shares
                .open(who, share, Some(ShareVersion::first()))
                .await
                .err(),
            shares.grants(who, share).await.err(),
            shares
                .publish(who, share, doc("# Two\n"), at(3))
                .await
                .err(),
            shares
                .grant(who, share, &other, Role::Reader, at(3))
                .await
                .err(),
            shares
                .grant(who, share, &owner, Role::Admin, at(3))
                .await
                .err(),
            shares.revoke(who, share, &other).await.err(),
            shares.revoke(who, share, &owner).await.err(),
            shares.revoke(who, share, &own).await.err(),
            shares.file(who, &files, share, &file).await.err(),
        ]
    })
}

#[test]
fn a_person_with_no_grant_gets_not_found_equal_to_the_answer_for_a_missing_share() {
    let shares = with_share();
    let stranger = person("eve@example.com");
    let refused = block_on(shares.open(&stranger, &id(1), None)).unwrap_err();
    let missing = block_on(shares.open(&stranger, &id(2), None)).unwrap_err();
    assert_eq!(refused, ShareError::NotFound);
    assert_eq!(refused, missing);
    // The admitted twin: the owner opens it.
    assert!(block_on(shares.open(&alice(), &id(1), None)).is_ok());
}

#[test]
fn a_stranger_gets_the_same_not_found_for_every_request_on_a_share_that_exists_or_not() {
    let shares = with_share();
    let stranger = person("eve@example.com");
    for share in [id(1), id(2)] {
        let answers = every_request(&shares, &stranger, &share);
        assert!(
            answers
                .iter()
                .all(|answer| *answer == Some(ShareError::NotFound)),
            "{answers:?}"
        );
    }
    assert!(
        block_on(shares.grants(&alice(), &id(1)))
            .unwrap()
            .is_empty()
    );
}

#[test]
fn a_reader_revoking_a_granted_address_an_ungranted_one_and_the_owner_gets_three_equal_forbidden() {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    grant(&shares, &alice(), "carol@example.com", Role::Reader);
    let bob = person("bob@example.com");
    let answers: Vec<_> = ["carol@example.com", "zed@example.com", "alice@example.com"]
        .map(|address| block_on(shares.revoke(&bob, &id(1), &email(address))))
        .to_vec();
    assert_eq!(answers, vec![Err(ShareError::Forbidden); 3]);
    // The admitted twin: the owner revokes carol.
    assert!(block_on(shares.revoke(&alice(), &id(1), &email("carol@example.com"))).is_ok());
}

#[test]
fn a_reader_opens_the_latest_version_and_an_older_one() {
    let shares = with_share();
    block_on(shares.publish(&alice(), &id(1), doc("# Two\n"), at(5))).unwrap();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let bob = person("bob@example.com");
    let latest = block_on(shares.open(&bob, &id(1), None)).unwrap();
    assert_eq!(latest.version.number().get(), 2);
    assert_eq!(latest.version.content(), &doc("# Two\n"));
    assert_eq!(latest.standing, Standing::Reader);
    let older = block_on(shares.open(&bob, &id(1), Some(ShareVersion::first()))).unwrap();
    assert_eq!(older.version.content(), &doc("# One\n"));
}

#[test]
fn a_reader_asking_for_a_version_above_the_latest_gets_version_not_found_and_a_stranger_gets_not_found()
 {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let above = Some(ShareVersion::try_from(2).unwrap());
    let reader = block_on(shares.open(&person("bob@example.com"), &id(1), above));
    let stranger = block_on(shares.open(&person("eve@example.com"), &id(1), above));
    assert_eq!(reader.unwrap_err(), ShareError::VersionNotFound);
    assert_eq!(stranger.unwrap_err(), ShareError::NotFound);
}

#[test]
fn a_reader_whose_version_read_fails_gets_unavailable() {
    let grant_store = Shared(Arc::new(MemoryGrantStore::new()));
    let share_store = Shared(Arc::new(MemoryShareStore::new()));
    let working = Shares::new(share_store.clone(), grant_store.clone());
    block_on(working.create(&alice(), id(1), title(), doc("# One\n"), at(1))).unwrap();
    block_on(working.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Reader,
        at(2),
    ))
    .unwrap();
    let failing = Shares::new(VersionlessShares(share_store), grant_store);
    let bob = person("bob@example.com");
    let latest = block_on(failing.open(&bob, &id(1), None));
    let first = block_on(failing.open(&bob, &id(1), Some(ShareVersion::first())));
    assert_eq!(latest.unwrap_err(), ShareError::Unavailable);
    assert_eq!(first.unwrap_err(), ShareError::Unavailable);
}

#[test]
fn a_revoked_reader_is_refused_on_the_next_open_with_no_new_sign_in() {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let bob = person("bob@example.com");
    assert!(block_on(shares.open(&bob, &id(1), None)).is_ok());
    block_on(shares.revoke(&alice(), &id(1), &email("bob@example.com"))).unwrap();
    assert_eq!(
        block_on(shares.open(&bob, &id(1), None)).unwrap_err(),
        ShareError::NotFound
    );
}

#[test]
fn an_admin_grants_a_reader_and_is_refused_granting_an_admin_revoking_an_admin_and_revoking_the_owner()
 {
    let shares = with_share();
    grant(&shares, &alice(), "dave@example.com", Role::Admin);
    grant(&shares, &alice(), "erin@example.com", Role::Admin);
    let dave = person("dave@example.com");
    let granted = block_on(shares.grant(
        &dave,
        &id(1),
        &email("bob@example.com"),
        Role::Reader,
        at(3),
    ));
    assert_eq!(granted.unwrap().granted_by().as_str(), "dave@example.com");
    let as_admin = block_on(shares.grant(
        &dave,
        &id(1),
        &email("carol@example.com"),
        Role::Admin,
        at(3),
    ));
    let demote = block_on(shares.grant(
        &dave,
        &id(1),
        &email("erin@example.com"),
        Role::Reader,
        at(3),
    ));
    let revoke_admin = block_on(shares.revoke(&dave, &id(1), &email("erin@example.com")));
    let revoke_owner = block_on(shares.revoke(&dave, &id(1), &email("alice@example.com")));
    assert_eq!(as_admin.unwrap_err(), ShareError::Forbidden);
    assert_eq!(demote.unwrap_err(), ShareError::Forbidden);
    assert_eq!(revoke_admin.unwrap_err(), ShareError::Forbidden);
    assert_eq!(revoke_owner.unwrap_err(), ShareError::OwnerCannotBeChanged);
    // The admitted twin: the admin revokes the reader it granted.
    assert!(block_on(shares.revoke(&dave, &id(1), &email("bob@example.com"))).is_ok());
}

#[test]
fn a_reader_is_refused_when_granting_revoking_listing_grants_or_publishing() {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    grant(&shares, &alice(), "carol@example.com", Role::Reader);
    let bob = person("bob@example.com");
    let answers = block_on(async {
        [
            shares
                .grant(&bob, &id(1), &email("zed@example.com"), Role::Reader, at(3))
                .await
                .err(),
            shares
                .revoke(&bob, &id(1), &email("carol@example.com"))
                .await
                .err(),
            shares.grants(&bob, &id(1)).await.err(),
            shares
                .publish(&bob, &id(1), doc("# Two\n"), at(3))
                .await
                .err(),
        ]
    });
    assert_eq!(answers, [Some(ShareError::Forbidden); 4]);
    assert!(block_on(shares.open(&bob, &id(1), None)).is_ok());
}

#[test]
fn an_admin_cannot_publish_and_the_owner_can() {
    let shares = with_share();
    grant(&shares, &alice(), "dave@example.com", Role::Admin);
    let refused =
        block_on(shares.publish(&person("dave@example.com"), &id(1), doc("# Two\n"), at(3)));
    assert_eq!(refused.unwrap_err(), ShareError::Forbidden);
    let published = block_on(shares.publish(&alice(), &id(1), doc("# Two\n"), at(3)));
    assert_eq!(published.unwrap().get(), 2);
}

#[test]
fn the_owner_promotes_a_reader_to_admin_and_demotes_the_admin() {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let promoted = block_on(shares.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Admin,
        at(5),
    ));
    assert_eq!(promoted.unwrap().role(), Role::Admin);
    let bob = person("bob@example.com");
    assert_eq!(
        block_on(shares.open(&bob, &id(1), None)).unwrap().standing,
        Standing::Admin
    );
    let demoted = block_on(shares.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Reader,
        at(6),
    ));
    let demoted = demoted.unwrap();
    assert_eq!(demoted.role(), Role::Reader);
    assert_eq!(demoted.created(), at(2));
}

#[test]
fn a_reader_leaves_an_admin_leaves_and_a_stranger_who_tries_to_leave_gets_not_found() {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    grant(&shares, &alice(), "dave@example.com", Role::Admin);
    for address in ["bob@example.com", "dave@example.com"] {
        let leaver = person(address);
        assert!(block_on(shares.revoke(&leaver, &id(1), &email(address))).is_ok());
        assert_eq!(
            block_on(shares.open(&leaver, &id(1), None)).unwrap_err(),
            ShareError::NotFound
        );
    }
    let eve = person("eve@example.com");
    assert_eq!(
        block_on(shares.revoke(&eve, &id(1), &email("eve@example.com"))).unwrap_err(),
        ShareError::NotFound
    );
    let owner_leaves = block_on(shares.revoke(&alice(), &id(1), &email("alice@example.com")));
    assert_eq!(owner_leaves.unwrap_err(), ShareError::OwnerCannotBeChanged);
}

type Interleaving = Shared<InterleavingGrants<Shared<MemoryGrantStore>>>;
type Plain = Shares<Shared<MemoryShareStore>, Shared<MemoryGrantStore>>;

/// A service whose grant store runs an armed action between its reads and
/// its write, the handle that arms it, and a second service on the same
/// stores for the action. Alice owns share 1.
fn interleaved() -> (
    Shares<Shared<MemoryShareStore>, Interleaving>,
    Interleaving,
    Arc<Plain>,
) {
    let share_store = Shared(Arc::new(MemoryShareStore::new()));
    let grant_store = Shared(Arc::new(MemoryGrantStore::new()));
    let interleaving = Shared(Arc::new(InterleavingGrants::new(grant_store.clone())));
    let other = Arc::new(Shares::new(share_store.clone(), grant_store));
    block_on(other.create(&alice(), id(1), title(), doc("# One\n"), at(1))).unwrap();
    (
        Shares::new(share_store, interleaving.clone()),
        interleaving,
        other,
    )
}

#[test]
fn a_revoked_admin_cannot_grant_and_a_grant_by_an_admin_revoked_between_read_and_write_is_refused()
{
    let (shares, arming, other) = interleaved();
    block_on(other.grant(
        &alice(),
        &id(1),
        &email("dave@example.com"),
        Role::Admin,
        at(2),
    ))
    .unwrap();
    let dave = person("dave@example.com");
    let racing = Arc::clone(&other);
    arming.0.arm(move || {
        block_on(racing.revoke(&alice(), &id(1), &email("dave@example.com"))).unwrap();
    });
    let answer = block_on(shares.grant(
        &dave,
        &id(1),
        &email("carol@example.com"),
        Role::Reader,
        at(3),
    ));
    assert_eq!(answer.unwrap_err(), ShareError::Forbidden);
    let carol = block_on(other.grants(&alice(), &id(1))).unwrap();
    assert!(carol.is_empty(), "{carol:?}");
    let later = block_on(shares.grant(
        &dave,
        &id(1),
        &email("carol@example.com"),
        Role::Reader,
        at(4),
    ));
    assert_eq!(later.unwrap_err(), ShareError::NotFound);
}

#[test]
fn an_admin_demoted_between_read_and_revoke_is_refused_and_the_readers_grant_remains() {
    let (shares, arming, other) = interleaved();
    for (grantee, role) in [
        ("dave@example.com", Role::Admin),
        ("bob@example.com", Role::Reader),
    ] {
        block_on(other.grant(&alice(), &id(1), &email(grantee), role, at(2))).unwrap();
    }
    let racing = Arc::clone(&other);
    arming.0.arm(move || {
        block_on(racing.grant(
            &alice(),
            &id(1),
            &email("dave@example.com"),
            Role::Reader,
            at(3),
        ))
        .unwrap();
    });
    let dave = person("dave@example.com");
    let answer = block_on(shares.revoke(&dave, &id(1), &email("bob@example.com")));
    assert_eq!(answer, Err(ShareError::Forbidden));
    let bob = block_on(other.grants(&alice(), &id(1)))
        .unwrap()
        .into_iter()
        .find(|grant| grant.grantee().as_str() == "bob@example.com")
        .map(|grant| grant.role());
    assert_eq!(bob, Some(Role::Reader));
}

#[test]
fn a_grant_that_changed_between_the_services_read_and_write_answers_grant_changed_and_writes_nothing()
 {
    let (shares, arming, other) = interleaved();
    block_on(other.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Reader,
        at(2),
    ))
    .unwrap();
    let racing = Arc::clone(&other);
    arming.0.arm(move || {
        block_on(racing.revoke(&alice(), &id(1), &email("bob@example.com"))).unwrap();
    });
    let answer = block_on(shares.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Admin,
        at(3),
    ));
    assert_eq!(answer.unwrap_err(), ShareError::GrantChanged);
    assert!(block_on(other.grants(&alice(), &id(1))).unwrap().is_empty());
    // The admitted twin: with nothing in between, the promotion applies.
    block_on(other.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Reader,
        at(4),
    ))
    .unwrap();
    assert!(
        block_on(shares.grant(
            &alice(),
            &id(1),
            &email("bob@example.com"),
            Role::Admin,
            at(5)
        ))
        .is_ok()
    );
}

#[test]
fn the_grant_the_version_and_the_share_limits_each_answer_their_error_and_write_nothing() {
    let shares = with_share();
    for number in 0..MAX_GRANTS {
        let address = format!("p{number}@example.com");
        grant(&shares, &alice(), &address, Role::Reader);
    }
    let over = block_on(shares.grant(
        &alice(),
        &id(1),
        &email("zed@example.com"),
        Role::Reader,
        at(3),
    ));
    assert_eq!(over.unwrap_err(), ShareError::TooManyGrants);
    assert_eq!(
        block_on(shares.grants(&alice(), &id(1))).unwrap().len(),
        MAX_GRANTS
    );

    for second in 2..=u64::from(MAX_VERSIONS) {
        block_on(shares.publish(&alice(), &id(1), doc("# Again\n"), at(second))).unwrap();
    }
    let too_many = block_on(shares.publish(&alice(), &id(1), doc("# Over\n"), at(999)));
    assert_eq!(too_many.unwrap_err(), ShareError::TooManyVersions);
    let opened = block_on(shares.open(&alice(), &id(1), None)).unwrap();
    assert_eq!(opened.share.latest().get(), MAX_VERSIONS);

    let owner = person("owen@example.com");
    for number in 0..MAX_SHARES_PER_OWNER {
        let mut bytes = [7; 16];
        bytes[..8].copy_from_slice(&u64::try_from(number).unwrap().to_be_bytes());
        let share = ShareId::from_random(bytes);
        block_on(shares.create(&owner, share, title(), doc("# S\n"), at(1))).unwrap();
    }
    let refused = block_on(shares.create(&owner, id(0xEE), title(), doc("# S\n"), at(2)));
    assert_eq!(refused.unwrap_err(), ShareError::TooManyShares);
    assert_eq!(
        block_on(shares.mine(&owner)).unwrap().items().len(),
        MAX_SHARES_PER_OWNER
    );
    let taken = block_on(shares.create(&alice(), id(1), title(), doc("# S\n"), at(2)));
    assert_eq!(taken.unwrap_err(), ShareError::IdTaken);
}

#[test]
fn create_and_publish_record_the_calling_principal_as_owner_and_publisher() {
    let shares = memory();
    // The principal's own id differs from its verified address; the share
    // records the address.
    let alice = Principal::with_email(
        PrincipalId::try_from("google:1234").unwrap(),
        email("Alice@Example.com"),
    );
    let share = block_on(shares.create(&alice, id(1), title(), doc("# One\n"), at(1))).unwrap();
    assert_eq!(share.owner().as_str(), "alice@example.com");
    block_on(shares.publish(&alice, &id(1), doc("# Two\n"), at(2))).unwrap();
    let opened = block_on(shares.open(&alice, &id(1), None)).unwrap();
    assert_eq!(opened.version.published_by().as_str(), "alice@example.com");
    assert_eq!(opened.standing, Standing::Owner);
}

#[test]
fn a_principal_with_no_verified_email_gets_no_verified_email_on_every_method() {
    let shares = with_share();
    let nameless = Principal::new(PrincipalId::try_from("alice@example.com").unwrap());
    let files = MemoryFileStore::new();
    let other = email("bob@example.com");
    let file = FileId::from_random([1; 16]);
    let answers = block_on(async {
        [
            shares
                .create(&nameless, id(2), title(), doc("# X\n"), at(1))
                .await
                .err(),
            shares.open(&nameless, &id(1), None).await.err(),
            shares
                .publish(&nameless, &id(1), doc("# X\n"), at(1))
                .await
                .err(),
            shares
                .grant(&nameless, &id(1), &other, Role::Reader, at(1))
                .await
                .err(),
            shares.revoke(&nameless, &id(1), &other).await.err(),
            shares.grants(&nameless, &id(1)).await.err(),
            shares.mine(&nameless).await.err(),
            shares.shared_with_me(&nameless).await.err(),
            shares.file(&nameless, &files, &id(1), &file).await.err(),
        ]
    });
    assert_eq!(answers, [Some(ShareError::NoVerifiedEmail); 9]);
    assert!(block_on(shares.open(&alice(), &id(1), None)).is_ok());
}

#[test]
fn a_principal_whose_config_id_is_bob_and_whose_email_is_mallory_opens_mallorys_share_and_not_bobs()
{
    let shares = memory();
    for (byte, grantee) in [(1, "bob@y.com"), (2, "mallory@x.com")] {
        block_on(shares.create(&alice(), id(byte), title(), doc("# S\n"), at(1))).unwrap();
        block_on(shares.grant(&alice(), &id(byte), &email(grantee), Role::Reader, at(2))).unwrap();
    }
    let mallory = Principal::with_email(
        PrincipalId::try_from("bob@y.com").unwrap(),
        email("mallory@x.com"),
    );
    assert_eq!(
        block_on(shares.open(&mallory, &id(1), None)).unwrap_err(),
        ShareError::NotFound
    );
    assert!(block_on(shares.open(&mallory, &id(2), None)).is_ok());
}

#[test]
fn a_grant_to_a_mixed_case_address_opens_the_share_for_the_lower_case_sign_in() {
    let shares = with_share();
    grant(&shares, &alice(), "Bob@Example.COM", Role::Reader);
    assert!(block_on(shares.open(&person("bob@example.com"), &id(1), None)).is_ok());
}

#[test]
fn a_grant_to_a_dotted_address_does_not_open_the_share_for_the_undotted_one() {
    let shares = with_share();
    grant(&shares, &alice(), "a.lice@x.com", Role::Reader);
    assert_eq!(
        block_on(shares.open(&person("alice@x.com"), &id(1), None)).unwrap_err(),
        ShareError::NotFound
    );
    assert!(block_on(shares.open(&person("a.lice@x.com"), &id(1), None)).is_ok());
}

#[test]
fn a_reader_reads_a_file_of_the_share_and_gets_not_found_for_another_shares_file_and_a_stranger_gets_not_found()
 {
    let shares = with_share();
    block_on(shares.create(&alice(), id(2), title(), doc("# Two\n"), at(1))).unwrap();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let files = MemoryFileStore::new();
    let png = MediaType::try_from("image/png").unwrap();
    let bytes = FileBytes::try_from(b"png".to_vec()).unwrap();
    let (own, others) = (FileId::from_random([1; 16]), FileId::from_random([2; 16]));
    block_on(files.put(NewFile::new(id(1), own.clone(), png.clone(), bytes.clone()))).unwrap();
    block_on(files.put(NewFile::new(id(2), others.clone(), png, bytes))).unwrap();
    let bob = person("bob@example.com");
    let read = block_on(shares.file(&bob, &files, &id(1), &own)).unwrap();
    assert_eq!(read.bytes().as_bytes(), b"png");
    let across = block_on(shares.file(&bob, &files, &id(1), &others));
    assert_eq!(across.unwrap_err(), ShareError::NotFound);
    let other_share = block_on(shares.file(&bob, &files, &id(2), &others));
    assert_eq!(other_share.unwrap_err(), ShareError::NotFound);
    let stranger = block_on(shares.file(&person("eve@example.com"), &files, &id(1), &own));
    assert_eq!(stranger.unwrap_err(), ShareError::NotFound);
}

#[test]
fn every_failing_store_gives_unavailable_and_never_gives_access() {
    let bob = person("bob@example.com");
    let other = email("carol@example.com");
    let file = FileId::from_random([1; 16]);

    // Every share store method fails, for the owner and for a reader.
    let grant_store = Shared(Arc::new(MemoryGrantStore::new()));
    let working = Shares::new(MemoryShareStore::new(), grant_store.clone());
    block_on(working.create(&alice(), id(1), title(), doc("# One\n"), at(1))).unwrap();
    block_on(working.grant(
        &alice(),
        &id(1),
        &email("bob@example.com"),
        Role::Reader,
        at(2),
    ))
    .unwrap();
    let broken = Shares::new(BrokenShares, grant_store);
    let files = MemoryFileStore::new();
    for who in [alice(), bob.clone()] {
        let answers = block_on(async {
            [
                broken
                    .create(&who, id(3), title(), doc("# X\n"), at(1))
                    .await
                    .err(),
                broken.open(&who, &id(1), None).await.err(),
                broken
                    .publish(&who, &id(1), doc("# X\n"), at(1))
                    .await
                    .err(),
                broken
                    .grant(&who, &id(1), &other, Role::Reader, at(1))
                    .await
                    .err(),
                broken.revoke(&who, &id(1), &other).await.err(),
                broken.grants(&who, &id(1)).await.err(),
                broken.mine(&who).await.err(),
                broken.shared_with_me(&bob).await.err(),
                broken.file(&who, &files, &id(1), &file).await.err(),
            ]
        });
        assert_eq!(answers, [Some(ShareError::Unavailable); 9]);
    }

    // Every grant store method fails: no one but the owner gets in, and
    // nothing that reads a grant succeeds.
    let broken = Shares::new(MemoryShareStore::new(), BrokenGrants);
    block_on(broken.create(&alice(), id(1), title(), doc("# One\n"), at(1))).unwrap();
    let answers = block_on(async {
        [
            broken.open(&bob, &id(1), None).await.err(),
            broken
                .publish(&bob, &id(1), doc("# X\n"), at(1))
                .await
                .err(),
            broken
                .grant(&alice(), &id(1), &other, Role::Reader, at(1))
                .await
                .err(),
            broken.revoke(&alice(), &id(1), &other).await.err(),
            broken.grants(&alice(), &id(1)).await.err(),
            broken.shared_with_me(&bob).await.err(),
            broken.file(&bob, &files, &id(1), &file).await.err(),
        ]
    });
    assert_eq!(answers, [Some(ShareError::Unavailable); 7]);

    // The file store fails.
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let failed = block_on(shares.file(&bob, &BrokenFiles, &id(1), &file));
    assert_eq!(failed.unwrap_err(), ShareError::Unavailable);
}

#[test]
fn the_share_list_shows_owned_shares_only_and_shared_with_me_shows_granted_shares_and_drops_a_revoked_one()
 {
    let shares = with_share();
    let bob = person("bob@example.com");
    block_on(shares.create(&bob, id(2), title(), doc("# Bob's\n"), at(1))).unwrap();
    block_on(shares.create(&alice(), id(3), title(), doc("# Three\n"), at(4))).unwrap();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    block_on(shares.grant(
        &alice(),
        &id(3),
        &email("bob@example.com"),
        Role::Admin,
        at(5),
    ))
    .unwrap();

    let mine: Vec<ShareId> = block_on(shares.mine(&alice()))
        .unwrap()
        .items()
        .iter()
        .map(|share| share.id().clone())
        .collect();
    assert_eq!(mine, [id(3), id(1)]);

    let shared = block_on(shares.shared_with_me(&bob)).unwrap();
    let seen: Vec<(ShareId, Role)> = shared
        .items()
        .iter()
        .map(|entry| (entry.share.id().clone(), entry.role))
        .collect();
    assert_eq!(seen, [(id(3), Role::Admin), (id(1), Role::Reader)]);

    block_on(shares.revoke(&alice(), &id(1), &email("bob@example.com"))).unwrap();
    let shared = block_on(shares.shared_with_me(&bob)).unwrap();
    assert_eq!(shared.items().len(), 1);
    assert_eq!(shared.items()[0].share.id(), &id(3));
}

#[test]
fn the_debug_output_of_an_opened_share_and_a_shared_with_me_entry_holds_no_address() {
    let shares = with_share();
    grant(&shares, &alice(), "bob@example.com", Role::Reader);
    let bob = person("bob@example.com");
    let opened = block_on(shares.open(&bob, &id(1), None)).unwrap();
    let shared = block_on(shares.shared_with_me(&bob)).unwrap();
    let grants = block_on(shares.grants(&alice(), &id(1))).unwrap();
    for debug in [
        format!("{opened:?}"),
        format!("{shared:?}"),
        format!("{grants:?}"),
    ] {
        assert!(!debug.contains("example.com"), "{debug}");
    }
}
