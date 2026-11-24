//! The [`TokenStore`] contract suite. Every function writes the codes and
//! tokens it needs, so the caller passes a new, empty store.

use super::{Clause, RunTogether, Violation, ensure, fixture, unexpected};
use crate::{
    Digest, Email, LoginCode, NewToken, TOKEN_LIFE, Timestamp, TokenId, TokenLabel, TokenStore,
    TokenStoreError,
};

/// The owners and labels that the suite uses.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct TokenFixture {
    owner: Email,
    other_owner: Email,
    label: TokenLabel,
    other_label: TokenLabel,
}

impl TokenFixture {
    /// The standard fixture.
    ///
    /// # Errors
    ///
    /// Returns a [`Violation`] if a constant of the fixture is refused by its
    /// type, which means this crate is broken.
    pub fn standard() -> Result<Self, Violation> {
        Ok(Self {
            owner: fixture(Email::try_from("owner@example.com"))?,
            other_owner: fixture(Email::try_from("other@example.com"))?,
            label: fixture(TokenLabel::try_from("flare on studio"))?,
            other_label: fixture(TokenLabel::try_from("flare on laptop"))?,
        })
    }

    fn code(&self, tag: u8, issued: u64) -> LoginCode {
        LoginCode::new(
            Digest::from_bytes([tag; 32]),
            self.owner.clone(),
            4000 + u16::from(tag),
            Timestamp::from_secs(issued),
        )
    }

    fn token(&self, tag: u8, owner: &Email, label: &TokenLabel, created: u64) -> NewToken {
        NewToken::new(
            TokenId::from_random([tag; 16]),
            Digest::from_bytes([tag; 32]),
            owner.clone(),
            label.clone(),
            Timestamp::from_secs(created),
        )
    }
}

fn at(seconds: u64) -> Timestamp {
    Timestamp::from_secs(seconds)
}

/// A login code gives its owner and port once, and a second use is refused.
///
/// # Errors
///
/// Returns [`Clause::ACodeIsUsedOnce`] otherwise.
pub async fn a_code_is_used_once<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let code = contract_fixture.code(1, 100);
    unexpected(store.issue_code(code.clone()).await)?;
    let first = store.use_code(code.digest(), at(110)).await;
    ensure(
        first.as_ref().is_ok_and(|used| *used == code.used()),
        Clause::ACodeIsUsedOnce,
        || format!("first use answered {first:?}"),
    )?;
    let second = store.use_code(code.digest(), at(111)).await;
    ensure(
        second == Err(TokenStoreError::CodeRefused),
        Clause::ACodeIsUsedOnce,
        || format!("second use answered {second:?}"),
    )
}

/// A code used before its expiry works, and one used at or after its
/// expiry is refused.
///
/// # Errors
///
/// Returns [`Clause::AnExpiredCodeIsRefused`] otherwise.
pub async fn an_expired_code_is_refused<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let late = contract_fixture.code(2, 100);
    let timely = contract_fixture.code(3, 100);
    unexpected(store.issue_code(late.clone()).await)?;
    unexpected(store.issue_code(timely.clone()).await)?;
    let refused = store.use_code(late.digest(), late.expires()).await;
    let used = store
        .use_code(timely.digest(), at(late.expires().as_secs() - 1))
        .await;
    ensure(
        refused == Err(TokenStoreError::CodeRefused) && used.is_ok(),
        Clause::AnExpiredCodeIsRefused,
        || format!("expired use {refused:?}, timely use {used:?}"),
    )
}

/// A code that was never issued is refused.
///
/// # Errors
///
/// Returns [`Clause::AnUnknownCodeIsRefused`] otherwise.
pub async fn an_unknown_code_is_refused<T: TokenStore>(
    store: &T,
    _contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let answer = store.use_code(&Digest::from_bytes([9; 32]), at(1)).await;
    ensure(
        answer == Err(TokenStoreError::CodeRefused),
        Clause::AnUnknownCodeIsRefused,
        || format!("{answer:?}"),
    )
}

/// Two uses of one code that run together give one success and one
/// refusal.
///
/// # Errors
///
/// Returns [`Clause::ConcurrentUsesOfOneCodeGiveOneSuccess`] otherwise.
pub async fn concurrent_uses_of_one_code_give_one_success<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
    runner: &impl RunTogether,
) -> Result<(), Violation> {
    for round in 0..8_u8 {
        let code = contract_fixture.code(round.saturating_add(10), 100);
        unexpected(store.issue_code(code.clone()).await)?;
        let answers = runner
            .run_together(
                || store.use_code(code.digest(), at(101)),
                || store.use_code(code.digest(), at(101)),
            )
            .await;
        let (one, two) = unexpected(answers.ok_or("a concurrent use panicked"))?;
        let refused = Err(TokenStoreError::CodeRefused);
        ensure(
            (one.is_ok() && two == refused) || (one == refused && two.is_ok()),
            Clause::ConcurrentUsesOfOneCodeGiveOneSuccess,
            || format!("round {round}: {one:?} and {two:?}"),
        )?;
    }
    Ok(())
}

/// A created token reads back with its owner, label, digest, creation
/// time, a 90-day expiry and no revoked time.
///
/// # Errors
///
/// Returns [`Clause::ATokenReadsBack`] otherwise.
pub async fn a_token_reads_back<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let token = contract_fixture.token(1, &contract_fixture.owner, &contract_fixture.label, 50);
    let revoked = unexpected(store.create(token.clone()).await)?;
    let read = unexpected(store.get(token.id()).await)?;
    ensure(
        revoked.is_none()
            && read == token.clone().stored()
            && read.expires().as_secs() == 50 + TOKEN_LIFE,
        Clause::ATokenReadsBack,
        || format!("revoked {revoked:?}, read {read:?}"),
    )?;
    let taken = store.create(token).await;
    ensure(
        taken == Err(TokenStoreError::Taken),
        Clause::ATokenReadsBack,
        || format!("a second create of one id answered {taken:?}"),
    )
}

/// An unknown token id answers not found.
///
/// # Errors
///
/// Returns [`Clause::AnUnknownTokenIsNotFound`] otherwise.
pub async fn an_unknown_token_is_not_found<T: TokenStore>(
    store: &T,
    _contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let answer = store.get(&TokenId::from_random([7; 16])).await;
    ensure(
        answer == Err(TokenStoreError::NotFound),
        Clause::AnUnknownTokenIsNotFound,
        || format!("{answer:?}"),
    )
}

/// A revoke sets the revoked time, and a second revoke keeps the first
/// time.
///
/// # Errors
///
/// Returns [`Clause::ARevokeKeepsItsFirstTime`] otherwise.
pub async fn a_revoke_keeps_its_first_time<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let owner = &contract_fixture.owner;
    let token = contract_fixture.token(2, owner, &contract_fixture.label, 50);
    unexpected(store.create(token.clone()).await)?;
    unexpected(store.revoke(token.id(), owner, at(60)).await)?;
    unexpected(store.revoke(token.id(), owner, at(70)).await)?;
    let read = unexpected(store.get(token.id()).await)?;
    ensure(
        read.revoked() == Some(at(60)),
        Clause::ARevokeKeepsItsFirstTime,
        || format!("{read:?}"),
    )
}

/// A revoke by someone other than the owner answers not found and changes
/// nothing, and the owner's revoke applies.
///
/// # Errors
///
/// Returns [`Clause::ARevokeByAnotherOwnerIsNotFound`] otherwise.
pub async fn a_revoke_by_another_owner_is_not_found<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let owner = &contract_fixture.owner;
    let token = contract_fixture.token(3, owner, &contract_fixture.label, 50);
    unexpected(store.create(token.clone()).await)?;
    let refused = store
        .revoke(token.id(), &contract_fixture.other_owner, at(60))
        .await;
    let kept = unexpected(store.get(token.id()).await)?;
    ensure(
        refused == Err(TokenStoreError::NotFound) && kept.revoked().is_none(),
        Clause::ARevokeByAnotherOwnerIsNotFound,
        || format!("another owner's revoke {refused:?}, token {kept:?}"),
    )?;
    let applied = store.revoke(token.id(), owner, at(61)).await;
    let read = unexpected(store.get(token.id()).await)?;
    ensure(
        applied.is_ok() && read.revoked() == Some(at(61)),
        Clause::ARevokeByAnotherOwnerIsNotFound,
        || format!("the owner's revoke {applied:?}, token {read:?}"),
    )
}

/// A new token revokes the owner's active token with the same label, and
/// leaves other labels and other owners' tokens active.
///
/// # Errors
///
/// Returns [`Clause::ANewTokenWithTheSameLabelRevokesTheOld`] otherwise.
pub async fn a_new_token_with_the_same_label_revokes_the_old<T: TokenStore>(
    store: &T,
    contract_fixture: &TokenFixture,
) -> Result<(), Violation> {
    let (owner, other) = (&contract_fixture.owner, &contract_fixture.other_owner);
    let (label, other_label) = (&contract_fixture.label, &contract_fixture.other_label);
    let old = contract_fixture.token(4, owner, label, 50);
    let elsewhere = contract_fixture.token(5, owner, other_label, 51);
    let theirs = contract_fixture.token(6, other, label, 52);
    let new = contract_fixture.token(7, owner, label, 60);
    for token in [&old, &elsewhere, &theirs] {
        unexpected(store.create(token.clone()).await)?;
    }
    let revoked = unexpected(store.create(new.clone()).await)?;
    let old_read = unexpected(store.get(old.id()).await)?;
    let elsewhere_read = unexpected(store.get(elsewhere.id()).await)?;
    let theirs_read = unexpected(store.get(theirs.id()).await)?;
    let new_read = unexpected(store.get(new.id()).await)?;
    ensure(
        revoked.as_ref() == Some(old.id())
            && old_read.revoked() == Some(at(60))
            && elsewhere_read.revoked().is_none()
            && theirs_read.revoked().is_none()
            && new_read.revoked().is_none(),
        Clause::ANewTokenWithTheSameLabelRevokesTheOld,
        || {
            format!(
                "revoked {revoked:?}; old {old_read:?}, other label {elsewhere_read:?}, other owner {theirs_read:?}, new {new_read:?}"
            )
        },
    )
}
