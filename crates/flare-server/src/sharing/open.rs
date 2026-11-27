//! `GET /api/shares/{id}`: one version of a share, as the caller sees it.

use super::convert::{share_error, standing, summary};
use super::run::{json, share_id};
use super::{Answer, Sharing};
use flare_contract::OpenedShare;
use flare_core::{
    CommentStore, Creators, CursorStore, GrantStore, Principal, ResponseStore, ShareContent,
    ShareStore, ShareVersion, Teams,
};

impl<S, G, R, C, U, T, X, M, K, F> Sharing<S, G, R, C, U, T, X, M, K, F>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    M: Teams,
    K: Creators,
{
    /// Opens version `number` of a share, or its latest. A number above
    /// the latest answers `flare::shares::not_found`, as a share the caller
    /// cannot open does, after the caller's standing is checked. The
    /// answer for an HTML share holds an empty `content`, because its
    /// file is served only to a frame by the content host.
    pub(super) async fn open(
        &self,
        who: &Principal,
        id: &str,
        number: Option<ShareVersion>,
    ) -> Answer {
        let id = share_id(id)?;
        let opened = self
            .flares
            .shares()
            .open(who, &id, number)
            .await
            .map_err(share_error)?;
        let content = match opened.version.content() {
            ShareContent::Document(text) => text.as_str().to_owned(),
            _ => String::new(),
        };
        let version = opened.version.number().get();
        let mut share = summary(&opened.share, standing(opened.standing));
        share.comments = self.live_comments(who, &id).await.ok();
        json(
            200,
            &OpenedShare {
                share,
                version,
                content,
            },
        )
    }
}
