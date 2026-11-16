//! The hand-over code table. `localhost` never sets the session cookie:
//! after an OpenID Connect sign-in, the callback stores the session under a
//! one-use code, and the launch host's finish route redeems the code and
//! sets the cookie on its own host.

use crate::secret::{self, Digest};
use hangar_core::Credential;
use secrecy::SecretString;
use std::collections::VecDeque;
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// What a code hands over.
#[derive(Debug)]
pub(crate) struct Handed {
    /// The OpenID Connect session.
    pub(crate) session: Credential,
    /// The local path the sign-in returns to.
    pub(crate) return_to: String,
    /// How long the session lasts from when the code was issued.
    pub(crate) lasts: Duration,
}

#[derive(Debug)]
struct Row {
    digest: Digest,
    handed: Handed,
    issued: Instant,
}

/// What `redeem` found.
#[derive(Debug)]
pub(crate) enum Redeemed {
    /// The code worked.
    Handed(Handed),
    /// The code was older than its life. Its session is the caller's to
    /// sign out.
    Expired(Credential),
    /// No row has the code: never issued, used, dropped or swept.
    Unknown,
    /// A panic poisoned the table, so a half-changed table is never read
    /// as good. The finish route answers 503.
    Unusable,
}

/// What `issue` gives.
#[derive(Debug)]
pub(crate) struct Issued {
    /// The code, for the finish URL.
    pub(crate) code: SecretString,
    /// The session of the oldest row, dropped because the table was full;
    /// the caller signs it out.
    pub(crate) dropped: Option<Credential>,
}

/// The hand-over codes this process issued, oldest first.
#[derive(Debug)]
pub(crate) struct Handover {
    rows: Mutex<VecDeque<Row>>,
    capacity: usize,
    life: Duration,
}

impl Handover {
    /// A table of at most `capacity` codes, each working for `life`.
    pub(crate) fn new(capacity: usize, life: Duration) -> Self {
        Self {
            rows: Mutex::new(VecDeque::new()),
            capacity,
            life,
        }
    }

    fn rows(&self) -> Option<MutexGuard<'_, VecDeque<Row>>> {
        self.rows.lock().ok()
    }

    /// Stores `handed` under a new code. `None` without random bytes or
    /// after a panic poisoned the table.
    pub(crate) fn issue(&self, handed: Handed, now: Instant) -> Option<Issued> {
        let code = secret::random()?;
        let mut rows = self.rows()?;
        let mut dropped = None;
        if !rows.is_empty() && rows.len() >= self.capacity {
            dropped = rows.pop_front().map(|row| row.handed.session);
        }
        rows.push_back(Row {
            digest: secret::digest_of(&code),
            handed,
            issued: now,
        });
        Some(Issued { code, dropped })
    }

    /// Removes the row of `code`. A code works once, while it is younger
    /// than its life.
    pub(crate) fn redeem(&self, code: &str, now: Instant) -> Redeemed {
        let digest = secret::digest_of_text(code);
        let Some(mut rows) = self.rows() else {
            return Redeemed::Unusable;
        };
        let Some(row) = rows
            .iter()
            .position(|row| secret::same(&row.digest, &digest))
            .and_then(|at| rows.remove(at))
        else {
            return Redeemed::Unknown;
        };
        let age = now.saturating_duration_since(row.issued);
        if age < self.life {
            Redeemed::Handed(Handed {
                lasts: row.handed.lasts.saturating_sub(age),
                ..row.handed
            })
        } else {
            Redeemed::Expired(row.handed.session)
        }
    }

    /// Drops the codes older than their life and returns their sessions,
    /// which the caller signs out.
    pub(crate) fn expire(&self, now: Instant) -> Vec<Credential> {
        let Some(mut rows) = self.rows() else {
            return Vec::new();
        };
        let mut expired = Vec::new();
        let mut kept = VecDeque::with_capacity(rows.len());
        for row in rows.drain(..) {
            if now.saturating_duration_since(row.issued) < self.life {
                kept.push_back(row);
            } else {
                expired.push(row.handed.session);
            }
        }
        *rows = kept;
        expired
    }
}

#[cfg(test)]
mod tests;
