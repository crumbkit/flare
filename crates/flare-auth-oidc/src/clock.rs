//! The clock that pending sign-ins, sessions, the key set cache and the ID
//! token expiry check read.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Tells the authenticator the time. [`SystemClock`] reads the system clock,
/// and tests move a clock of their own.
pub trait Clock: Send + Sync + 'static {
    /// The time now.
    fn now(&self) -> SystemTime;
}

/// The system clock.
#[derive(Debug, Clone, Copy, Default)]
#[non_exhaustive]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// The whole seconds from the Unix epoch to `time`, or zero for a time
/// before the epoch.
pub(crate) fn unix_seconds(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// Whether `since` lies at least `age` before `now`. A `since` after `now`,
/// which a system clock that moved back gives, counts as no time passed.
pub(crate) fn has_passed(since: SystemTime, age: Duration, now: SystemTime) -> bool {
    now.duration_since(since)
        .is_ok_and(|elapsed| elapsed >= age)
}

#[cfg(feature = "testing")]
pub use manual::TestClock;

#[cfg(feature = "testing")]
mod manual {
    use super::Clock;
    use std::sync::{Arc, Mutex, PoisonError};
    use std::time::{Duration, SystemTime};

    /// A clock that starts at the system time and moves only when a test
    /// calls [`TestClock::advance`]. Clones share one time.
    #[derive(Debug, Clone)]
    pub struct TestClock(Arc<Mutex<SystemTime>>);

    impl TestClock {
        /// A clock set to the system time now.
        #[must_use]
        pub fn new() -> Self {
            Self(Arc::new(Mutex::new(SystemTime::now())))
        }

        /// Moves the clock forward by `by`.
        pub fn advance(&self, by: Duration) {
            let mut now = self.0.lock().unwrap_or_else(PoisonError::into_inner);
            *now += by;
        }
    }

    impl Default for TestClock {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Clock for TestClock {
        fn now(&self) -> SystemTime {
            *self.0.lock().unwrap_or_else(PoisonError::into_inner)
        }
    }
}
