//! The activity route of [`Sharing`]: responses and comments on named
//! flares after a position, from the larger of `since` and a saved cursor.

use super::convert::{flare_error, item};
use super::run::{json, share_id};
use super::{Answer, Sharing};
use flare_contract::{ActivityPage, ApiCode, Cursor};
use flare_core::{
    CommentStore, Creators, CursorStore, GrantStore, Position, Principal, Recipients,
    ResponseStore, ShareStore, Teams, TokenStore,
};

impl<S, G, R, C, U, T, X, M, K> Sharing<S, G, R, C, U, T, X, M, K>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
    K: Creators,
{
    pub(super) async fn activity(
        &self,
        who: &Principal,
        flares: &[String],
        (since, since_last): (Option<&str>, Option<&str>),
        limit: usize,
    ) -> Answer {
        let given = match since {
            Some(text) => Cursor::try_from(text)
                .map_err(|_| ApiCode::BadCursor)?
                .position(),
            None => 0,
        };
        let saved = match since_last {
            Some(name) => self.saved_position(who, name).await?.unwrap_or(0),
            None => 0,
        };
        let after = given.max(saved);
        let ids = flares
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>();
        if flares.len() > flare_core::MAX_FLARES {
            return Err(ApiCode::TooManyFlares.into());
        }
        let page = self
            .flares
            .activity(who, &ids?, Position::new(after), limit)
            .await
            .map_err(flare_error)?;
        json(
            200,
            &ActivityPage {
                items: page.items.iter().filter_map(item).collect(),
                cursor: Cursor::new(page.last.get()).to_string(),
                more: page.more,
            },
        )
    }
}
