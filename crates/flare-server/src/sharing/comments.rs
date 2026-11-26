//! The comment and cursor routes of [`Sharing`], and the contract's form
//! of a comment.

use super::convert::flare_error;
use super::tokens::random_id;
use super::{Answer, Count, Refusal, Sharing};
use flare_contract::{
    ActivityItem as WireItem, ApiCode, Comment as WireComment, CommentEdit, CommentOrReply,
    CommentThread as WireThread, Cursor, NamedCursor, NewComment, Reply as WireReply, utc,
};
use flare_core::{
    Comment, CommentAction, CommentActivity, CommentBody, CommentDraft, CommentId, CommentKey,
    CommentStore, CommentThread, Creators, CursorName, CursorStore, FlareError, GrantStore,
    Position, Principal, Recipients, ResponseStore, ShareId, ShareStore, Teams, Timestamp,
    TokenStore,
};
use serde::Serialize;

fn json(status: u16, body: &impl Serialize) -> Answer {
    serde_json::to_string(body)
        .map(|text| (status, Some(text)))
        .map_err(|_| ApiCode::CommentsUnavailable.into())
}

fn invalid(detail: impl ToString) -> Refusal {
    Refusal {
        code: ApiCode::CommentInvalid,
        detail: Some(detail.to_string()),
        share: None,
        errors: Vec::new(),
    }
}

fn comment_id(text: &str) -> Result<CommentId, Refusal> {
    CommentId::try_from(text).map_err(|_| ApiCode::CommentNotFound.into())
}

fn cursor_name(text: &str) -> Result<CursorName, Refusal> {
    CursorName::try_from(text).map_err(|_| ApiCode::BadQuery.into())
}

fn body_text(comment: &Comment) -> Option<String> {
    comment.body.as_ref().map(|body| body.as_str().to_owned())
}

fn reply(comment: &Comment, parent: &CommentId) -> WireReply {
    WireReply {
        id: comment.id.as_str().to_owned(),
        flare: comment.flare.as_str().to_owned(),
        parent: parent.as_str().to_owned(),
        author: comment.author.as_str().to_owned(),
        at: utc(comment.created.as_secs()),
        edited: comment.edited.map(|at| utc(at.as_secs())),
        deleted: comment.is_deleted(),
        body: body_text(comment),
    }
}

fn top(comment: &Comment, replies: Vec<WireReply>) -> WireComment {
    WireComment {
        id: comment.id.as_str().to_owned(),
        flare: comment.flare.as_str().to_owned(),
        author: comment.author.as_str().to_owned(),
        at: utc(comment.created.as_secs()),
        edited: comment.edited.map(|at| utc(at.as_secs())),
        deleted: comment.is_deleted(),
        body: body_text(comment),
        replies,
    }
}

/// One comment or reply as the contract writes it, with no replies.
pub(super) fn one(comment: &Comment) -> CommentOrReply {
    match &comment.parent {
        Some(parent) => CommentOrReply::Reply(reply(comment, parent)),
        None => CommentOrReply::Comment(top(comment, Vec::new())),
    }
}

fn thread(flare: &ShareId, threads: &[CommentThread]) -> WireThread {
    WireThread {
        flare: flare.as_str().to_owned(),
        comments: threads
            .iter()
            .map(|one| {
                let replies = one
                    .replies
                    .iter()
                    .map(|each| reply(each, &one.comment.id))
                    .collect();
                top(&one.comment, replies)
            })
            .collect(),
    }
}

/// The contract's activity item for a comment event. A comment, reply or
/// edit whose author has since deleted the comment holds no text and is
/// left out; the deletion's own item follows it.
pub(super) fn activity(item: &CommentActivity) -> Option<WireItem> {
    if item.action != CommentAction::Deleted && item.body.is_none() {
        return None;
    }
    Some(item_of(item))
}

fn item_of(item: &CommentActivity) -> WireItem {
    let flare = item.flare.as_str().to_owned();
    let comment = item.comment.as_str().to_owned();
    let by = item.by.as_str().to_owned();
    let at = utc(item.at.as_secs());
    let parent = item
        .parent
        .as_ref()
        .map(|parent| parent.as_str().to_owned());
    let body = item
        .body
        .as_ref()
        .map(|body| body.as_str().to_owned())
        .unwrap_or_default();
    match (item.action, parent) {
        (CommentAction::Deleted, parent) => WireItem::Deleted {
            flare,
            comment,
            parent,
            by,
            at,
        },
        (CommentAction::Edited, parent) => WireItem::Edited {
            flare,
            comment,
            parent,
            by,
            body,
            at,
        },
        (_, Some(parent)) => WireItem::Reply {
            flare,
            comment,
            parent,
            by,
            body,
            at,
        },
        (_, None) => WireItem::Comment {
            flare,
            comment,
            by,
            body,
            at,
        },
    }
}

impl<S, G, R, C, U, T, X, M, K, F> Sharing<S, G, R, C, U, T, X, M, K, F>
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
    /// `GET /api/comments?flare=`.
    pub(super) async fn thread(&self, who: &Principal, flare: &str) -> Answer {
        let flare = ShareId::try_from(flare).map_err(|_| ApiCode::ShareNotFound)?;
        let threads = self
            .flares
            .comments(who, &flare)
            .await
            .map_err(flare_error)?;
        json(200, &thread(&flare, &threads))
    }

    /// `POST /api/comments`.
    pub(super) async fn post(
        &self,
        who: &Principal,
        request: NewComment,
        now: Timestamp,
    ) -> Answer {
        let flare =
            ShareId::try_from(request.flare.as_str()).map_err(|_| ApiCode::ShareNotFound)?;
        let parent = request.parent.as_deref().map(comment_id).transpose()?;
        let body = CommentBody::try_from(request.body.as_str()).map_err(invalid)?;
        let key = CommentKey::try_from(request.key.as_str()).map_err(invalid)?;
        let id = CommentId::from_random(random_id().ok_or(ApiCode::CommentsUnavailable)?);
        let posted = self
            .flares
            .post_comment(who, CommentDraft::new(id, flare, parent, body, key), now)
            .await
            .map_err(flare_error)?;
        json(
            if posted.created { 201 } else { 200 },
            &one(&posted.comment),
        )
    }

    /// `PATCH /api/comments/{id}`.
    pub(super) async fn edit(
        &self,
        who: &Principal,
        id: &str,
        request: CommentEdit,
        now: Timestamp,
    ) -> Answer {
        let id = comment_id(id)?;
        let body = CommentBody::try_from(request.body.as_str()).map_err(invalid)?;
        let edited = self
            .flares
            .edit_comment(who, &id, body, now)
            .await
            .map_err(flare_error)?;
        json(200, &one(&edited))
    }

    /// `DELETE /api/comments/{id}`.
    pub(super) async fn delete(&self, who: &Principal, id: &str, now: Timestamp) -> Answer {
        let id = comment_id(id)?;
        let deleted = self
            .flares
            .delete_comment(who, &id, now)
            .await
            .map_err(flare_error)?;
        json(200, &one(&deleted))
    }

    /// `GET /api/cursors/{name}`.
    pub(super) async fn cursor(&self, who: &Principal, name: &str) -> Answer {
        let name = cursor_name(name)?;
        let position = self.flares.cursor(who, &name).await.map_err(flare_error)?;
        json(200, &named(position))
    }

    /// `PUT /api/cursors/{name}`.
    pub(super) async fn save_cursor(
        &self,
        who: &Principal,
        name: &str,
        request: NamedCursor,
    ) -> Answer {
        let name = cursor_name(name)?;
        let to = Cursor::try_from(request.cursor.as_str()).map_err(|_| ApiCode::BadCursor)?;
        let kept = self
            .flares
            .advance_cursor(who, &name, Position::new(to.position()))
            .await
            .map_err(flare_error)?;
        json(200, &named(kept))
    }

    /// The position of the caller's cursor `name`, or none when they have
    /// saved no such cursor.
    pub(super) async fn saved_position(
        &self,
        who: &Principal,
        name: &str,
    ) -> Result<Option<u64>, Refusal> {
        let name = cursor_name(name)?;
        match self.flares.cursor(who, &name).await {
            Ok(position) => Ok(Some(position.get())),
            Err(FlareError::CursorNotFound) => Ok(None),
            Err(error) => Err(flare_error(error)),
        }
    }
}

fn named(position: Position) -> NamedCursor {
    NamedCursor {
        cursor: Cursor::new(position.get()).to_string(),
    }
}

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
    /// How many comments and replies of `flare` their authors have not
    /// deleted.
    pub(super) async fn live_comments(
        &self,
        who: &Principal,
        flare: &ShareId,
    ) -> Result<u32, Refusal> {
        let threads = self
            .flares
            .comments(who, flare)
            .await
            .map_err(flare_error)?;
        let live = threads
            .iter()
            .flat_map(|one| std::iter::once(&one.comment).chain(one.replies.iter()))
            .filter(|comment| !comment.is_deleted())
            .count();
        Ok(u32::try_from(live).unwrap_or(u32::MAX))
    }

    /// The comments and replies of `flare` as the reading page's comments
    /// section counts them: every reply that is not deleted, and every
    /// comment that is not deleted or that still has such a reply, because
    /// the section keeps that comment's place.
    pub(super) async fn shown_count(
        &self,
        who: &Principal,
        flare: &ShareId,
    ) -> Result<Count, Refusal> {
        let threads = self
            .flares
            .comments(who, flare)
            .await
            .map_err(flare_error)?;
        let mut count = Count::default();
        for one in &threads {
            let replies = one.replies.iter().filter(|each| !each.is_deleted()).count();
            if !one.comment.is_deleted() || replies > 0 {
                count.comments = count.comments.saturating_add(1);
            }
            count.replies = count
                .replies
                .saturating_add(u32::try_from(replies).unwrap_or(u32::MAX));
        }
        Ok(count)
    }
}
