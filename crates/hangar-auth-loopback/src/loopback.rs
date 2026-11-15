//! [`Loopback`]: one server's launch host, token, codes and sessions, and
//! its [`Authenticator`] implementation.

use crate::codes::{CodeError, LaunchCode, ReturnPath, SignInError, SignedIn, Table};
use crate::host::LaunchHost;
use crate::principal::Served;
use crate::probe::Nonce;
use crate::record::ServerRecord;
use crate::request::{self, Checked, Refusal};
use crate::secret::{compose, proof, random_secret, same_secret};
use hangar_core::{Access, AuthError, Authenticator, Credential, Decision, Principal, Resource};
use secrecy::SecretString;
use std::fmt;
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::Instant;

/// Why a server could not start.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum StartError {
    /// The operating system gave no random bytes for the host name or the
    /// token.
    #[error("the operating system gave no random bytes")]
    Random,
}

/// The sign-in state of one `hangar serve` process. Clones share it.
#[derive(Clone)]
pub struct Loopback(Arc<Inner>);

struct Inner {
    host: LaunchHost,
    token: SecretString,
    principal: Principal,
    served: Served,
    table: Mutex<Table>,
}

impl Loopback {
    /// Makes a new launch host name and token for a server on `port` whose
    /// one principal is `principal`.
    ///
    /// # Errors
    ///
    /// Returns [`StartError::Random`] when the operating system gives no
    /// random bytes.
    pub fn new(port: u16, principal: Principal, served: Served) -> Result<Self, StartError> {
        let host = LaunchHost::random(port).map_err(|_| StartError::Random)?;
        let token = random_secret().map_err(|_| StartError::Random)?;
        Ok(Self(Arc::new(Inner {
            host,
            token,
            principal,
            served,
            table: Mutex::new(Table::default()),
        })))
    }

    /// The launch host and port.
    #[must_use]
    pub fn launch_host(&self) -> &LaunchHost {
        &self.0.host
    }

    /// The record for the server file.
    #[must_use]
    pub fn server_record(&self) -> ServerRecord {
        ServerRecord::new(self.0.host.clone(), self.0.token.clone())
    }

    /// Checks a request before any route runs: the peer is a loopback
    /// address, the `Host` header is the launch host, a request other than
    /// GET or HEAD comes from the launch origin, and the credential is well
    /// formed. The crate docs give the rules.
    ///
    /// # Errors
    ///
    /// Returns the [`Refusal`] of the first check that fails.
    pub fn check(
        &self,
        peer: IpAddr,
        method: &http::Method,
        headers: &http::HeaderMap,
    ) -> Result<Checked, Refusal> {
        request::check(&self.0.host, peer, method, headers)
    }

    fn table(&self) -> Option<MutexGuard<'_, Table>> {
        self.0.table.lock().ok()
    }

    /// Issues a launch code for `target`. It works once, for 60 seconds.
    ///
    /// # Errors
    ///
    /// Returns [`CodeError::TooMany`] while 64 codes are unused and
    /// unexpired, [`CodeError::Random`] without random bytes and
    /// [`CodeError::Unavailable`] after a panic poisoned the table.
    pub fn issue_code(&self, target: ReturnPath, now: Instant) -> Result<LaunchCode, CodeError> {
        let secret = random_secret().map_err(|_| CodeError::Random)?;
        self.table()
            .ok_or(CodeError::Unavailable)?
            .issue(secret, target, now)
    }

    /// Ties the opener written for `code` to it, so [`SignedIn::opener`]
    /// and [`expire`](Self::expire) hand it back for removal.
    pub fn keep_opener(&self, code: &LaunchCode, opener: PathBuf) {
        if let Some(mut table) = self.table() {
            table.keep_opener(code, opener);
        }
    }

    /// The launch URL: `http://<launch host>:<port>/launch?code=<code>`.
    #[must_use]
    pub fn launch_url(&self, code: &LaunchCode) -> SecretString {
        let origin = self.0.host.origin();
        compose(code.secret(), |code| format!("{origin}/launch?code={code}"))
    }

    /// An HTML page that sends the browser to the launch URL, for the
    /// opener file.
    #[must_use]
    pub fn opener_page(&self, code: &LaunchCode) -> SecretString {
        compose(&self.launch_url(code), |url| {
            format!(
                "<!doctype html>\n<meta charset=\"utf-8\">\n<meta name=\"referrer\" content=\"no-referrer\">\n<meta http-equiv=\"refresh\" content=\"0;url={url}\">\n<title>hangar</title>\n<p><a href=\"{url}\">Open hangar</a></p>\n"
            )
        })
    }

    /// Uses a launch code and starts a session. Of concurrent calls with
    /// one code, exactly one succeeds and the others get
    /// [`SignInError::Used`]. At 64 live sessions, the new one replaces
    /// the oldest.
    ///
    /// # Errors
    ///
    /// Returns [`SignInError::Unknown`], [`SignInError::Used`] or
    /// [`SignInError::Expired`] for a code that does not work, and
    /// [`SignInError::Unavailable`] without random bytes or after a panic
    /// poisoned the table.
    pub fn sign_in(&self, code: &str, now: Instant) -> Result<SignedIn, SignInError> {
        let session = random_secret().map_err(|_| SignInError::Unavailable)?;
        self.table()
            .ok_or(SignInError::Unavailable)?
            .sign_in(code, session, now)
    }

    /// Hands back the openers of codes that expired unused, for removal,
    /// and forgets codes made more than 10 minutes before `now`.
    #[must_use = "each opener returned holds a dead code and is the caller's to remove"]
    pub fn expire(&self, now: Instant) -> Vec<PathBuf> {
        self.table()
            .map(|mut table| table.expire(now))
            .unwrap_or_default()
    }

    /// The probe's answer for `nonce`: the hex SHA-256 of the token
    /// followed by the nonce.
    #[must_use]
    pub fn prove(&self, nonce: &Nonce) -> String {
        proof(&self.0.token, nonce.as_str())
    }
}

impl Authenticator for Loopback {
    /// Accepts the token as [`Credential::Bearer`] and a live session
    /// secret as [`Credential::Session`], both compared in constant time,
    /// and rejects every other credential.
    async fn authenticate(&self, credential: &Credential) -> Result<Principal, AuthError> {
        let accepted = match credential {
            Credential::Bearer(token) => same_secret(token, &self.0.token),
            Credential::Session(secret) => self
                .table()
                .ok_or(AuthError::Unavailable)?
                .is_session(secret),
            _ => false,
        };
        if accepted {
            Ok(self.0.principal.clone())
        } else {
            Err(AuthError::Rejected)
        }
    }

    /// Allows the local principal to read and write what the server
    /// serves, and denies everything else.
    async fn authorize(
        &self,
        principal: &Principal,
        _access: Access,
        resource: &Resource,
    ) -> Result<Decision, AuthError> {
        if *principal == self.0.principal && self.0.served.reaches(resource) {
            Ok(Decision::Allowed)
        } else {
            Ok(Decision::Denied)
        }
    }
}

impl fmt::Debug for Loopback {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Loopback")
            .field("host", &self.0.host)
            .field("principal", &self.0.principal)
            .field("served", &self.0.served)
            .finish_non_exhaustive()
    }
}
