//! The [`Shares`] methods that list the caller's shares and the shares
//! granted to the caller, whole or one page after a key.

use super::{SharedWithMe, Shares, actor};
use crate::auth::Principal;
use crate::share::{Listing, MAX_LISTED, Share};
use crate::sharing::error::ShareError;
use crate::store::{GrantStore, ListKey, ListPage, ShareStore, ShareStoreError};

impl<S: ShareStore, G: GrantStore> Shares<S, G> {
    /// The shares that the caller owns, newest first.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn mine(&self, who: &Principal) -> Result<Listing<Share>, ShareError> {
        let page = self.mine_page(who, None, MAX_LISTED).await?;
        let truncated = page.next.is_some();
        Ok(Listing::with_truncated(page.items, truncated))
    }

    /// At most `limit` of the shares that the caller owns that sort after
    /// `after`, newest first and then by id, and the key of the last one
    /// when more follow. A `limit` above [`MAX_LISTED`] reads
    /// [`MAX_LISTED`], and a `limit` of 0 reads one.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`].
    pub async fn mine_page(
        &self,
        who: &Principal,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<ListPage<Share>, ShareError> {
        let actor = actor(who)?;
        // This reads the owner's whole list and applies the key and the
        // limit here, where a store-side key would read only the page. An
        // owner holds at most MAX_SHARES_PER_OWNER (500) shares, which
        // equals MAX_LISTED, so `owned_by` always returns the whole list
        // and one read of at most 500 shares serves every page. When
        // MAX_SHARES_PER_OWNER rises above MAX_LISTED, `ShareStore` needs
        // an `owned_page` with a key, as `GrantStore::granted_page` has.
        let owned = self
            .shares
            .owned_by(&actor)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let truncated = owned.truncated();
        let following: Vec<Share> = owned
            .into_items()
            .into_iter()
            .filter(|share| after.is_none_or(|key| key.precedes(&ListKey::of_share(share))))
            .collect();
        let page = Listing::bounded(following, limit.max(1));
        let more = page.truncated() || truncated;
        let items = page.into_items();
        let next = more.then(|| items.last().map(ListKey::of_share)).flatten();
        Ok(ListPage::new(items, next))
    }

    /// The shares granted to the caller, newest grant first, each with the
    /// caller's role. A grant whose share no longer exists is left out.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`] when either store fails.
    pub async fn shared_with_me(
        &self,
        who: &Principal,
    ) -> Result<Listing<SharedWithMe>, ShareError> {
        let page = self.shared_with_me_page(who, None, MAX_LISTED).await?;
        let truncated = page.next.is_some();
        Ok(Listing::with_truncated(page.items, truncated))
    }

    /// The shares granted to the caller whose grants sort after `after`,
    /// newest grant first and then by share, read from at most `limit`
    /// grants, and the key of the last grant read when more follow. A grant
    /// whose share no longer exists is left out, and the key still passes
    /// it, so a page can hold fewer than `limit` entries and have a next
    /// key. A `limit` above [`MAX_LISTED`] reads [`MAX_LISTED`] grants, and
    /// a `limit` of 0 reads one.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`] when either store fails.
    pub async fn shared_with_me_page(
        &self,
        who: &Principal,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<ListPage<SharedWithMe>, ShareError> {
        let actor = actor(who)?;
        let granted = self
            .grants
            .granted_page(&actor, after, limit)
            .await
            .map_err(|_| ShareError::Unavailable)?;
        let truncated = granted.truncated();
        let grants = granted.into_items();
        let next = truncated
            .then(|| grants.last().map(ListKey::of_grant))
            .flatten();
        let mut entries = Vec::with_capacity(grants.len());
        for grant in grants {
            match self.shares.get(grant.share()).await {
                Ok(share) => entries.push(SharedWithMe {
                    share,
                    role: grant.role(),
                }),
                Err(ShareStoreError::NotFound) => {}
                Err(_) => return Err(ShareError::Unavailable),
            }
        }
        Ok(ListPage::new(entries, next))
    }
}
