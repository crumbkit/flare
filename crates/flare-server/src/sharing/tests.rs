//! Unit tests of the token check against the admit rule, of the share
//! and version checks, of what an admin sees of a team's grant, and of the
//! get flare address a reading page links to.

use super::tokens::{digest, new_secret, token_text};
use super::{Api, Call, GetFlare, Sharing, TokenCheck};
use flare_auth_oidc::{Admit, AllowEntry, AllowList, Reach};
use flare_contract::{ApiCode, Format, VersionRequest};
use flare_core::memory::MemoryTeamStore;
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore, MemoryShareStore,
    MemoryTokenStore,
};
use flare_core::{
    AllAdmitted, ByIdentity, DocumentText, Email, FlareTeams, Flares, Group, IdentityKind,
    NewFlare, NewToken, Person, Principal, PrincipalId, Role, ShareContent, ShareId, ShareTitle,
    TeamName, Timestamp, TokenId, TokenLabel, TokenStore as _,
};
use secrecy::SecretString;

const NOW: Timestamp = Timestamp::from_secs(1_800_000_000);

fn email(text: &str) -> Email {
    Email::try_from(text).unwrap()
}

/// An admit rule that lists `addresses`.
fn listing(addresses: &[&str]) -> Admit {
    let entries = addresses
        .iter()
        .map(|address| {
            AllowEntry::new(
                email(address).into(),
                PrincipalId::try_from(*address).unwrap(),
                Reach::Everything,
            )
        })
        .collect();
    Admit::Emails(AllowList::new(entries).unwrap())
}

/// A token of `owner`: its id, its secret and the token's text.
struct Minted {
    id: TokenId,
    secret: SecretString,
    owner: Email,
    text: SecretString,
}

fn mint(id: &str, owner: &str) -> Minted {
    let id = TokenId::try_from(id).unwrap();
    let secret = new_secret().unwrap();
    let text = SecretString::from(token_text(&id, &secret));
    Minted {
        id,
        secret,
        owner: email(owner),
        text,
    }
}

/// The share API over a token store that holds `tokens`, admitting the
/// people `admit` names, as a server started with that rule runs it.
async fn sharing(
    tokens: &[&Minted],
    admit: Admit,
) -> Sharing<
    MemoryShareStore,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    MemoryTokenStore,
    ByIdentity,
> {
    let store = MemoryTokenStore::default();
    for minted in tokens {
        let token = NewToken::new(
            minted.id.clone(),
            digest(&minted.secret),
            minted.owner.clone().into(),
            TokenLabel::try_from("test").unwrap(),
            NOW,
        );
        store.create(token).await.unwrap();
    }
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    Sharing::new(flares, store, ByIdentity(IdentityKind::Email), admit)
}

fn owner_of(check: TokenCheck) -> Option<Email> {
    match check {
        TokenCheck::Principal(principal) => principal.email().cloned(),
        _ => None,
    }
}

#[tokio::test]
async fn a_token_whose_owner_left_the_admit_list_answers_sign_in_needed_and_a_listed_owner_still_signs_in()
 {
    let ana = mint("0123456789abcdef0123456789abcdef", "ana@example.org");
    let bob = mint("fedcba9876543210fedcba9876543210", "bob@example.org");
    let before = sharing(
        &[&ana, &bob],
        listing(&["ana@example.org", "bob@example.org"]),
    )
    .await;
    assert_eq!(
        owner_of(before.authenticate(&ana.text, NOW).await),
        Some(ana.owner.clone())
    );

    // The operator takes ana off the list and restarts the server.
    let after = sharing(&[&ana, &bob], listing(&["bob@example.org"])).await;
    assert!(matches!(
        after.authenticate(&ana.text, NOW).await,
        TokenCheck::Refused(ApiCode::SignInNeeded)
    ));
    // The admitted twin: bob, still on the list, signs in with his token.
    assert_eq!(
        owner_of(after.authenticate(&bob.text, NOW).await),
        Some(bob.owner.clone())
    );
}

#[tokio::test]
async fn an_admin_is_refused_a_new_version_and_the_owner_publishes_it() {
    let ana = Principal::with_email(
        email("ana@example.org").principal_id(),
        email("ana@example.org"),
    );
    let bob = Principal::with_email(
        email("bob@example.org").principal_id(),
        email("bob@example.org"),
    );
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    let id = ShareId::try_from("0123456789abcdef0123456789abcdef").unwrap();
    let text = DocumentText::try_from("# Plan\n").unwrap();
    let new = NewFlare::new(
        id.clone(),
        ShareTitle::try_from("Plan").unwrap(),
        ShareContent::Document(text),
        Vec::new(),
        Vec::new(),
    );
    flares.share(&ana, new, NOW).await.unwrap();
    flares
        .shares()
        .grant(
            &ana,
            &id,
            &Person::from(email("bob@example.org")),
            Role::Admin,
            NOW,
        )
        .await
        .unwrap();
    let api = Sharing::new(
        flares,
        MemoryTokenStore::default(),
        ByIdentity(IdentityKind::Email),
        listing(&["ana@example.org", "bob@example.org"]),
    );
    let post = || {
        Call::Publish(
            id.as_str().to_owned(),
            VersionRequest {
                format: Format::Markdown,
                content: "# Plan 2\n".to_owned(),
            },
        )
    };
    let refused = api.run(&bob, post(), NOW).await.unwrap_err();
    assert_eq!(refused.code, ApiCode::ShareForbidden);
    let (status, _) = api.run(&ana, post(), NOW).await.unwrap();
    assert_eq!(status, 201);
}

fn share_request(title: &str, format: Format, content: &str) -> flare_contract::ShareRequest {
    flare_contract::ShareRequest {
        title: title.to_owned(),
        format,
        content: content.to_owned(),
        with: Vec::new(),
        re: Vec::new(),
    }
}

#[test]
fn the_share_checks_refuse_html_first_then_the_title_then_the_content() {
    let code = |request| super::check_share(&request).map(|_| ()).map_err(|e| e.code);
    assert_eq!(
        code(share_request("", Format::Html, "")),
        Err(ApiCode::KindNotServed)
    );
    assert_eq!(
        code(share_request("", Format::Markdown, "")),
        Err(ApiCode::ShareInvalid)
    );
    assert_eq!(
        code(share_request(
            "Plan",
            Format::Markdown,
            &"x".repeat(262_145)
        )),
        Err(ApiCode::ShareTooLarge)
    );
    assert_eq!(
        code(share_request("Plan", Format::Markdown, "# Plan\n")),
        Ok(())
    );
}

#[test]
fn the_version_checks_refuse_html_and_empty_content_and_admit_markdown() {
    let code = |format, content: &str| {
        super::check_version(&VersionRequest {
            format,
            content: content.to_owned(),
        })
        .map(|_| ())
        .map_err(|e| e.code)
    };
    assert_eq!(code(Format::Html, "<p>x</p>"), Err(ApiCode::KindNotServed));
    assert_eq!(code(Format::Markdown, ""), Err(ApiCode::ShareInvalid));
    assert_eq!(code(Format::Markdown, "# Plan\n"), Ok(()));
}

#[tokio::test]
async fn a_share_admin_outside_a_team_sees_its_grant_as_a_placeholder_in_the_list_and_the_reading_view()
 {
    let person =
        |address: &str| Principal::with_email(email(address).principal_id(), email(address));
    let (ana, bob) = (person("ana@example.org"), person("bob@example.org"));
    let flares = Flares::new(
        MemoryShareStore::new(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    )
    .with_teams(FlareTeams::new(MemoryTeamStore::new()));
    let design = TeamName::try_from("design").unwrap();
    flares.create_team(&ana, design.clone(), NOW).await.unwrap();
    let id = ShareId::try_from("0123456789abcdef0123456789abcdef").unwrap();
    let new = NewFlare::new(
        id.clone(),
        ShareTitle::try_from("Plan").unwrap(),
        ShareContent::Document(DocumentText::try_from("# Plan\n").unwrap()),
        Vec::new(),
        Vec::new(),
    )
    .with_groups(vec![Group::Team(design)]);
    flares.share(&ana, new, NOW).await.unwrap();
    flares
        .shares()
        .grant(
            &ana,
            &id,
            &Person::from(email("bob@example.org")),
            Role::Admin,
            NOW,
        )
        .await
        .unwrap();
    let api = Sharing::new(
        flares,
        MemoryTokenStore::default(),
        ByIdentity(IdentityKind::Email),
        listing(&["ana@example.org", "bob@example.org"]),
    );
    let grantees = |answer: Option<String>| -> Vec<String> {
        let list: serde_json::Value = serde_json::from_str(&answer.unwrap()).unwrap();
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|entry| entry["kind"] == "team")
            .map(|entry| entry["grantee"].as_str().unwrap().to_owned())
            .collect()
    };
    let call = || Call::ListGrants(id.as_str().to_owned());
    let (_, by_admin) = api.run(&bob, call(), NOW).await.unwrap();
    assert_eq!(grantees(by_admin), vec!["team:*".to_owned()]);
    let (_, by_owner) = api.run(&ana, call(), NOW).await.unwrap();
    assert_eq!(grantees(by_owner), vec!["team:design".to_owned()]);
    let read_by_admin = api.read(&bob, id.as_str(), None).await.unwrap();
    assert_eq!(
        read_by_admin.with,
        Some(vec!["bob@example.org".to_owned(), "team:*".to_owned()])
    );
    let read_by_owner = api.read(&ana, id.as_str(), None).await.unwrap();
    assert_eq!(
        read_by_owner.with,
        Some(vec!["bob@example.org".to_owned(), "team:design".to_owned()])
    );
}

/// A share store that counts `owned_by` reads, and fails them while
/// `fail_owned` is set. Every other call passes through.
#[derive(Debug, Default, Clone)]
struct Counting {
    inner: std::sync::Arc<MemoryShareStore>,
    owned_reads: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    fail_owned: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl flare_core::ShareStore for Counting {
    async fn create(
        &self,
        new: flare_core::NewShare,
    ) -> Result<flare_core::Share, flare_core::ShareStoreError> {
        self.inner.create(new).await
    }
    async fn get(&self, id: &ShareId) -> Result<flare_core::Share, flare_core::ShareStoreError> {
        self.inner.get(id).await
    }
    async fn publish(
        &self,
        id: &ShareId,
        version: flare_core::NewVersion,
    ) -> Result<flare_core::ShareVersion, flare_core::ShareStoreError> {
        self.inner.publish(id, version).await
    }
    async fn version(
        &self,
        id: &ShareId,
        number: flare_core::ShareVersion,
    ) -> Result<flare_core::SharedVersion, flare_core::ShareStoreError> {
        self.inner.version(id, number).await
    }
    async fn owned_by(
        &self,
        owner: &PrincipalId,
    ) -> Result<flare_core::Listing<flare_core::Share>, flare_core::ShareStoreError> {
        use std::sync::atomic::Ordering;
        self.owned_reads.fetch_add(1, Ordering::SeqCst);
        if self.fail_owned.load(Ordering::SeqCst) {
            return Err(flare_core::ShareStoreError::Unavailable);
        }
        self.inner.owned_by(owner).await
    }
}

const GET_FLARE: &str = "https://example.com/flare";

fn person(address: &str) -> Principal {
    Principal::with_email(email(address).principal_id(), email(address))
}

/// Shares a Markdown flare titled `title` as `owner`, granted to `with`,
/// and returns its id.
async fn share_as(
    flares: &Flares<
        Counting,
        MemoryGrantStore,
        MemoryResponseStore,
        MemoryCommentStore,
        MemoryCursorStore,
    >,
    owner: &str,
    id: &str,
    with: &[&str],
) -> ShareId {
    let id = ShareId::try_from(id).unwrap();
    let new = NewFlare::new(
        id.clone(),
        ShareTitle::try_from("Plan").unwrap(),
        ShareContent::Document(DocumentText::try_from("# Plan\n").unwrap()),
        with.iter().map(|address| email(address).into()).collect(),
        Vec::new(),
    );
    flares.share(&person(owner), new, NOW).await.unwrap();
    id
}

/// A share API where ana shared one flare with bob, its id and the store,
/// linking to `get_flare` when given.
async fn badged(
    get_flare: Option<&str>,
) -> (
    Sharing<
        Counting,
        MemoryGrantStore,
        MemoryResponseStore,
        MemoryCommentStore,
        MemoryCursorStore,
        MemoryTokenStore,
        ByIdentity,
    >,
    ShareId,
    Counting,
) {
    let store = Counting::default();
    let flares = Flares::new(
        store.clone(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        AllAdmitted,
    );
    let id = share_as(
        &flares,
        "ana@example.org",
        "0123456789abcdef0123456789abcdef",
        &["bob@example.org"],
    )
    .await;
    let api = Sharing::new(
        flares,
        MemoryTokenStore::default(),
        ByIdentity(IdentityKind::Email),
        listing(&["ana@example.org", "bob@example.org"]),
    );
    let api = match get_flare {
        Some(text) => api.with_get_flare(GetFlare::try_from(text).unwrap()),
        None => api,
    };
    (api, id, store)
}

fn owned_reads(store: &Counting) -> usize {
    store.owned_reads.load(std::sync::atomic::Ordering::SeqCst)
}

#[tokio::test]
async fn a_reader_who_has_never_shared_gets_the_get_flare_address() {
    let (api, id, _) = badged(Some(GET_FLARE)).await;
    let bob = person("bob@example.org");
    let reading = api.read(&bob, id.as_str(), None).await.unwrap();
    assert_eq!(reading.badge.as_deref(), Some(GET_FLARE));
}

#[tokio::test]
async fn a_reader_who_owns_a_share_gets_no_get_flare_address() {
    let (api, id, _) = badged(Some(GET_FLARE)).await;
    let bob = person("bob@example.org");
    api.run(
        &bob,
        Call::Create(share_request("Mine", Format::Markdown, "# Mine\n")),
        NOW,
    )
    .await
    .unwrap();
    let reading = api.read(&bob, id.as_str(), None).await.unwrap();
    assert_eq!(reading.badge, None);
}

#[tokio::test]
async fn the_owner_gets_no_get_flare_address_and_their_shares_are_not_read() {
    let (api, id, store) = badged(Some(GET_FLARE)).await;
    let ana = person("ana@example.org");
    let reading = api.read(&ana, id.as_str(), None).await.unwrap();
    assert!(reading.own);
    assert_eq!(reading.badge, None);
    assert_eq!(owned_reads(&store), 0);
    // The reader twin: bob's read does list his shares.
    api.read(&person("bob@example.org"), id.as_str(), None)
        .await
        .unwrap();
    assert_eq!(owned_reads(&store), 1);
}

#[tokio::test]
async fn a_site_with_no_get_flare_address_gives_no_reader_one_and_reads_no_shares() {
    let (api, id, store) = badged(None).await;
    let reading = api
        .read(&person("bob@example.org"), id.as_str(), None)
        .await
        .unwrap();
    assert_eq!(reading.badge, None);
    assert_eq!(owned_reads(&store), 0);
}

#[tokio::test]
async fn a_store_that_cannot_list_the_readers_shares_still_gives_the_page_with_no_address() {
    let (api, id, store) = badged(Some(GET_FLARE)).await;
    store
        .fail_owned
        .store(true, std::sync::atomic::Ordering::SeqCst);
    let reading = api
        .read(&person("bob@example.org"), id.as_str(), None)
        .await
        .unwrap();
    assert_eq!(reading.badge, None);
    assert_eq!(reading.title, "Plan");
    store
        .fail_owned
        .store(false, std::sync::atomic::Ordering::SeqCst);
    let reading = api
        .read(&person("bob@example.org"), id.as_str(), None)
        .await
        .unwrap();
    assert_eq!(reading.badge.as_deref(), Some(GET_FLARE));
}

#[test]
fn a_get_flare_address_must_be_an_https_url_with_a_host() {
    for refused in [
        "http://example.com/flare",
        "/flare",
        "example.com/flare",
        "not a url",
        "https://",
        "https:///flare",
        "https://user:secret@example.com/flare",
        "javascript:alert(1)",
        "",
    ] {
        assert!(GetFlare::try_from(refused).is_err(), "{refused}");
    }
    for admitted in [
        "https://example.com",
        "https://example.com/flare",
        "https://example.com/flare?from=page#get",
    ] {
        assert!(GetFlare::try_from(admitted).is_ok(), "{admitted}");
    }
    assert_eq!(
        GetFlare::try_from(GET_FLARE).unwrap().as_str(),
        "https://example.com/flare"
    );
}
