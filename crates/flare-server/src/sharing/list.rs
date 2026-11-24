//! `GET /api/shares`: one page of the caller's shares or of the shares
//! granted to the caller, after a key, with the cursor of the next page.

use super::convert::{list_key, role, share_cursor, share_error, summary};
use super::run::json;
use super::{Answer, Refusal, Scope, Sharing};
use flare_contract::{ApiCode, Role as WireRole, ShareCursor, ShareList};
use flare_core::{
    GrantStore, ListKey, Principal, Recipients, ResponseStore, ShareStore, TokenStore,
};

/// The key after which a share list of `scope` starts, from the query's
/// `cursor` values, or `None` when it has none. Two values are a bad
/// query. A value that is not a share cursor, or one of the other scope,
/// is a bad cursor. The cursor names no person: the list is the caller's,
/// whatever key it holds.
pub(crate) fn list_after(cursors: Vec<String>, scope: Scope) -> Result<Option<ListKey>, Refusal> {
    let [text] = cursors.as_slice() else {
        return if cursors.is_empty() {
            Ok(None)
        } else {
            Err(ApiCode::BadQuery.into())
        };
    };
    let cursor = ShareCursor::try_from(text.as_str())
        .ok()
        .filter(|cursor| cursor.scope() == scope)
        .ok_or(ApiCode::SharesBadCursor)?;
    list_key(cursor).map(Some)
}

impl<S, G, R, T, X> Sharing<S, G, R, T, X>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
{
    /// At most `limit` shares of `scope` after `after`, or from the first,
    /// and the cursor of the next page when more follow. A failing store
    /// answers `flare::shares::unavailable` and no page.
    pub(super) async fn list(
        &self,
        who: &Principal,
        scope: Scope,
        after: Option<&ListKey>,
        limit: usize,
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
                    .shared_with_me_page(who, after, limit)
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
