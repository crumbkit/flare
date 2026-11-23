//! [`Flares::activity`]: the responses to one or more flares since a
//! position, oldest first.
//!
//! A call reads the response store's head first and then the links up to
//! it, so a link written during the call is left for the next call and
//! none is skipped. Each call opens every named flare and every response
//! again, so a cursor grants nothing by itself. A response the caller
//! cannot open is left out without a trace: the page holds no count of
//! hidden responses, and when the page is not cut short its cursor is the
//! store's head, which every response to any flare moves. A call that
//! reads past hidden responses reads on until its page holds `limit`
//! items or every response up to the head is read, so a page that says
//! more items follow is full.

use super::{FlareError, Flares, MAX_FLARES, flare_refused, once_each};
use crate::auth::Principal;
use crate::response::{MAX_PAGE, Position, ResponseLink};
use crate::share::{Share, ShareId, ShareVersion};
use crate::sharing::ShareError;
use crate::store::{GrantStore, ResponseStore, ShareStore};
use crate::teams::Teams;
use crate::time::Timestamp;

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

impl<S: ShareStore, G: GrantStore, R: ResponseStore, T: Teams> Flares<S, G, R, T> {
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
            let (links, cut_at) = self.read(&flares, last, head, limit).await?;
            for link in links {
                if items.len() == limit {
                    return Ok(ActivityPage {
                        items,
                        last,
                        more: true,
                    });
                }
                last = link.position;
                if let Some(item) = self.item(who, link).await? {
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

    /// The links to `flares` above `after` and up to `head`, oldest first,
    /// with the position where the earliest cut read stopped. Each flare's
    /// read stops after `limit + 1` links, so only the links at or below
    /// that position are complete across every flare, and the rest are
    /// dropped for the next read.
    async fn read(
        &self,
        flares: &[ShareId],
        after: Position,
        head: Position,
        limit: usize,
    ) -> Result<(Vec<ResponseLink>, Option<Position>), FlareError> {
        let mut links: Vec<ResponseLink> = Vec::new();
        let mut cut_at: Option<Position> = None;
        for flare in flares {
            let read = self
                .responses
                .to_flare(flare, after, head, limit.saturating_add(1))
                .await
                .map_err(|_| FlareError::ResponsesUnavailable)?;
            if read.len() > limit
                && let Some(stop) = read.last().map(|link| link.position)
            {
                cut_at = Some(cut_at.map_or(stop, |known| known.min(stop)));
            }
            links.extend(read);
        }
        if let Some(stop) = cut_at {
            links.retain(|link| link.position <= stop);
        }
        links.sort_by_key(|link| link.position);
        Ok((links, cut_at))
    }

    /// The item for `link`, or `None` when the caller cannot open the
    /// response.
    async fn item(
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
        Ok(Some(ActivityItem {
            flare: link.flare,
            response,
            version: link.version,
            at: link.at,
            also,
            position: link.position,
        }))
    }
}
