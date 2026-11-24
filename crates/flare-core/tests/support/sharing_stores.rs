//! Stores for the tests of the `Shares` service: a handle that lets two
//! services share one store, a grant store that runs an action between the
//! service's read and its write, a share store whose version reads fail, and
//! stores whose every method fails.

use flare_core::{
    FileCursor, FileId, FilePage, FileStore, FileStoreError, Grant, GrantRevoke, GrantStore,
    GrantStoreError, GrantWrite, ListKey, Listing, NewFile, NewShare, NewVersion, PrincipalId,
    Share, ShareId, ShareStore, ShareStoreError, ShareVersion, SharedVersion, StoredFile,
};
use std::sync::{Arc, Mutex};

/// A handle on a store that several services share.
pub(crate) struct Shared<T>(pub(crate) Arc<T>);

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T: ShareStore> ShareStore for Shared<T> {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        self.0.create(new).await
    }

    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        ShareStore::get(&*self.0, id).await
    }

    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        self.0.publish(id, version).await
    }

    async fn version(
        &self,
        id: &ShareId,
        number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        self.0.version(id, number).await
    }

    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        self.0.owned_by(owner).await
    }
}

impl<T: GrantStore> GrantStore for Shared<T> {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        self.0.grant(write).await
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        self.0.revoke(revoke).await
    }

    async fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        GrantStore::get(&*self.0, share, grantee).await
    }

    async fn of_share(&self, share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        self.0.of_share(share).await
    }

    async fn granted_page(
        &self,
        grantee: &PrincipalId,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<Listing<Grant>, GrantStoreError> {
        self.0.granted_page(grantee, after, limit).await
    }
}

type Action = Box<dyn FnOnce() + Send>;

/// A grant store that runs an armed action once, after the service's reads
/// and before its next `grant` or `revoke` reaches the store, and then
/// delegates. A test arms it with a write by another service on the same
/// store, so the write lands between the service's read and its write.
pub(crate) struct InterleavingGrants<G> {
    inner: G,
    action: Mutex<Option<Action>>,
}

impl<G> InterleavingGrants<G> {
    pub(crate) fn new(inner: G) -> Self {
        Self {
            inner,
            action: Mutex::new(None),
        }
    }

    /// Runs `action` before the next write.
    pub(crate) fn arm(&self, action: impl FnOnce() + Send + 'static) {
        *self.action.lock().unwrap() = Some(Box::new(action));
    }

    fn interleave(&self) {
        let action = self.action.lock().unwrap().take();
        if let Some(action) = action {
            action();
        }
    }
}

impl<G: GrantStore> GrantStore for InterleavingGrants<G> {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        self.interleave();
        self.inner.grant(write).await
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        self.interleave();
        self.inner.revoke(revoke).await
    }

    async fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        self.inner.get(share, grantee).await
    }

    async fn of_share(&self, share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        self.inner.of_share(share).await
    }

    async fn granted_page(
        &self,
        grantee: &PrincipalId,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<Listing<Grant>, GrantStoreError> {
        self.inner.granted_page(grantee, after, limit).await
    }
}

/// A share store whose every method answers `Unavailable`.
pub(crate) struct BrokenShares;

impl ShareStore for BrokenShares {
    async fn create(&self, _new: NewShare) -> Result<Share, ShareStoreError> {
        Err(ShareStoreError::Unavailable)
    }

    async fn get(&self, _id: &ShareId) -> Result<Share, ShareStoreError> {
        Err(ShareStoreError::Unavailable)
    }

    async fn publish(
        &self,
        _id: &ShareId,
        _version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        Err(ShareStoreError::Unavailable)
    }

    async fn version(
        &self,
        _id: &ShareId,
        _number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        Err(ShareStoreError::Unavailable)
    }

    async fn owned_by(&self, _owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        Err(ShareStoreError::Unavailable)
    }
}

/// A share store that delegates every method except `version`, which
/// answers `Unavailable`.
pub(crate) struct VersionlessShares<S>(pub(crate) S);

impl<S: ShareStore> ShareStore for VersionlessShares<S> {
    async fn create(&self, new: NewShare) -> Result<Share, ShareStoreError> {
        self.0.create(new).await
    }

    async fn get(&self, id: &ShareId) -> Result<Share, ShareStoreError> {
        self.0.get(id).await
    }

    async fn publish(
        &self,
        id: &ShareId,
        version: NewVersion,
    ) -> Result<ShareVersion, ShareStoreError> {
        self.0.publish(id, version).await
    }

    async fn version(
        &self,
        _id: &ShareId,
        _number: ShareVersion,
    ) -> Result<SharedVersion, ShareStoreError> {
        Err(ShareStoreError::Unavailable)
    }

    async fn owned_by(&self, owner: &PrincipalId) -> Result<Listing<Share>, ShareStoreError> {
        self.0.owned_by(owner).await
    }
}

/// A grant store whose every method answers `Unavailable`.
pub(crate) struct BrokenGrants;

impl GrantStore for BrokenGrants {
    async fn grant(&self, _write: GrantWrite) -> Result<Grant, GrantStoreError> {
        Err(GrantStoreError::Unavailable)
    }

    async fn revoke(&self, _revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        Err(GrantStoreError::Unavailable)
    }

    async fn get(
        &self,
        _share: &ShareId,
        _grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        Err(GrantStoreError::Unavailable)
    }

    async fn of_share(&self, _share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        Err(GrantStoreError::Unavailable)
    }

    async fn granted_page(
        &self,
        _grantee: &PrincipalId,
        _after: Option<&ListKey>,
        _limit: usize,
    ) -> Result<Listing<Grant>, GrantStoreError> {
        Err(GrantStoreError::Unavailable)
    }
}

/// A file store whose every method answers `Unavailable`.
pub(crate) struct BrokenFiles;

impl FileStore for BrokenFiles {
    async fn put(&self, _file: NewFile) -> Result<(), FileStoreError> {
        Err(FileStoreError::Unavailable)
    }

    async fn get(&self, _share: &ShareId, _id: &FileId) -> Result<StoredFile, FileStoreError> {
        Err(FileStoreError::Unavailable)
    }

    async fn delete(&self, _share: &ShareId, _id: &FileId) -> Result<(), FileStoreError> {
        Err(FileStoreError::Unavailable)
    }

    async fn list(
        &self,
        _after: Option<&FileCursor>,
        _limit: usize,
    ) -> Result<FilePage, FileStoreError> {
        Err(FileStoreError::Unavailable)
    }
}
