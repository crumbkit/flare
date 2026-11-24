//! The [`Shares`] methods that list the caller's shares and the shares
//! granted to the caller, whole or one page after a key.

use super::groups::{address, rank};
use super::{SharedWithMe, Shares, actor};
use crate::auth::Principal;
use crate::grant::Grant;
use crate::share::{Listing, MAX_LISTED, Share, ShareId};
use crate::sharing::error::ShareError;
use crate::store::{GrantStore, ListKey, ListPage, ShareStore, ShareStoreError};
use crate::teams::{Group, Teams};
use std::collections::HashMap;

impl<S: ShareStore, G: GrantStore, T: Teams> Shares<S, G, T> {
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

    /// The shares granted to the caller, directly or through an active
    /// team, newest grant first, each with the caller's highest role. With
    /// `everyone`, the shares open to everyone whose audience covers the
    /// caller are listed too. A grant whose share no longer exists, and a
    /// share the caller owns, are left out.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`] when a store fails.
    pub async fn shared_with_me(
        &self,
        who: &Principal,
        everyone: bool,
    ) -> Result<Listing<SharedWithMe>, ShareError> {
        let page = self
            .shared_with_me_page(who, everyone, None, MAX_LISTED)
            .await?;
        let truncated = page.next.is_some();
        Ok(Listing::with_truncated(page.items, truncated))
    }

    /// The shares granted to the caller whose grants sort after `after`,
    /// newest grant first and then by share, read from at most `limit`
    /// grants, and the key of the last grant read when more follow.
    ///
    /// The page merges the caller's own grants, the grants of each active
    /// team the caller belongs to and, with `everyone`, the everyone grants.
    /// Each list is read after `after` and at most `limit` deep, so the
    /// first `limit` grants of the merge are exact, and the next key is the
    /// last of those. Everyone grants whose audience does not cover the
    /// caller are dropped after the merge is cut to `limit`, so the key
    /// never passes an everyone grant that was not read. A share that two
    /// of the remaining grants name is listed once on a page, with the
    /// higher role. When its grants fall on two pages, it is listed on
    /// each.
    ///
    /// A grant whose share no longer exists, a share the caller owns, and
    /// an everyone grant that does not open to the caller are left out, and
    /// the key still passes them, so a page can hold fewer than `limit`
    /// entries and have a next key. A `limit` above
    /// [`MAX_LISTED`] reads [`MAX_LISTED`] grants, and a `limit` of 0 reads
    /// one.
    ///
    /// # Errors
    ///
    /// Returns [`ShareError::NoVerifiedEmail`] and
    /// [`ShareError::Unavailable`] when a store fails.
    pub async fn shared_with_me_page(
        &self,
        who: &Principal,
        everyone: bool,
        after: Option<&ListKey>,
        limit: usize,
    ) -> Result<ListPage<SharedWithMe>, ShareError> {
        let actor = actor(who)?;
        let person = address(&actor).ok_or(ShareError::NoVerifiedEmail)?;
        let limit = limit.clamp(1, MAX_LISTED);
        let mut lists = vec![actor.clone()];
        lists.extend(
            self.active_teams(&person)
                .await?
                .into_iter()
                .map(|team| Group::Team(team).principal_id()),
        );
        if everyone {
            lists.push(Group::Everyone.principal_id());
        }
        let mut grants: Vec<Grant> = Vec::new();
        let mut more = false;
        for list in &lists {
            let read = self
                .grants
                .granted_page(list, after, limit)
                .await
                .map_err(|_| ShareError::Unavailable)?;
            more |= read.truncated();
            grants.extend(read.into_items());
        }
        grants.sort_by_key(ListKey::of_grant);
        if grants.len() > limit {
            more = true;
            grants.truncate(limit);
        }
        let next = more.then(|| grants.last().map(ListKey::of_grant)).flatten();
        let mut order: Vec<ShareId> = Vec::with_capacity(grants.len());
        let mut best: HashMap<ShareId, Grant> = HashMap::new();
        let opens = |grant: &Grant| {
            Group::of_grantee(grant.grantee()) != Some(Group::Everyone)
                || self.everyone_opens(grant, &person)
        };
        for grant in grants.into_iter().filter(opens) {
            match best.get(grant.share()) {
                None => {
                    order.push(grant.share().clone());
                    best.insert(grant.share().clone(), grant);
                }
                Some(held) if rank(grant.role()) > rank(held.role()) => {
                    best.insert(grant.share().clone(), grant);
                }
                Some(_) => {}
            }
        }
        let mut entries = Vec::with_capacity(order.len());
        for id in order {
            let Some(grant) = best.remove(&id) else {
                continue;
            };
            match self.shares.get(&id).await {
                Ok(share) if share.owner() == &actor => {}
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
