//! Launch codes and the sessions they start, kept in one table behind one
//! lock.

use crate::request::SESSION_COOKIE;
use crate::secret::{compose, digest_of, is_hex, same, sha256};
use flare_core::Credential;
use secrecy::SecretString;
use std::collections::VecDeque;
use std::fmt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How long a launch code works after it is made.
pub(crate) const CODE_LIFE: Duration = Duration::from_secs(60);
/// How long the digest of a code is kept after it is made, so a late use
/// is told `Used` or `Expired` and not `Unknown`.
const REMEMBER: Duration = Duration::from_secs(10 * 60);
/// How many codes may be unused and unexpired at once.
const MAX_OUTSTANDING: usize = 64;
/// How many sessions may be live at once. A new one past this replaces the
/// oldest.
const MAX_SESSIONS: usize = 64;
/// The longest return path in bytes.
const MAX_RETURN_PATH: usize = 2048;

/// A path on this server to send the browser to after it signs in, such
/// as `/b/name?x=1`. It starts with one `/`, and holds only visible ASCII
/// with no backslash, so it can never name another host.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReturnPath(String);

impl ReturnPath {
    /// The path as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for ReturnPath {
    type Error = ReturnPathError;

    fn try_from(text: &str) -> Result<Self, ReturnPathError> {
        if text.len() > MAX_RETURN_PATH {
            return Err(ReturnPathError::TooLong);
        }
        let on_this_server = text.starts_with('/') && !text.starts_with("//");
        let plain = text
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'\\');
        if on_this_server && plain {
            Ok(Self(text.to_owned()))
        } else {
            Err(ReturnPathError::Malformed)
        }
    }
}

/// Why a return path was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReturnPathError {
    /// The path is longer than 2,048 bytes.
    #[error("the return path is longer than 2048 bytes")]
    TooLong,
    /// The path is empty, does not start with exactly one `/`, or holds a
    /// space, a control character, a non-ASCII character or a backslash.
    #[error(
        "the return path must start with one slash and hold only visible ASCII without a backslash"
    )]
    Malformed,
}

/// A launch code: 32 random bytes as 64 hex digits. It works once, for 60
/// seconds. The crate keeps only its SHA-256.
#[derive(Debug)]
pub struct LaunchCode {
    secret: SecretString,
    digest: [u8; 32],
}

impl LaunchCode {
    /// The code.
    #[must_use]
    pub fn secret(&self) -> &SecretString {
        &self.secret
    }
}

/// Why no code was issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CodeError {
    /// 64 codes are unused and unexpired. One frees its place when it is
    /// used or 60 seconds after it was made.
    #[error("64 launch codes are outstanding; try again within a minute")]
    TooMany,
    /// The operating system gave no random bytes.
    #[error("the operating system gave no random bytes")]
    Random,
    /// The server's state is unusable after a panic.
    #[error("the server cannot issue codes")]
    Unavailable,
}

/// Why a launch code did not sign the browser in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignInError {
    /// The server never issued the code, or forgot it 10 minutes after
    /// making it.
    #[error("the launch code is unknown; run flare open again")]
    Unknown,
    /// The code was used already.
    #[error("the launch code was used already; run flare open again")]
    Used,
    /// The code is more than 60 seconds old.
    #[error("the launch code expired; run flare open again")]
    Expired,
    /// The server could not make a session secret, or its state is
    /// unusable after a panic. The code still works within its 60 seconds.
    #[error("the server cannot sign in now")]
    Unavailable,
}

/// What a successful sign-in returns: the `Set-Cookie` value, the target
/// bound to the code, and the opener to remove.
#[non_exhaustive]
pub struct SignedIn {
    session: Credential,
    set_cookie: SecretString,
    target: ReturnPath,
    opener: Option<PathBuf>,
}

impl SignedIn {
    /// The session credential the cookie carries.
    #[must_use]
    pub fn session(&self) -> &Credential {
        &self.session
    }

    /// The `Set-Cookie` value:
    /// `flare_session=<64 hex>; Path=/; HttpOnly; SameSite=Strict`. It is
    /// host-only and has no expiry; it leaves out `Secure` because the
    /// server speaks http.
    #[must_use]
    pub fn set_cookie(&self) -> &SecretString {
        &self.set_cookie
    }

    /// The path given to `issue_code`.
    #[must_use]
    pub fn target(&self) -> &ReturnPath {
        &self.target
    }

    /// The opener kept for the code with `keep_opener`, for the server to
    /// remove.
    #[must_use]
    pub fn opener(&self) -> Option<&Path> {
        self.opener.as_deref()
    }
}

impl fmt::Debug for SignedIn {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SignedIn")
            .field("target", &self.target)
            .field("opener", &self.opener)
            .finish_non_exhaustive()
    }
}

#[derive(Debug)]
struct Code {
    digest: [u8; 32],
    made: Instant,
    target: ReturnPath,
    opener: Option<PathBuf>,
    used: bool,
}

impl Code {
    fn expired(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.made) > CODE_LIFE
    }

    fn forgotten(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.made) > REMEMBER
    }
}

/// The codes and sessions of one server.
#[derive(Debug, Default)]
pub(crate) struct Table {
    codes: Vec<Code>,
    sessions: VecDeque<[u8; 32]>,
    /// The openers of codes that `issue` forgot, kept for the next
    /// `expire`.
    forgotten_openers: Vec<PathBuf>,
}

impl Table {
    /// Stores the digest of `secret` as a new code for `target`. It first
    /// forgets codes made more than 10 minutes ago. The codes it remembers
    /// grow only as fast as the token's holder asks for codes; nothing
    /// else caps them.
    pub(crate) fn issue(
        &mut self,
        secret: SecretString,
        target: ReturnPath,
        now: Instant,
    ) -> Result<LaunchCode, CodeError> {
        self.forget(now);
        let outstanding = self
            .codes
            .iter()
            .filter(|code| !code.used && !code.expired(now))
            .count();
        if outstanding >= MAX_OUTSTANDING {
            return Err(CodeError::TooMany);
        }
        let digest = digest_of(&secret);
        self.codes.push(Code {
            digest,
            made: now,
            target,
            opener: None,
            used: false,
        });
        Ok(LaunchCode { secret, digest })
    }

    /// Ties an opener file to a code, so `sign_in` or `expire` hands it back
    /// for removal. Returns `false` when the table no longer holds the code.
    pub(crate) fn keep_opener(&mut self, code: &LaunchCode, opener: PathBuf) -> bool {
        let entry = self
            .codes
            .iter_mut()
            .find(|entry| entry.digest == code.digest);
        entry.map(|entry| entry.opener = Some(opener)).is_some()
    }

    /// Forgets codes made more than 10 minutes before `now`, and keeps
    /// their openers for the next `expire`.
    fn forget(&mut self, now: Instant) {
        let (forgotten, kept): (Vec<_>, Vec<_>) = std::mem::take(&mut self.codes)
            .into_iter()
            .partition(|code| code.forgotten(now));
        self.codes = kept;
        self.forgotten_openers
            .extend(forgotten.into_iter().filter_map(|code| code.opener));
    }

    /// Uses `code` and stores `session` as a live session, in one step.
    pub(crate) fn sign_in(
        &mut self,
        code: &str,
        session: SecretString,
        now: Instant,
    ) -> Result<SignedIn, SignInError> {
        if !is_hex(code, 64) {
            return Err(SignInError::Unknown);
        }
        let digest = sha256(code.as_bytes());
        let entry = self
            .codes
            .iter_mut()
            .find(|entry| same(&entry.digest, &digest))
            .filter(|entry| !entry.forgotten(now))
            .ok_or(SignInError::Unknown)?;
        if entry.used {
            return Err(SignInError::Used);
        }
        if entry.expired(now) {
            return Err(SignInError::Expired);
        }
        entry.used = true;
        let opener = entry.opener.take();
        let target = entry.target.clone();
        if self.sessions.len() >= MAX_SESSIONS {
            self.sessions.pop_front();
        }
        self.sessions.push_back(digest_of(&session));
        let set_cookie = compose(&session, |secret| {
            format!("{SESSION_COOKIE}={secret}; Path=/; HttpOnly; SameSite=Strict")
        });
        Ok(SignedIn {
            session: Credential::Session(session),
            set_cookie,
            target,
            opener,
        })
    }

    /// Hands back, once each, the openers still held by codes made more
    /// than 60 seconds before `now`, used or not, and the openers of codes
    /// that `issue` forgot. Then it forgets codes made more than 10
    /// minutes before `now`.
    pub(crate) fn expire(&mut self, now: Instant) -> Vec<PathBuf> {
        let mut openers: Vec<PathBuf> = std::mem::take(&mut self.forgotten_openers);
        openers.extend(
            self.codes
                .iter_mut()
                .filter(|code| code.expired(now))
                .filter_map(|code| code.opener.take()),
        );
        self.codes.retain(|code| !code.forgotten(now));
        openers
    }

    /// Whether `secret` is a live session. Every live session is compared,
    /// so the time taken does not depend on which one matches.
    pub(crate) fn is_session(&self, secret: &SecretString) -> bool {
        let digest = digest_of(secret);
        self.sessions
            .iter()
            .fold(false, |found, live| same(live, &digest) | found)
    }
}
