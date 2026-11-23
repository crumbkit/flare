//! [`Timestamp`], a moment that a caller supplies.

/// A moment as whole seconds since the Unix epoch. A caller reads its clock
/// and passes the moment in, so a store reads no clock and a test needs
/// none.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Timestamp(u64);

impl Timestamp {
    /// The moment `seconds` after the Unix epoch.
    #[must_use]
    pub const fn from_secs(seconds: u64) -> Self {
        Self(seconds)
    }

    /// The seconds since the Unix epoch.
    #[must_use]
    pub const fn as_secs(self) -> u64 {
        self.0
    }
}
