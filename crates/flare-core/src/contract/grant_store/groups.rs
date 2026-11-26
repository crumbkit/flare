//! The clause about group grants: a grant to a group keeps the audience it
//! was written with.

use super::{GrantFixture, share, write};
use crate::contract::{Clause, Violation, ensure, fixture, unexpected};
use crate::{Audience, GrantStore, Group, Role};

/// A group grant reads back with its audience from every read.
///
/// # Errors
///
/// Returns a [`Violation`] of [`Clause::AGroupGrantKeepsItsAudience`].
pub async fn a_group_grant_keeps_its_audience<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
) -> Result<(), Violation> {
    // Each audience is written on its own share, so `signed-in`, which no
    // domain spells, round-trips through the grant item and the listing
    // entry beside a domain and `admitted`.
    for (number, text) in [(1, "example.com"), (2, "admitted"), (3, "signed-in")] {
        let audience = fixture(Audience::try_from(text))?;
        kept_audience(store, contract_fixture, &share(9, number), &audience).await?;
    }
    Ok(())
}

/// Writes an everyone grant on `shared` with `audience` and checks that
/// every read keeps it.
async fn kept_audience<G: GrantStore>(
    store: &G,
    contract_fixture: &GrantFixture,
    shared: &crate::ShareId,
    audience: &Audience,
) -> Result<(), Violation> {
    let everyone = Group::Everyone.principal_id();
    let written = write(
        shared,
        &everyone,
        Role::Reader,
        None,
        contract_fixture.by_owner(),
        5,
    )
    .with_audience(Some(audience.clone()));
    unexpected(store.grant(written).await)?;
    let clause = Clause::AGroupGrantKeepsItsAudience;
    let read = unexpected(store.get(shared, &everyone).await)?;
    let kept = read.as_ref().and_then(|grant| grant.audience());
    ensure(kept == Some(audience), clause, || format!("get {read:?}"))?;
    let listed = unexpected(store.of_share(shared).await)?;
    let kept = listed.first().and_then(|grant| grant.audience());
    ensure(kept == Some(audience), clause, || {
        format!("of_share {listed:?}")
    })?;
    let granted = unexpected(store.granted_to(&everyone).await)?;
    let kept = granted
        .items()
        .iter()
        .find(|grant| grant.share() == shared)
        .and_then(|grant| grant.audience());
    ensure(kept == Some(audience), clause, || {
        format!("granted_to {:?}", granted.items())
    })
}
