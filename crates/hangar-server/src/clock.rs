//! The clock that the server's tables and the loopback authenticator read.

use std::time::{Instant, SystemTime};

/// Tells the server the time. [`SystemClock`] reads the system clock, and
/// tests implement the trait to move time by hand.
pub trait Clock: Send + Sync + 'static {
    /// The monotonic time now, for launch codes, form tokens and hand-over
    /// codes.
    fn now(&self) -> Instant;

    /// The wall-clock time now, for the state directory's sweep of old
    /// files.
    fn wall(&self) -> SystemTime;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::now()
    }
}
