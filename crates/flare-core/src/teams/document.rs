//! [`DocumentTeams`]: read-only [`Teams`] over a teams document that the
//! deployment's own sync job writes, such as an export of its directory.
//!
//! The document is read once by [`DocumentTeams::load`], and again by the
//! first call that finds the copy in memory [`DOCUMENT_MAX_AGE`] old or
//! older. No background task runs, so the same code serves a long-running
//! process and one that a platform freezes between requests. While one
//! call reads the document, other calls answer from the copy they find.
//! A document refused on a later read leaves the last good copy in place,
//! and the hook that [`DocumentTeams::on_refused`] sets is told why; the
//! next read is tried once the kept copy is that old again.
//!
//! A team is active exactly while the document lists it. A team that
//! leaves the document opens nothing, and its grants are dormant; when the
//! name returns, its grants open again to the members the document then
//! lists.

use super::directory::{Directory, DocumentRefused, DocumentUnreadable};
use super::names::TeamName;
use super::{Teams, TeamsError};
use crate::person::{IdentityKind, Person};
use std::fmt;
use std::future::Future;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// How old the copy in memory may be before a call reads the document
/// again. Two processes of one deployment may disagree for up to this long
/// after the document changes.
pub const DOCUMENT_MAX_AGE: Duration = Duration::from_secs(60);

/// Where a teams document is read from, such as a file or an object.
pub trait TeamsDocument: Send + Sync {
    /// The document's bytes. A reader may stop after
    /// [`MAX_DOCUMENT_BYTES`](super::MAX_DOCUMENT_BYTES) plus one bytes,
    /// which is refused as too large either way.
    ///
    /// # Errors
    ///
    /// Returns [`DocumentUnreadable`] with a reason that holds none of the
    /// document's text.
    fn read(&self) -> impl Future<Output = Result<Vec<u8>, DocumentUnreadable>> + Send;
}

/// The hook that a refused read is reported to.
type Report = Box<dyn Fn(&DocumentRefused) + Send + Sync>;

/// The copy in memory and when it was read.
struct Held {
    copy: Arc<Directory>,
    read_at: Instant,
    reading: bool,
}

/// Teams read from a [`TeamsDocument`]. flare does not manage them: every
/// change answers [`TeamsError::ManagedElsewhere`]. A member reads a team
/// and its members; anyone else gets [`TeamsError::NotFound`].
///
/// `N` is the clock that ages the copy, [`Instant::now`] unless a test sets
/// another with [`DocumentTeams::with_clock`].
pub struct DocumentTeams<D, N = fn() -> Instant> {
    document: D,
    kind: IdentityKind,
    now: N,
    held: Mutex<Held>,
    report: Option<Report>,
}

impl<D, N> fmt::Debug for DocumentTeams<D, N> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DocumentTeams")
            .field("kind", &self.kind)
            .finish_non_exhaustive()
    }
}

/// Reads and checks the document.
async fn read<D: TeamsDocument>(
    document: &D,
    kind: IdentityKind,
) -> Result<Directory, DocumentRefused> {
    let bytes = document.read().await.map_err(DocumentRefused::Unreadable)?;
    Directory::parse(&bytes, kind)
}

impl<D: TeamsDocument> DocumentTeams<D> {
    /// Reads `document`, whose members are people of `kind`.
    ///
    /// # Errors
    ///
    /// Returns the [`DocumentRefused`] that names why the document could
    /// not be used.
    pub async fn load(document: D, kind: IdentityKind) -> Result<Self, DocumentRefused> {
        let copy = read(&document, kind).await?;
        Ok(Self {
            document,
            kind,
            now: Instant::now,
            held: Mutex::new(Held {
                copy: Arc::new(copy),
                read_at: Instant::now(),
                reading: false,
            }),
            report: None,
        })
    }
}

impl<D: TeamsDocument, N: Fn() -> Instant + Send + Sync> DocumentTeams<D, N> {
    /// The same teams aged by the clock `now`, with the copy counted as
    /// read at `now()`.
    #[must_use]
    pub fn with_clock<M: Fn() -> Instant + Send + Sync>(self, now: M) -> DocumentTeams<D, M> {
        let Self {
            document,
            kind,
            held,
            report,
            ..
        } = self;
        let mut held = held
            .into_inner()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        held.read_at = now();
        DocumentTeams {
            document,
            kind,
            now,
            held: Mutex::new(held),
            report,
        }
    }

    /// The same teams, telling `report` why each later read was refused.
    #[must_use]
    pub fn on_refused(self, report: impl Fn(&DocumentRefused) + Send + Sync + 'static) -> Self {
        Self {
            report: Some(Box::new(report)),
            ..self
        }
    }

    /// The copy to answer from: the one in memory, or a new one when this
    /// call found it old and read the document.
    async fn current(&self) -> Result<Arc<Directory>, TeamsError> {
        let (copy, stale) = {
            let mut held = self.held.lock().map_err(|_| TeamsError::Unavailable)?;
            let now = (self.now)();
            let stale =
                !held.reading && now.saturating_duration_since(held.read_at) >= DOCUMENT_MAX_AGE;
            if stale {
                held.reading = true;
            }
            (Arc::clone(&held.copy), stale)
        };
        if !stale {
            return Ok(copy);
        }
        let reading = Reading(&self.held);
        let outcome = read(&self.document, self.kind).await;
        let mut held = self.held.lock().map_err(|_| TeamsError::Unavailable)?;
        held.read_at = (self.now)();
        match outcome {
            Ok(fresh) => held.copy = Arc::new(fresh),
            Err(refusal) => {
                if let Some(report) = &self.report {
                    report(&refusal);
                }
            }
        }
        drop(held);
        drop(reading);
        Ok(self.copy())
    }

    /// The copy in memory.
    fn copy(&self) -> Arc<Directory> {
        let held = self
            .held
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        Arc::clone(&held.copy)
    }
}

/// Clears the reading mark when the call that read the document ends,
/// also when its future is dropped before the read finished.
struct Reading<'a>(&'a Mutex<Held>);

impl Drop for Reading<'_> {
    fn drop(&mut self) {
        let mut held = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        held.reading = false;
    }
}

impl<D: TeamsDocument, N: Fn() -> Instant + Send + Sync> Teams for DocumentTeams<D, N> {
    async fn teams_of(&self, person: &Person) -> Result<Vec<TeamName>, TeamsError> {
        Ok(self.current().await?.teams_of(person))
    }

    async fn exists(&self, team: &TeamName) -> Result<bool, TeamsError> {
        Ok(self.current().await?.exists(team))
    }

    async fn members(&self, team: &TeamName) -> Result<Vec<Person>, TeamsError> {
        self.current()
            .await?
            .members(team)
            .map(<[Person]>::to_vec)
            .ok_or(TeamsError::NotFound)
    }
}
