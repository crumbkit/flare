//! Store and directory wrappers that fail every call while a test holds
//! their switch on, and pass every call through otherwise.

use flare_core::memory::{MemoryResponseStore, MemoryShareStore, MemoryTokenStore};
use flare_core::{
    AddressesOnly, CreatorList, Creators, CreatorsError, Digest, Email, IndexEntry, Listing,
    LoginCode, NewEntry, NewLink, NewShare, NewToken, NewVersion, Position, Principal, PrincipalId,
    Reader, RecipientError, Recipients, ResponseLink, ResponseStore, ResponseStoreError, Share,
    ShareId, ShareStore, ShareStoreError, ShareVersion, SharedVersion, StoredToken, Timestamp,
    TokenId, TokenStore, TokenStoreError, UsedCode,
};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// The switches of the four wrappers.
#[derive(Debug, Clone, Default)]
pub struct Faults {
    /// Fails the share store.
    pub shares: Arc<AtomicBool>,
    /// Fails the token store.
    pub tokens: Arc<AtomicBool>,
    /// Fails the response store.
    pub responses: Arc<AtomicBool>,
    /// Fails only the response store's index writes.
    pub index: Arc<AtomicBool>,
    /// Fails the recipients directory.
    pub recipients: Arc<AtomicBool>,
    /// Fails the creators rule.
    pub creators: Arc<AtomicBool>,
}

fn on(switch: &AtomicBool) -> bool {
    switch.load(Ordering::SeqCst)
}

/// A share store that fails while its switch is on.
#[derive(Debug, Default, Clone)]
pub struct FaultyShares {
    /// The store that answers while the switch is off, which a test may
    /// also reach through a clone.
    pub inner: Arc<MemoryShareStore>,
    /// The switch.
    pub fail: Arc<AtomicBool>,
}

impl FaultyShares {
    fn check(&self) -> Result<(), ShareStoreError> {
        if on(&self.fail) {
            Err(ShareStoreError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl ShareStore for FaultyShares {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        self.check()?;
        self.inner.create(new).await
    }
    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        self.check()?;
        self.inner.get(id).await
    }
    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        self.check()?;
        self.inner.publish(id, version).await
    }
    async fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        self.check()?;
        self.inner.version(id, number).await
    }
    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        self.check()?;
        self.inner.owned_by(owner).await
    }
}

/// A token store that fails while its switch is on.
#[derive(Debug, Default)]
pub struct FaultyTokens {
    /// The store that answers while the switch is off.
    pub inner: MemoryTokenStore,
    /// The switch.
    pub fail: Arc<AtomicBool>,
}

impl FaultyTokens {
    fn check(&self) -> Result<(), TokenStoreError> {
        if on(&self.fail) {
            Err(TokenStoreError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl TokenStore for FaultyTokens {
    async fn issue_code(&self, code: LoginCode) -> Result<(), TokenStoreError> {
        self.check()?;
        self.inner.issue_code(code).await
    }
    async fn use_code(&self, digest: &Digest, now: Timestamp) -> Result<UsedCode, TokenStoreError> {
        self.check()?;
        self.inner.use_code(digest, now).await
    }
    async fn create(&self, token: NewToken) -> Result<Option<TokenId>, TokenStoreError> {
        self.check()?;
        self.inner.create(token).await
    }
    async fn get(&self, id: &TokenId) -> Result<StoredToken, TokenStoreError> {
        self.check()?;
        self.inner.get(id).await
    }
    async fn revoke(
        &self,
        id: &TokenId,
        owner: &Email,
        at: Timestamp,
    ) -> Result<(), TokenStoreError> {
        self.check()?;
        self.inner.revoke(id, owner, at).await
    }
}

/// A response store that fails while its switch is on, and fails its
/// index writes alone while `fail_index` is on.
#[derive(Debug, Default)]
pub struct FaultyResponses {
    /// The store that answers while the switch is off.
    pub inner: MemoryResponseStore,
    /// The switch.
    pub fail: Arc<AtomicBool>,
    /// The switch of the index writes.
    pub fail_index: Arc<AtomicBool>,
}

impl FaultyResponses {
    fn check(&self) -> Result<(), ResponseStoreError> {
        if on(&self.fail) {
            Err(ResponseStoreError::Unavailable)
        } else {
            Ok(())
        }
    }
}

impl ResponseStore for FaultyResponses {
    async fn link(&self, link: NewLink) -> Result<ResponseLink, ResponseStoreError> {
        self.check()?;
        self.inner.link(link).await
    }
    async fn head(&self) -> Result<Position, ResponseStoreError> {
        self.check()?;
        self.inner.head().await
    }
    async fn to_flare(
        &self,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        self.check()?;
        self.inner.to_flare(flare, after, upto, limit).await
    }
    async fn of_response(
        &self,
        response: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        self.check()?;
        self.inner.of_response(response).await
    }
    async fn pairs(
        &self,
        responses: &[ShareId],
        flare: &ShareId,
    ) -> Result<Vec<ResponseLink>, ResponseStoreError> {
        self.check()?;
        self.inner.pairs(responses, flare).await
    }
    async fn index(&self, entries: Vec<NewEntry>) -> Result<(), ResponseStoreError> {
        self.check()?;
        if on(&self.fail_index) {
            return Err(ResponseStoreError::Unavailable);
        }
        self.inner.index(entries).await
    }
    async fn unindex(&self, reader: &Reader, response: &ShareId) -> Result<(), ResponseStoreError> {
        self.check()?;
        if on(&self.fail_index) {
            return Err(ResponseStoreError::Unavailable);
        }
        self.inner.unindex(reader, response).await
    }
    async fn visible(
        &self,
        reader: &Reader,
        flare: &ShareId,
        after: Position,
        upto: Position,
        limit: usize,
    ) -> Result<Vec<IndexEntry>, ResponseStoreError> {
        self.check()?;
        self.inner.visible(reader, flare, after, upto, limit).await
    }
}

/// The address-only recipients, failing while its switch is on.
#[derive(Debug, Default)]
pub struct FaultyRecipients {
    /// The switch.
    pub fail: Arc<AtomicBool>,
}

impl Recipients for FaultyRecipients {
    async fn resolve(&self, identifier: &str) -> Result<Email, RecipientError> {
        if on(&self.fail) {
            return Err(RecipientError::Unavailable);
        }
        AddressesOnly.resolve(identifier).await
    }
}

/// A creators list, failing while its switch is on.
#[derive(Debug)]
pub struct FaultyCreators {
    /// The switch.
    pub fail: Arc<AtomicBool>,
    /// The list it asks while the switch is off.
    pub inner: CreatorList,
}

impl Creators for FaultyCreators {
    async fn may_create(&self, who: &Principal) -> Result<bool, CreatorsError> {
        if on(&self.fail) {
            return Err(CreatorsError::Unavailable);
        }
        self.inner.may_create(who).await
    }
}
