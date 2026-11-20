//! The share store clauses about the limits on shares and versions.

use super::{ShareFixture, number, numbered_id};
use crate::contract::{Clause, RunTogether, Violation, ensure, unexpected};
use crate::{MAX_SHARES_PER_OWNER, MAX_VERSIONS, PrincipalId, ShareStore, ShareStoreError};

/// Creates shares for `owner` until it has `count`, with ids tagged `tag`.
async fn fill_owner<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
    owner: &PrincipalId,
    tag: u8,
    count: usize,
) -> Result<(), Violation> {
    for index in 0..count {
        let at = u64::try_from(index).unwrap_or_default();
        let id = numbered_id(tag, u32::try_from(index).unwrap_or_default());
        unexpected(
            store
                .create(contract_fixture.new_share(&id, owner, at))
                .await,
        )?;
    }
    Ok(())
}

/// A create by an owner with [`MAX_SHARES_PER_OWNER`] shares answers
/// `TooManyShares` and writes nothing, a create by another owner succeeds,
/// and two creates by an owner with one share fewer than the limit, run
/// together by `runner`, give one success and one `TooManyShares`.
///
/// # Errors
///
/// Returns [`Clause::TheShareLimitPerOwnerHolds`] otherwise.
pub async fn the_share_limit_per_owner_holds<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let clause = Clause::TheShareLimitPerOwnerHolds;
    let owner = &contract_fixture.owner;
    fill_owner(store, contract_fixture, owner, 30, MAX_SHARES_PER_OWNER).await?;
    let over = numbered_id(31, 0);
    let refused = store
        .create(contract_fixture.new_share(&over, owner, 9_999))
        .await;
    let written = store.get(&over).await;
    let listed = unexpected(store.owned_by(owner).await)?;
    ensure(
        refused == Err(ShareStoreError::TooManyShares)
            && written == Err(ShareStoreError::NotFound)
            && listed.items().len() == MAX_SHARES_PER_OWNER,
        clause,
        || format!("the create past the limit answered {refused:?}, and get {written:?}"),
    )?;
    let other = &contract_fixture.other_owner;
    let admitted = store
        .create(contract_fixture.new_share(&numbered_id(32, 0), other, 1))
        .await;
    ensure(admitted.is_ok(), clause, || {
        format!("another owner's create answered {admitted:?}")
    })?;
    // The other owner now has one share; bring it to one below the limit.
    let below = MAX_SHARES_PER_OWNER.saturating_sub(2);
    fill_owner(store, contract_fixture, other, 33, below).await?;
    let first = contract_fixture.new_share(&numbered_id(34, 0), other, 20_000);
    let second = contract_fixture.new_share(&numbered_id(34, 1), other, 20_001);
    let answers = runner
        .run_together(
            || store.create(first.clone()),
            || store.create(second.clone()),
        )
        .await;
    let (one, two) = unexpected(answers.ok_or("a concurrent create panicked"))?;
    let listed = unexpected(store.owned_by(other).await)?;
    let one_each = matches!(
        (&one, &two),
        (Ok(_), Err(ShareStoreError::TooManyShares)) | (Err(ShareStoreError::TooManyShares), Ok(_))
    );
    ensure(
        one_each && listed.items().len() == MAX_SHARES_PER_OWNER,
        clause,
        || format!("two creates at the edge answered {one:?} and {two:?}"),
    )
}

/// A publish to a share with [`MAX_VERSIONS`] versions answers
/// `TooManyVersions` and writes nothing, and two publishes to a share with
/// one version fewer, run together by `runner`, give one success and one
/// `TooManyVersions`.
///
/// # Errors
///
/// Returns [`Clause::TheVersionLimitHolds`] otherwise.
pub async fn the_version_limit_holds<S: ShareStore>(
    store: &S,
    contract_fixture: &ShareFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    let clause = Clause::TheVersionLimitHolds;
    let owner = &contract_fixture.owner;
    let full = numbered_id(35, 0);
    unexpected(
        store
            .create(contract_fixture.new_share(&full, owner, 1))
            .await,
    )?;
    for at in 2..=u64::from(MAX_VERSIONS) {
        unexpected(store.publish(&full, contract_fixture.version(1, at)).await)?;
    }
    let refused = store
        .publish(&full, contract_fixture.version(2, 1_000))
        .await;
    let latest = unexpected(store.get(&full).await)?.latest();
    let above = store
        .version(&full, number(MAX_VERSIONS.saturating_add(1))?)
        .await;
    ensure(
        refused == Err(ShareStoreError::TooManyVersions)
            && latest == number(MAX_VERSIONS)?
            && above == Err(ShareStoreError::NotFound),
        clause,
        || format!("the publish past the limit answered {refused:?}; latest {latest:?}"),
    )?;
    let edge = numbered_id(35, 1);
    unexpected(
        store
            .create(contract_fixture.new_share(&edge, owner, 1))
            .await,
    )?;
    for at in 2..u64::from(MAX_VERSIONS) {
        unexpected(store.publish(&edge, contract_fixture.version(1, at)).await)?;
    }
    let answers = runner
        .run_together(
            || store.publish(&edge, contract_fixture.version(1, 2_000)),
            || store.publish(&edge, contract_fixture.version(2, 2_001)),
        )
        .await;
    let (one, two) = unexpected(answers.ok_or("a concurrent publish panicked"))?;
    let latest = unexpected(store.get(&edge).await)?.latest();
    let one_each = matches!(
        (&one, &two),
        (Ok(_), Err(ShareStoreError::TooManyVersions))
            | (Err(ShareStoreError::TooManyVersions), Ok(_))
    );
    ensure(one_each && latest == number(MAX_VERSIONS)?, clause, || {
        format!("two publishes at the edge answered {one:?} and {two:?}; latest {latest:?}")
    })
}
