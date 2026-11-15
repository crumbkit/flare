//! The steps of a server file read, and the switches that the `testing`
//! feature exposes: a hook that runs at each step, and a user id to read
//! as. Without the feature, a read runs no hook and reads as this process's
//! user.

use super::StateDir;
use super::files::user;
use std::fmt;
use std::io;
use std::path::Path;
#[cfg(feature = "testing")]
use std::sync::Arc;

/// A step of [`StateDir::read_server`] at which the `testing` feature's
/// hook runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ReadStep {
    /// The directory and the file passed the `lstat` checks, and the file
    /// is not yet open.
    Checked,
    /// The file is open, and its device, inode, type, mode and links are
    /// not yet checked.
    Opened,
}

/// The hook a test sets with [`StateDir::with_read_hook`].
#[cfg(feature = "testing")]
type ReadHook = Arc<dyn Fn(ReadStep, &Path) + Send + Sync>;

/// The test switches of one state directory. Without the `testing`
/// feature it holds nothing.
#[derive(Clone, Default)]
pub(super) struct Switches {
    #[cfg(feature = "testing")]
    hook: Option<ReadHook>,
    #[cfg(feature = "testing")]
    user: Option<u32>,
}

impl Switches {
    /// Runs the hook, if one is set, at `step` of a read of `path`.
    pub(super) fn at(&self, step: ReadStep, path: &Path) {
        #[cfg(feature = "testing")]
        if let Some(hook) = &self.hook {
            hook(step, path);
        }
        #[cfg(not(feature = "testing"))]
        let _ = (step, path);
    }

    /// The user id the checks compare owners with.
    pub(super) fn user(&self) -> io::Result<u32> {
        #[cfg(feature = "testing")]
        if let Some(user) = self.user {
            return Ok(user);
        }
        user()
    }
}

impl fmt::Debug for Switches {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("Switches").finish_non_exhaustive()
    }
}

#[cfg(feature = "testing")]
impl StateDir {
    /// Runs `hook` at each [`ReadStep`] of every later `read_server`, so a
    /// test can change the file between the steps. Only the `testing`
    /// feature has it.
    #[must_use]
    pub fn with_read_hook(
        mut self,
        hook: impl Fn(ReadStep, &Path) + Send + Sync + 'static,
    ) -> Self {
        self.switches.hook = Some(Arc::new(hook));
        self
    }

    /// Checks owners against `uid` in place of this process's user, so a
    /// test can see how a directory of another user is refused. Only the
    /// `testing` feature has it.
    #[must_use]
    pub fn as_user(mut self, uid: u32) -> Self {
        self.switches.user = Some(uid);
        self
    }
}
