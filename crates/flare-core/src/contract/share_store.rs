//! The [`ShareStore`] contract suite. The clauses about limits, the owner's
//! list and calls that run together live in submodules, and this module re-exports
//! them.
//!
//! Every function creates the shares it needs through the store, so the
//! caller passes a new, empty store.

mod images;
mod limits;
mod lists;
mod races;

pub use images::{an_html_share_keeps_its_file, each_version_keeps_its_image_table};
pub use limits::{the_share_limit_per_owner_holds, the_version_limit_holds};
pub use lists::{
    a_share_is_listed_once_created, the_owners_list_holds_only_their_shares_newest_first,
};
pub use races::{
    concurrent_creates_of_one_id_give_one_id_taken, concurrent_publishes_get_consecutive_versions,
};

use super::{Clause, People, Violation, ensure, fixture, unexpected};
use crate::{
    DocumentText, NewShare, NewVersion, PrincipalId, ShareContent, ShareId, ShareKind, ShareStore,
    ShareStoreError, ShareTitle, ShareVersion, Timestamp,
};

/// The owners, ids and contents that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ShareFixture {
    owner: PrincipalId,
    other_owner: PrincipalId,
    title: ShareTitle,
    other_title: ShareTitle,
    contents: [ShareContent; 3],
}

impl ShareFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Self::of(People::Emails)
    }

    /// The fixture with `people` as the people it names.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn of(people: People) -> Result<Self, Violation> {
        let document =
            |text: &str| fixture(DocumentText::try_from(text)).map(ShareContent::Document);
        Ok(Self {
            owner: people.id("owner")?,
            other_owner: people.id("other")?,
            title: fixture(ShareTitle::try_from("Contract notes"))?,
            other_title: fixture(ShareTitle::try_from("Another title"))?,
            contents: [
                document("# First\n")?,
                document("# Second\n")?,
                document("# Third\n")?,
            ],
        })
    }

    /// The content numbered `index`, of three.
    fn content(&self, index: usize) -> &ShareContent {
        let [first, second, third] = &self.contents;
        match index % 3 {
            0 => first,
            1 => second,
            _ => third,
        }
    }

    /// A new share of `owner` with the fixture's title and first content.
    fn new_share(&self, id: &ShareId, owner: &PrincipalId, at: u64) -> NewShare {
        NewShare::new(
            id.clone(),
            owner.clone(),
            self.title.clone(),
            self.content(0).clone(),
            Timestamp::from_secs(at),
        )
    }

    /// A version with content `index`, published by the owner at `at`.
    fn version(&self, index: usize, at: u64) -> NewVersion {
        NewVersion::new(
            self.content(index).clone(),
            self.owner.clone(),
            Timestamp::from_secs(at),
        )
    }
}

/// The share id that `tag` and `number` give, so each clause uses ids of
/// its own.
fn numbered_id(tag: u8, number: u32) -> ShareId {
    let mut bytes = [tag; 16];
    for (slot, byte) in bytes.iter_mut().skip(12).zip(number.to_be_bytes()) {
        *slot = byte;
    }
    ShareId::from_random(bytes)
}

/// A version number from a fixture constant.
fn number(value: u32) -> Result<ShareVersion, Violation> {
    fixture(ShareVersion::try_from(value))
}

/// A created share reads back with its owner, title, kind, creation time
/// and version 1, from `create` and from `get`.
///
/// # Errors
///
/// Returns [`Clause::ACreatedShareReadsBack`] otherwise.
pub async fn a_created_share_reads_back<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(1, 0);
    let new = contract_fixture.new_share(&id, &contract_fixture.owner, 10);
    let created = unexpected(store.create(new.clone()).await)?;
    let read = unexpected(store.get(&id).await)?;
    let expected = new.share();
    ensure(
        created == expected
            && read == expected
            && read.owner() == &contract_fixture.owner
            && read.kind() == ShareKind::Document
            && read.latest() == ShareVersion::first(),
        Clause::ACreatedShareReadsBack,
        || format!("create gave {created:?}, get gave {read:?}"),
    )
}

/// The first version of a created share holds the content, publisher and
/// time it was created with.
///
/// # Errors
///
/// Returns [`Clause::TheFirstVersionHoldsTheCreatedContent`] otherwise.
pub async fn the_first_version_holds_the_created_content<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(2, 0);
    let new = contract_fixture.new_share(&id, &contract_fixture.owner, 10);
    unexpected(store.create(new.clone()).await)?;
    let first = store.version(&id, ShareVersion::first()).await;
    ensure(
        first.as_ref() == Ok(&new.first_version()),
        Clause::TheFirstVersionHoldsTheCreatedContent,
        || format!("version 1 is {first:?}"),
    )
}

/// Creating a share with a taken id answers `IdTaken` and leaves the first
/// share and its version 1 unchanged.
///
/// # Errors
///
/// Returns [`Clause::ATakenShareIdChangesNothing`] otherwise.
pub async fn a_taken_share_id_changes_nothing<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(3, 0);
    let first = contract_fixture.new_share(&id, &contract_fixture.owner, 10);
    unexpected(store.create(first.clone()).await)?;
    let second = NewShare::new(
        id.clone(),
        contract_fixture.other_owner.clone(),
        contract_fixture.other_title.clone(),
        contract_fixture.content(1).clone(),
        Timestamp::from_secs(20),
    );
    let answer = store.create(second).await;
    let share = unexpected(store.get(&id).await)?;
    let version = unexpected(store.version(&id, ShareVersion::first()).await)?;
    ensure(
        answer == Err(ShareStoreError::IdTaken)
            && share == first.share()
            && version == first.first_version(),
        Clause::ATakenShareIdChangesNothing,
        || format!("create answered {answer:?}; the share is {share:?} with {version:?}"),
    )
}

/// Publishing gives the next version number, moves the share's latest to
/// it, and the new version reads back.
///
/// # Errors
///
/// Returns [`Clause::PublishGivesTheNextVersion`] otherwise.
pub async fn publish_gives_the_next_version<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(4, 0);
    unexpected(
        store
            .create(contract_fixture.new_share(&id, &contract_fixture.owner, 10))
            .await,
    )?;
    let version = contract_fixture.version(1, 20);
    let answer = store.publish(&id, version.clone()).await;
    let share = unexpected(store.get(&id).await)?;
    let second = number(2)?;
    let read = store.version(&id, second).await;
    ensure(
        answer == Ok(second)
            && share.latest() == second
            && read == Ok(version.shared(id.clone(), second)),
        Clause::PublishGivesTheNextVersion,
        || {
            format!(
                "publish answered {answer:?}; latest {:?}; read {read:?}",
                share.latest()
            )
        },
    )
}

/// A publish leaves every older version unchanged, with its content,
/// publisher and time.
///
/// # Errors
///
/// Returns [`Clause::APublishLeavesOlderVersionsUnchanged`] otherwise.
pub async fn a_publish_leaves_older_versions_unchanged<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(5, 0);
    let new = contract_fixture.new_share(&id, &contract_fixture.owner, 10);
    unexpected(store.create(new.clone()).await)?;
    let second = contract_fixture.version(1, 20);
    unexpected(store.publish(&id, second.clone()).await)?;
    unexpected(store.publish(&id, contract_fixture.version(2, 30)).await)?;
    let one = store.version(&id, ShareVersion::first()).await;
    let two = store.version(&id, number(2)?).await;
    ensure(
        one == Ok(new.first_version()) && two == Ok(second.shared(id.clone(), number(2)?)),
        Clause::APublishLeavesOlderVersionsUnchanged,
        || format!("version 1 is {one:?}, version 2 is {two:?}"),
    )
}

/// Every version from 1 to the latest reads back with its own number.
///
/// # Errors
///
/// Returns [`Clause::EveryVersionReadsBack`] otherwise.
pub async fn every_version_reads_back<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(6, 0);
    unexpected(
        store
            .create(contract_fixture.new_share(&id, &contract_fixture.owner, 10))
            .await,
    )?;
    for index in 1..5_u32 {
        let at = 10 + u64::from(index);
        let content = usize::try_from(index).unwrap_or_default();
        unexpected(
            store
                .publish(&id, contract_fixture.version(content, at))
                .await,
        )?;
    }
    let latest = unexpected(store.get(&id).await)?.latest();
    ensure(latest == number(5)?, Clause::EveryVersionReadsBack, || {
        format!("five versions give latest {latest:?}")
    })?;
    for value in 1..=latest.get() {
        let wanted = number(value)?;
        let read = store.version(&id, wanted).await;
        ensure(
            read.as_ref()
                .is_ok_and(|version| version.number() == wanted),
            Clause::EveryVersionReadsBack,
            || format!("version {value} reads as {read:?}"),
        )?;
    }
    Ok(())
}

/// Publishing to an unknown share answers `NotFound` and creates nothing.
///
/// # Errors
///
/// Returns [`Clause::PublishingToAnUnknownShareCreatesNothing`] otherwise.
pub async fn publishing_to_an_unknown_share_creates_nothing<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let missing = numbered_id(7, 0);
    let answer = store
        .publish(&missing, contract_fixture.version(1, 20))
        .await;
    let share = store.get(&missing).await;
    let version = store.version(&missing, ShareVersion::first()).await;
    let listed = unexpected(store.owned_by(&contract_fixture.owner).await)?;
    ensure(
        answer == Err(ShareStoreError::NotFound)
            && share == Err(ShareStoreError::NotFound)
            && version == Err(ShareStoreError::NotFound)
            && listed.items().is_empty(),
        Clause::PublishingToAnUnknownShareCreatesNothing,
        || format!("publish answered {answer:?}; get {share:?}; version {version:?}"),
    )
}

/// An unknown share, and a version above the latest, answer `NotFound`.
///
/// # Errors
///
/// Returns [`Clause::AnUnknownShareOrVersionIsNotFound`] otherwise.
pub async fn an_unknown_share_or_version_is_not_found<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let missing = numbered_id(8, 1);
    let unknown = store.get(&missing).await;
    let unknown_version = store.version(&missing, ShareVersion::first()).await;
    let id = numbered_id(8, 0);
    unexpected(
        store
            .create(contract_fixture.new_share(&id, &contract_fixture.owner, 10))
            .await,
    )?;
    let above = store.version(&id, number(2)?).await;
    ensure(
        unknown == Err(ShareStoreError::NotFound)
            && unknown_version == Err(ShareStoreError::NotFound)
            && above == Err(ShareStoreError::NotFound),
        Clause::AnUnknownShareOrVersionIsNotFound,
        || format!("get {unknown:?}; version {unknown_version:?}; version 2 {above:?}"),
    )
}

/// A created share always has its version 1, which reads back after later
/// publishes, and a version above the latest answers `NotFound`.
///
/// # Errors
///
/// Returns [`Clause::AShareAlwaysHasItsFirstVersion`] otherwise.
pub async fn a_share_always_has_its_first_version<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
) -> Result<(), Violation> {
    let id = numbered_id(11, 0);
    let new = contract_fixture.new_share(&id, &contract_fixture.owner, 10);
    unexpected(store.create(new.clone()).await)?;
    let clause = Clause::AShareAlwaysHasItsFirstVersion;
    for round in 0..2_u32 {
        let share = unexpected(store.get(&id).await)?;
        let first = store.version(&id, ShareVersion::first()).await;
        let above = match share.latest().next() {
            Some(next) => store.version(&id, next).await,
            None => Err(ShareStoreError::NotFound),
        };
        ensure(
            first == Ok(new.first_version()) && above == Err(ShareStoreError::NotFound),
            clause,
            || format!("round {round}: version 1 {first:?}, above latest {above:?}"),
        )?;
        unexpected(store.publish(&id, contract_fixture.version(1, 20)).await)?;
    }
    Ok(())
}
