//! The form token table, in memory behind one mutex. A row is keyed by the
//! SHA-256 of its token and ties the token to the session, the binding,
//! the slot, the record and the blob the form was drawn from.

use crate::secret::{self, Digest};
use hangar_core::{BindingName, Blob, RecordPath, SlotName};
use hangar_render::FormToken;
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// What a token was issued for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Tied {
    /// The digest of the session secret that drew the form.
    pub(crate) session: Digest,
    /// The binding the form edits through.
    pub(crate) binding: BindingName,
    /// The slot the form edits.
    pub(crate) slot: SlotName,
    /// The record, taken from the binding's fills.
    pub(crate) path: RecordPath,
    /// The blob the form was drawn from.
    pub(crate) blob: Blob,
}

#[derive(Debug)]
struct Row {
    digest: Digest,
    tied: Tied,
    issued: Instant,
}

/// The table after a panic poisoned its lock. Its routes answer 503
/// `forms::unavailable`, because a half-changed table is never read as
/// good.
#[derive(Debug)]
pub(crate) struct Unusable;

/// The form tokens this process issued, oldest first.
#[derive(Debug)]
pub(crate) struct Forms {
    rows: Mutex<VecDeque<Row>>,
    capacity: usize,
    /// The most rows one session keeps: an eighth of the table, so one
    /// session that draws many forms cannot push out another's tokens
    /// until eight sessions fill the table.
    per_session: usize,
    life: Duration,
}

impl Forms {
    /// A table of at most `capacity` rows, each living `life`. One session
    /// keeps at most an eighth of them.
    pub(crate) fn new(capacity: usize, life: Duration) -> Self {
        Self {
            rows: Mutex::new(VecDeque::new()),
            capacity,
            per_session: (capacity / 8).max(1),
            life,
        }
    }

    fn rows(&self) -> Result<MutexGuard<'_, VecDeque<Row>>, Unusable> {
        self.rows.lock().map_err(|_| Unusable)
    }

    /// Issues a token for `tied`. A session that holds its share of rows
    /// drops its own oldest row, and a full table drops its oldest row.
    /// `None` when the system gives no random bytes or the table is
    /// unusable.
    pub(crate) fn issue(&self, tied: Tied, now: Instant) -> Option<FormToken> {
        let secret = secret::random()?;
        let mut rows = self.rows().ok()?;
        let own = rows
            .iter()
            .filter(|row| row.tied.session == tied.session)
            .count();
        if own >= self.per_session
            && let Some(at) = rows.iter().position(|row| row.tied.session == tied.session)
        {
            rows.remove(at);
        }
        while !rows.is_empty() && rows.len() >= self.capacity {
            rows.pop_front();
        }
        rows.push_back(Row {
            digest: secret::digest_of(&secret),
            tied,
            issued: now,
        });
        Some(secret::form_token(&secret))
    }

    /// Removes the row of `token` and returns what it was tied to, when it
    /// is there and younger than its life. The token is spent either way.
    pub(crate) fn take(&self, token: &FormToken, now: Instant) -> Result<Option<Tied>, Unusable> {
        let digest = secret::digest_of_token(token);
        let mut rows = self.rows()?;
        let Some(row) = rows
            .iter()
            .position(|row| secret::same(&row.digest, &digest))
            .and_then(|at| rows.remove(at))
        else {
            return Ok(None);
        };
        Ok((now.saturating_duration_since(row.issued) < self.life).then_some(row.tied))
    }

    /// Drops the rows older than their life.
    pub(crate) fn expire(&self, now: Instant) {
        if let Ok(mut rows) = self.rows() {
            rows.retain(|row| now.saturating_duration_since(row.issued) < self.life);
        }
    }
}

#[cfg(test)]
mod tests;
