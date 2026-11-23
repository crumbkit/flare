//! [`Flares::activity`]: the responses to one or more flares since a
//! position, oldest first.
//!
//! A call reads the caller's own activity index (see the `index` module),
//! which holds an entry only for a response the caller could open when the
//! entry was written, so it never reads a response someone else hid from
//! the caller. A call reads the response store's head first and then the
//! entries up to it, so an entry written during the call is left for the
//! next call and none is skipped. Each call opens every named flare and
//! every response again, so neither a cursor nor an entry grants anything
//! by itself, and an entry whose response the caller can no longer open is
//! skipped without a trace. The page holds no count of hidden responses,
//! and when the page is not cut short its cursor is the store's head,
//! which every link and entry anywhere moves.
//!
//! An entry's position is its own, so a response the caller gains access
//! to after their cursor shows as new activity, even when its link is
//! older than the cursor.
//!
//! One call examines at most [`MAX_EXAMINED`] entries. Only entries the
//! caller can no longer open can fill that many without filling the page,
//! so a page cut by it can be short and say that more follow.

use super::{FlareError, Flares, MAX_FLARES, flare_refused, once_each};
use crate::auth::Principal;
use crate::response::{IndexEntry, MAX_PAGE, Position, Reader};
use crate::share::{Share, ShareId, ShareVersion};
use crate::sharing::ShareError;
use crate::store::{GrantStore, ResponseStore, ShareStore};
use crate::time::Timestamp;

/// The most index entries one activity call examines before it answers
/// the items it found, with `more` set.
pub const MAX_EXAMINED: usize = 1_000;

/// One response to a flare that the caller can open.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct ActivityItem {
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

impl<S: ShareStore, G: GrantStore, R: ResponseStore> Flares<S, G, R> {
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
        let reader = Reader::Person(
            who.email()
                .map(crate::Email::principal_id)
                .ok_or(FlareError::Share(ShareError::NoVerifiedEmail))?,
        );
        let head = self
            .responses
            .head()
            .await
            .map_err(|_| FlareError::ResponsesUnavailable)?;
        let mut items = Vec::new();
        let mut last = after;
        let mut examined = 0_usize;
        loop {
            let (entries, cut_at) = self.read(&reader, &flares, last, head, limit).await?;
            for entry in entries {
                if items.len() == limit || examined == MAX_EXAMINED {
                    return Ok(ActivityPage {
                        items,
                        last,
                        more: true,
                    });
                }
                examined = examined.saturating_add(1);
                last = entry.position;
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

    /// The caller's entries on `flares` above `after` and up to `head`,
    /// oldest first, with the position where the earliest cut read
    /// stopped. Each flare's read stops after `limit + 1` entries, so only
    /// the entries at or below that position are complete across every
    /// flare, and the rest are dropped for the next read.
    async fn read(
        &self,
        reader: &Reader,
        flares: &[ShareId],
        after: Position,
        head: Position,
        limit: usize,
    ) -> Result<(Vec<IndexEntry>, Option<Position>), FlareError> {
        let mut entries: Vec<IndexEntry> = Vec::new();
        let mut cut_at: Option<Position> = None;
        for flare in flares {
            let read = self
                .responses
                .visible(reader, flare, after, head, limit.saturating_add(1))
                .await
                .map_err(|_| FlareError::ResponsesUnavailable)?;
            if read.len() > limit
                && let Some(stop) = read.last().map(|entry| entry.position)
            {
                cut_at = Some(cut_at.map_or(stop, |known| known.min(stop)));
            }
            entries.extend(read);
        }
        if let Some(stop) = cut_at {
            entries.retain(|entry| entry.position <= stop);
        }
        entries.sort_by_key(|entry| entry.position);
        Ok((entries, cut_at))
    }

    /// The item for `entry`, or `None` when the caller cannot open the
    /// response.
    async fn item(
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
        Ok(Some(ActivityItem {
            flare: entry.flare,
            response,
            version: entry.version,
            at: entry.at,
            also,
            position: entry.position,
        }))
    }
}
