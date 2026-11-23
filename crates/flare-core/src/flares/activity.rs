//! [`Flares::activity`]: the responses and comment events of one or more
//! flares since a position, oldest first.
//!
//! A call reads the response store's head first and then the links and
//! comment events up to it, which share one sequence of positions, so an
//! item written during the call is left for the next call and none is
//! skipped. A comment event shows to everyone who can open the flare, with
//! the comment's text as it is now, or none after a delete. Each call opens every named flare and every response
//! again, so a cursor grants nothing by itself. A response the caller
//! cannot open is left out without a trace: the page holds no count of
//! hidden responses, and when the page is not cut short its cursor is the
//! store's head, which every response to any flare moves. A call that
//! reads past hidden responses reads on until its page holds `limit`
//! items or every response up to the head is read, so a page that says
//! more items follow is full.

use super::{FlareError, Flares, MAX_FLARES, flare_refused, once_each};
use crate::auth::Principal;
use crate::comment::{CommentAction, CommentBody, CommentEvent, CommentId};
use crate::name::PrincipalId;
use crate::response::{MAX_PAGE, Position, ResponseLink};
use crate::share::{Share, ShareId, ShareVersion};
use crate::sharing::ShareError;
use crate::store::{
    CommentStore, CommentStoreError, CursorStore, GrantStore, ResponseStore, ShareStore,
};
use crate::time::Timestamp;

/// One item of activity on a flare that the caller can open.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ActivityItem {
    /// A share that responds to the flare.
    Response(ResponseActivity),
    /// A comment, a reply, an edit or a delete.
    Comment(CommentActivity),
}

impl ActivityItem {
    /// The item's position, which a cursor counts.
    #[must_use]
    pub fn position(&self) -> Position {
        match self {
            Self::Response(item) => item.position,
            Self::Comment(item) => item.position,
        }
    }

    /// The response, when the item is one.
    #[must_use]
    pub fn as_response(&self) -> Option<&ResponseActivity> {
        match self {
            Self::Response(item) => Some(item),
            Self::Comment(_) => None,
        }
    }
}

/// A comment event on a flare that the caller can open.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct CommentActivity {
    /// What happened.
    pub action: CommentAction,
    /// The flare it is on.
    pub flare: ShareId,
    /// The comment.
    pub comment: CommentId,
    /// The comment it replies to, or `None` for a comment on the flare.
    pub parent: Option<CommentId>,
    /// The comment's author.
    pub by: PrincipalId,
    /// The comment's text now, or `None` once the author deleted it.
    pub body: Option<CommentBody>,
    /// When it happened.
    pub at: Timestamp,
    /// The item's position, which a cursor counts.
    pub position: Position,
}

/// One response to a flare that the caller can open.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ResponseActivity {
    /// The flare it responds to.
    pub flare: ShareId,
    /// The responding share.
    pub response: Share,
    /// The flare's version when the response named it.
    pub version: ShareVersion,
    /// When the response named the flare.
    pub at: Timestamp,
    /// The other flares the response responds to that the caller can open.
    pub also: Vec<Share>,
    /// The item's position, which a cursor counts.
    pub position: Position,
}

/// One page of activity.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityPage {
    /// The items, oldest first.
    pub items: Vec<ActivityItem>,
    /// The position to pass as `after` next time.
    pub last: Position,
    /// Whether more items may follow `last` now.
    pub more: bool,
}

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C: CommentStore, U: CursorStore>
    Flares<S, G, R, C, U>
{
    /// The responses to `flares` above `after` that the caller can open,
    /// oldest first, at most `limit` of them (1 to [`MAX_PAGE`]).
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::NotFound`] when the caller cannot open a named
    /// flare, [`FlareError::TooManyFlares`], [`FlareError::Share`] with
    /// [`ShareError::NoVerifiedEmail`] or [`ShareError::Unavailable`], and
    /// [`FlareError::ResponsesUnavailable`].
    pub async fn activity(
        &self,
        who: &Principal,
        flares: &[ShareId],
        after: Position,
        limit: usize,
    ) -> Result<ActivityPage, FlareError> {
        if who.email().is_none() {
            return Err(FlareError::Share(ShareError::NoVerifiedEmail));
        }
        let flares = once_each(flares.to_vec());
        if flares.len() > MAX_FLARES {
            return Err(FlareError::TooManyFlares);
        }
        for flare in &flares {
            self.shares
                .open(who, flare, None)
                .await
                .map_err(flare_refused)?;
        }
        let limit = limit.clamp(1, MAX_PAGE);
        let head = self
            .responses
            .head()
            .await
            .map_err(|_| FlareError::ResponsesUnavailable)?;
        let mut items = Vec::new();
        let mut last = after;
        loop {
            let (entries, cut_at) = self.read(&flares, last, head, limit).await?;
            for entry in entries {
                if items.len() == limit {
                    return Ok(ActivityPage {
                        items,
                        last,
                        more: true,
                    });
                }
                last = entry.position();
                if let Some(item) = self.item(who, entry).await? {
                    items.push(item);
                }
            }
            match cut_at {
                // A cut read leaves links above `cut_at`, so the call reads
                // on from there until the page fills or no read is cut.
                Some(position) if items.len() < limit => last = position,
                Some(position) => {
                    return Ok(ActivityPage {
                        items,
                        last: position,
                        more: true,
                    });
                }
                None => {
                    return Ok(ActivityPage {
                        items,
                        last: head.max(after),
                        more: false,
                    });
                }
            }
        }
    }

    /// The links and comment events of `flares` above `after` and up to
    /// `head`, oldest first, with the position where the earliest cut read
    /// stopped. Each read stops after `limit + 1` entries, so only the
    /// entries at or below that position are complete across every flare
    /// and every store, and the rest are dropped for the next read.
    async fn read(
        &self,
        flares: &[ShareId],
        after: Position,
        head: Position,
        limit: usize,
    ) -> Result<(Vec<Entry>, Option<Position>), FlareError> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut cut_at: Option<Position> = None;
        let bound = limit.saturating_add(1);
        for flare in flares {
            let links = self
                .responses
                .to_flare(flare, after, head, bound)
                .await
                .map_err(|_| FlareError::ResponsesUnavailable)?;
            let events = self
                .comments
                .events(flare, after, head, bound)
                .await
                .map_err(|_| FlareError::CommentsUnavailable)?;
            for (len, stop) in [
                (links.len(), links.last().map(|link| link.position)),
                (events.len(), events.last().map(|event| event.position)),
            ] {
                if len > limit
                    && let Some(stop) = stop
                {
                    cut_at = Some(cut_at.map_or(stop, |known| known.min(stop)));
                }
            }
            entries.extend(links.into_iter().map(Entry::Link));
            entries.extend(events.into_iter().map(Entry::Event));
        }
        if let Some(stop) = cut_at {
            entries.retain(|entry| entry.position() <= stop);
        }
        entries.sort_by_key(Entry::position);
        Ok((entries, cut_at))
    }

    /// The item for `entry`, or `None` when the caller cannot open the
    /// response.
    async fn item(
        &self,
        who: &Principal,
        entry: Entry,
    ) -> Result<Option<ActivityItem>, FlareError> {
        match entry {
            Entry::Link(link) => self.response_item(who, link).await,
            Entry::Event(event) => self.comment_item(event).await,
        }
    }

    /// The item for a comment event, with the comment's text as it is now.
    async fn comment_item(&self, event: CommentEvent) -> Result<Option<ActivityItem>, FlareError> {
        let comment = match self.comments.get(&event.comment).await {
            Ok(comment) => comment,
            Err(CommentStoreError::NotFound) => return Ok(None),
            Err(_) => return Err(FlareError::CommentsUnavailable),
        };
        Ok(Some(ActivityItem::Comment(CommentActivity {
            action: event.action,
            flare: event.flare,
            comment: event.comment,
            parent: event.parent,
            by: event.author,
            body: comment.body,
            at: event.at,
            position: event.position,
        })))
    }

    /// The item for `link`, or `None` when the caller cannot open the
    /// response.
    async fn response_item(
        &self,
        who: &Principal,
        link: ResponseLink,
    ) -> Result<Option<ActivityItem>, FlareError> {
        let response = match self.shares.open(who, &link.response, None).await {
            Ok(opened) => opened.share,
            Err(ShareError::NotFound) => return Ok(None),
            Err(other) => return Err(FlareError::Share(other)),
        };
        let others = self
            .responses
            .of_response(&link.response)
            .await
            .map_err(|_| FlareError::ResponsesUnavailable)?;
        let mut also = Vec::new();
        for other in others.iter().filter(|other| other.flare != link.flare) {
            match self.shares.open(who, &other.flare, None).await {
                Ok(opened) => also.push(opened.share),
                Err(ShareError::NotFound) => {}
                Err(error) => return Err(FlareError::Share(error)),
            }
        }
        Ok(Some(ActivityItem::Response(ResponseActivity {
            flare: link.flare,
            response,
            version: link.version,
            at: link.at,
            also,
            position: link.position,
        })))
    }
}

/// A link or a comment event, read together.
enum Entry {
    Link(ResponseLink),
    Event(CommentEvent),
}

impl Entry {
    fn position(&self) -> Position {
        match self {
            Self::Link(link) => link.position,
            Self::Event(event) => event.position,
        }
    }
}
