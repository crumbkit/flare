//! [`Flares::activity`]: the responses and comment events of one or more
//! flares since a position, oldest first.
//!
//! Responses come from the caller's own activity index (see the `index`
//! module), which holds an entry only for a response the caller could
//! open when the entry was written, so a call never reads a response
//! someone else hid from the caller. For each flare a call also reads the
//! index of each group through which the caller reads that flare: a team
//! grant on the flare of an active team the caller belongs to, and an
//! everyone grant whose audience covers the caller. A response that two
//! of those indexes list is listed once in a call, at the first of its
//! entries; when its entries fall in two calls, each call lists it.
//! Comment events come from the comment
//! store: a comment event shows to everyone who can open the flare, with
//! the comment's text as it is now, or none after a delete. Index entries,
//! links and comment events all take positions from one sequence, so a
//! call reads the response store's head first and then the entries and
//! events up to it, and an item written during the call is left for the
//! next call and none is skipped. Each call opens every named flare and
//! every response again, so neither a cursor nor an entry grants anything
//! by itself, and an entry whose response the caller can no longer open is
//! skipped without a trace. The page holds no count of hidden responses,
//! and when the page is not cut short its cursor is the store's head,
//! which every link, entry and comment event anywhere moves. A call that
//! reads past hidden responses reads on until its page holds `limit`
//! items or every item up to the head is read, so a page that says more
//! items follow is full unless the call examined its most.
//!
//! An entry's position is its own, so a response the caller gains access
//! to after their cursor shows as new activity, even when its link is
//! older than the cursor.
//!
//! One call examines at most [`MAX_EXAMINED`] index entries and comment
//! events together. Only entries whose responses the caller can no longer
//! open can fill that many without filling the page, so a page cut by it
//! can be short and say that more follow.

use super::{FlareError, Flares, MAX_FLARES, flare_refused, once_each};
use crate::auth::Principal;
use crate::comment::{CommentAction, CommentBody, CommentEvent, CommentId};
use crate::name::PrincipalId;
use crate::response::{IndexEntry, MAX_PAGE, Position, Reader};
use crate::share::{Share, ShareId, ShareVersion};
use crate::sharing::ShareError;
use crate::store::{
    CommentStore, CommentStoreError, CursorStore, GrantStore, ResponseStore, ShareStore,
};
use crate::teams::Teams;
use crate::time::Timestamp;
use std::collections::HashSet;

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

/// The most index entries and comment events, counted together, that one
/// activity call examines before it answers the items it found, with
/// `more` set.
pub const MAX_EXAMINED: usize = 1_000;

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

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C: CommentStore, U: CursorStore, T: Teams>
    Flares<S, G, R, C, U, T>
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
        let person = who
            .email()
            .ok_or(FlareError::Share(ShareError::NoVerifiedEmail))?;
        let mut readers = Vec::with_capacity(flares.len());
        for flare in &flares {
            let mut of_flare = vec![Reader::Person(person.principal_id())];
            let groups = self
                .shares
                .groups_reading(person, flare)
                .await
                .map_err(FlareError::Share)?;
            of_flare.extend(groups.into_iter().map(Reader::Group));
            readers.push((flare.clone(), of_flare));
        }
        let head = self
            .responses
            .head()
            .await
            .map_err(|_| FlareError::ResponsesUnavailable)?;
        let mut items = Vec::new();
        let mut last = after;
        let mut examined = 0_usize;
        let mut listed: HashSet<(ShareId, ShareId)> = HashSet::new();
        loop {
            let (entries, cut_at) = self.read(&readers, last, head, limit).await?;
            for entry in entries {
                if items.len() == limit || examined == MAX_EXAMINED {
                    return Ok(ActivityPage {
                        items,
                        last,
                        more: true,
                    });
                }
                examined = examined.saturating_add(1);
                last = entry.position();
                if let Entry::Index(indexed) = &entry
                    && !listed.insert((indexed.response.clone(), indexed.flare.clone()))
                {
                    continue;
                }
                if let Some(item) = self.item(who, entry).await? {
                    items.push(item);
                }
            }
            match cut_at {
                // A cut read leaves entries above `cut_at`, so the call
                // reads on from there until the page fills, the call has
                // examined its most, or no read is cut.
                Some(position) if items.len() < limit && examined < MAX_EXAMINED => {
                    last = position;
                }
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

    /// The index entries of each flare's readers and the comment events
    /// on each flare above `after` and up to `head`, oldest first, with the position
    /// where the earliest cut read stopped. Each read stops after
    /// `limit + 1` items, so only the items at or below that position are
    /// complete across every flare and every store, and the rest are
    /// dropped for the next read.
    async fn read(
        &self,
        readers: &[(ShareId, Vec<Reader>)],
        after: Position,
        head: Position,
        limit: usize,
    ) -> Result<(Vec<Entry>, Option<Position>), FlareError> {
        let mut entries: Vec<Entry> = Vec::new();
        let mut cut_at: Option<Position> = None;
        let bound = limit.saturating_add(1);
        for (flare, of_flare) in readers {
            let mut reads = Vec::with_capacity(of_flare.len().saturating_add(1));
            for reader in of_flare {
                let indexed = self
                    .responses
                    .visible(reader, flare, after, head, bound)
                    .await
                    .map_err(|_| FlareError::ResponsesUnavailable)?;
                reads.push((indexed.len(), indexed.last().map(|entry| entry.position)));
                entries.extend(indexed.into_iter().map(Entry::Index));
            }
            let events = self
                .comments
                .events(flare, after, head, bound)
                .await
                .map_err(|_| FlareError::CommentsUnavailable)?;
            reads.push((events.len(), events.last().map(|event| event.position)));
            for (len, stop) in reads {
                if len > limit
                    && let Some(stop) = stop
                {
                    cut_at = Some(cut_at.map_or(stop, |known| known.min(stop)));
                }
            }
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
            Entry::Index(indexed) => self.response_item(who, indexed).await,
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

    /// The item for `entry`, or `None` when the caller cannot open the
    /// response.
    async fn response_item(
        &self,
        who: &Principal,
        entry: IndexEntry,
    ) -> Result<Option<ActivityItem>, FlareError> {
        let response = match self.shares.open(who, &entry.response, None).await {
            Ok(opened) => opened.share,
            Err(ShareError::NotFound) => return Ok(None),
            Err(other) => return Err(FlareError::Share(other)),
        };
        let others = self
            .responses
            .of_response(&entry.response)
            .await
            .map_err(|_| FlareError::ResponsesUnavailable)?;
        let mut also = Vec::new();
        for other in others.iter().filter(|other| other.flare != entry.flare) {
            match self.shares.open(who, &other.flare, None).await {
                Ok(opened) => also.push(opened.share),
                Err(ShareError::NotFound) => {}
                Err(error) => return Err(FlareError::Share(error)),
            }
        }
        Ok(Some(ActivityItem::Response(ResponseActivity {
            flare: entry.flare,
            response,
            version: entry.version,
            at: entry.at,
            also,
            position: entry.position,
        })))
    }
}

/// An index entry or a comment event, read together.
enum Entry {
    Index(IndexEntry),
    Event(CommentEvent),
}

impl Entry {
    fn position(&self) -> Position {
        match self {
            Self::Index(entry) => entry.position,
            Self::Event(event) => event.position,
        }
    }
}
