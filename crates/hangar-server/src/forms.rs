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

/// The form tokens this process issued, oldest first.
#[derive(Debug)]
pub(crate) struct Forms {
    rows: Mutex<VecDeque<Row>>,
    capacity: usize,
    life: Duration,
}

impl Forms {
    /// A table of at most `capacity` rows, each living `life`.
    pub(crate) fn new(capacity: usize, life: Duration) -> Self {
        Self {
            rows: Mutex::new(VecDeque::new()),
            capacity,
            life,
        }
    }

    /// `None` after a panic poisoned the table, so a half-changed table is
    /// never read as good.
    fn rows(&self) -> Option<MutexGuard<'_, VecDeque<Row>>> {
        self.rows.lock().ok()
    }

    /// Issues a token for `tied`. A full table drops its oldest row. `None`
    /// when the system gives no random bytes or the table is unusable.
    pub(crate) fn issue(&self, tied: Tied, now: Instant) -> Option<FormToken> {
        let secret = secret::random()?;
        let mut rows = self.rows()?;
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
    pub(crate) fn take(&self, token: &FormToken, now: Instant) -> Option<Tied> {
        let digest = secret::digest_of_token(token);
        let mut rows = self.rows()?;
        let at = rows
            .iter()
            .position(|row| secret::same(&row.digest, &digest))?;
        let row = rows.remove(at)?;
        (now.saturating_duration_since(row.issued) < self.life).then_some(row.tied)
    }

    /// Drops the rows older than their life.
    pub(crate) fn expire(&self, now: Instant) {
        if let Some(mut rows) = self.rows() {
            rows.retain(|row| now.saturating_duration_since(row.issued) < self.life);
        }
    }
}
