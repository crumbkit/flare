//! `GET /api/shares`: one page of the caller's shares or of the shares
//! granted to the caller, after a key, with the cursor of the next page.

use super::convert::{role, share_cursor, share_error, summary};
use super::run::json;
use super::{Answer, Scope, Sharing};
use flare_contract::{Role as WireRole, ShareList};
use flare_core::{
    GrantStore, ListKey, Principal, Recipients, ResponseStore, ShareStore, Teams, TokenStore,
};

impl<S, G, R, T, X, M> Sharing<S, G, R, T, X, M>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
{
    /// At most `limit` shares of `scope` after `after`, or from the first,
    /// and the cursor of the next page when more follow. With `everyone`,
    /// the shares granted to the caller include those open to everyone
    /// whose audience covers the caller. A failing store
    /// answers `flare::shares::unavailable` and no page.
    pub(super) async fn list(
        &self,
        who: &Principal,
        scope: Scope,
        after: Option<&ListKey>,
        limit: usize,
        everyone: bool,
    ) -> Answer {
        let shares = self.flares.shares();
        let (items, next) = match scope {
            Scope::Mine => {
                let page = shares
                    .mine_page(who, after, limit)
                    .await
                    .map_err(share_error)?;
                let items = page
                    .items
                    .iter()
                    .map(|share| summary(share, WireRole::Owner))
                    .collect::<Vec<_>>();
                (items, page.next)
            }
            Scope::WithMe => {
                let page = shares
                    .shared_with_me_page(who, everyone, after, limit)
                    .await
                    .map_err(share_error)?;
                let items = page
                    .items
                    .iter()
                    .map(|entry| summary(&entry.share, role(entry.role)))
                    .collect::<Vec<_>>();
                (items, page.next)
            }
        };
        let next = next.map(|key| share_cursor(scope, &key)).transpose()?;
        json(
            200,
            &ShareList {
                items,
                more: next.is_some(),
                next,
            },
        )
    }
}
