//! Each wrong comment store below breaks one clause of the suite, so the
//! suite is known to catch that mistake.
#![cfg(test)]

use flare_core::contract::comment_store::{self, CommentFixture};
use flare_core::contract::{Clause, block_on};
use flare_core::memory::MemoryCommentStore;
use flare_core::{
    Added, Comment, CommentBody, CommentEvent, CommentId, CommentKey, CommentStore,
    CommentStoreError, NewComment, Position, ShareId, Timestamp,
};

/// Forwards every call to a correct store.
struct Wrapped(MemoryCommentStore);

impl CommentStore for Wrapped {
    async fn add(&self, comment: NewComment) -> Result<Added, CommentStoreError> {
        self.0.add(comment).await
    }
    async fn get(&self, id: &CommentId) -> Result<Comment, CommentStoreError> {
        self.0.get(id).await
    }
    async fn of_flare(&self, flare: &ShareId) -> Result<Vec<Comment>, CommentStoreError> {
        self.0.of_flare(flare).await
    }
    async fn edit(
        &self,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> Result<Comment, CommentStoreError> {
        self.0.edit(id, body, at).await
    }
    async fn delete(&self, id: &CommentId, at: Timestamp) -> Result<Comment, CommentStoreError> {
        self.0.delete(id, at).await
    }
    async fn events(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<CommentEvent>, CommentStoreError> {
        self.0.events(flare, after, upto, limit).await
    }
}

/// Gives every comment its own key, so no repeat is ever found.
struct IgnoresKeys(Wrapped);

impl CommentStore for IgnoresKeys {
    async fn add(&self, mut comment: NewComment) -> Result<Added, CommentStoreError> {
        comment.key = CommentKey::try_from(comment.id.as_str()).unwrap();
        self.0.add(comment).await
    }
    async fn get(&self, id: &CommentId) -> Result<Comment, CommentStoreError> {
        self.0.get(id).await
    }
    async fn of_flare(&self, flare: &ShareId) -> Result<Vec<Comment>, CommentStoreError> {
        self.0.of_flare(flare).await
    }
    async fn edit(
        &self,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> Result<Comment, CommentStoreError> {
        self.0.edit(id, body, at).await
    }
    async fn delete(&self, id: &CommentId, at: Timestamp) -> Result<Comment, CommentStoreError> {
        self.0.delete(id, at).await
    }
    async fn events(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<CommentEvent>, CommentStoreError> {
        self.0.events(flare, after, upto, limit).await
    }
}

/// Answers a delete without erasing the text.
struct KeepsTheText(Wrapped);

impl CommentStore for KeepsTheText {
    async fn add(&self, comment: NewComment) -> Result<Added, CommentStoreError> {
        self.0.add(comment).await
    }
    async fn get(&self, id: &CommentId) -> Result<Comment, CommentStoreError> {
        self.0.get(id).await
    }
    async fn of_flare(&self, flare: &ShareId) -> Result<Vec<Comment>, CommentStoreError> {
        self.0.of_flare(flare).await
    }
    async fn edit(
        &self,
        id: &CommentId,
        body: CommentBody,
        at: Timestamp,
    ) -> Result<Comment, CommentStoreError> {
        self.0.edit(id, body, at).await
    }
    async fn delete(&self, id: &CommentId, _at: Timestamp) -> Result<Comment, CommentStoreError> {
        self.0.get(id).await
    }
    async fn events(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<CommentEvent>, CommentStoreError> {
        self.0.events(flare, after, upto, limit).await
    }
}

#[test]
fn a_store_that_ignores_keys_fails_the_repeated_key_clause() {
    let fixture = CommentFixture::standard().unwrap();
    let store = IgnoresKeys(Wrapped(MemoryCommentStore::new()));
    let broken = block_on(comment_store::a_repeated_key_answers_the_first_comment(
        &store, &fixture,
    ))
    .unwrap_err();
    assert_eq!(broken.clause(), Clause::ARepeatedKeyAnswersTheFirstComment);
}

#[test]
fn a_store_that_keeps_the_text_after_a_delete_fails_the_delete_clause() {
    let fixture = CommentFixture::standard().unwrap();
    let store = KeepsTheText(Wrapped(MemoryCommentStore::new()));
    let broken = block_on(comment_store::a_delete_erases_the_text_and_keeps_the_rest(
        &store, &fixture,
    ))
    .unwrap_err();
    assert_eq!(broken.clause(), Clause::ADeleteErasesTheTextAndKeepsTheRest);
}
