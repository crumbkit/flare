//! The failures of `flare login`, `logout`, `share`, `show`, `grant`,
//! `shares` and `activity`. [`SharingFailed`] holds the command's own, and
//! [`Refused`] carries a refusal of the share API with the server's code, so
//! a caller sees `flare::shares::not_a_creator` whether it reads the API or
//! the command. No message holds a token or a login code.

use super::Failed;
use crumbkit::{Call, Code, Failure, Recovery, Then};
use flare_contract::{ApiCode, Recovery as ApiRecovery};
use flare_core::ShareId;
use std::fmt;

/// No login is saved, or the Keychain holds no token for it.
pub const LOGIN_NEEDED: Code<'static> = Code::from_spelling("flare::login::needed");
/// The browser sign-in did not hand back a login code.
pub const LOGIN_FAILED: Code<'static> = Code::from_spelling("flare::login::failed");
/// The server is not an `https` origin or a loopback `http` origin.
pub const LOGIN_INVALID_SERVER: Code<'static> = Code::from_spelling("flare::login::invalid_server");
/// The Keychain could not save, read or delete the token.
pub const KEYCHAIN_UNAVAILABLE: Code<'static> = Code::from_spelling("flare::keychain::unavailable");
/// The server did not answer.
pub const NETWORK_UNREACHABLE: Code<'static> = Code::from_spelling("flare::network::unreachable");
/// The server answered with something that is not the share API's answer.
pub const NETWORK_BAD_ANSWER: Code<'static> = Code::from_spelling("flare::network::bad_answer");
/// The file to share could not be read.
pub const FILES_UNREADABLE: Code<'static> = Code::from_spelling("flare::files::unreadable");
/// The rendered HTML of a dry run could not be written.
pub const FILES_UNWRITABLE: Code<'static> = Code::from_spelling("flare::files::unwritable");
/// The file to share is neither Markdown nor HTML.
pub const FILES_UNKNOWN_FORMAT: Code<'static> = Code::from_spelling("flare::files::unknown_format");

/// Why a sharing command failed before or after the server answered.
#[derive(Debug, thiserror::Error, Failure)]
pub enum SharingFailed {
    /// No login is saved, or its token is gone from the Keychain.
    #[error("no flare login is saved on this computer{}", for_server(server.as_deref()))]
    #[failure(code = LOGIN_NEEDED, recovery = sign_in(server.as_deref()))]
    LoginNeeded {
        /// The server of the saved login, when there is one.
        server: Option<String>,
    },
    /// The browser sign-in did not hand back a login code.
    #[error("the browser sign-in did not finish: {problem}")]
    #[failure(code = LOGIN_FAILED, recovery = sign_in(Some(server)))]
    LoginFailed {
        /// The server the sign-in was for.
        server: String,
        /// What went wrong.
        problem: String,
    },
    /// The server is not an `https` origin or a loopback `http` origin.
    #[error("{server} is not a flare server's origin: {problem}")]
    #[failure(
        code = LOGIN_INVALID_SERVER,
        hint = "name the server as https://<host>, such as https://flare.example.org",
        field(server = server)
    )]
    InvalidServer {
        /// The server as given.
        server: String,
        /// What is wrong with it.
        problem: String,
    },
    /// The Keychain could not save, read or delete the token.
    #[error("the Keychain could not {action} the flare token: {problem}")]
    #[failure(
        code = KEYCHAIN_UNAVAILABLE,
        recovery = Recovery::Retry,
        alternative = Recovery::hint("flare keeps its token in the macOS Keychain through /usr/bin/security; on another system, FLARE_KEYCHAIN names a program that takes the same commands")
    )]
    KeychainUnavailable {
        /// `save`, `read` or `delete`.
        action: &'static str,
        /// What went wrong.
        problem: String,
    },
    /// The server did not answer.
    #[error("{server} did not answer: {problem}")]
    #[failure(code = NETWORK_UNREACHABLE, recovery = Recovery::Retry, field(server = server))]
    Unreachable {
        /// The server.
        server: String,
        /// What went wrong.
        problem: String,
    },
    /// The server's answer is not the share API's answer.
    #[error("{server} answered with something flare cannot read: {problem}")]
    #[failure(code = NETWORK_BAD_ANSWER, recovery = bad_answer(*next), field(server = server))]
    BadAnswer {
        /// The server.
        server: String,
        /// What is wrong with the answer.
        problem: String,
        /// What the answer means for running the command again.
        next: NextRun,
    },
    /// The file to share could not be read.
    #[error("{path} could not be read: {problem}")]
    #[failure(code = FILES_UNREADABLE, hint = "pass a readable file", field(path = path))]
    FileUnreadable {
        /// The file.
        path: String,
        /// What went wrong.
        problem: String,
    },
    /// The rendered HTML of a dry run could not be written.
    #[error("{path} could not be written: {problem}")]
    #[failure(
        code = FILES_UNWRITABLE,
        hint = "pass --out with a file in a directory you can write",
        field(path = path)
    )]
    FileUnwritable {
        /// The file.
        path: String,
        /// What went wrong.
        problem: String,
    },
    /// The file to share is neither Markdown nor HTML.
    #[error("{path} does not end in .md or .html")]
    #[failure(
        code = FILES_UNKNOWN_FORMAT,
        hint = "share a Markdown file ending in .md",
        field(path = path)
    )]
    UnknownFormat {
        /// The file.
        path: String,
    },
}

/// What an answer that flare cannot read means for running the command
/// again.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NextRun {
    /// The server answered that the request did not succeed, so the next
    /// run starts clean.
    Clean,
    /// The server answered that the request succeeded. A command whose next
    /// run ends in the same state, such as a read, `flare grant`, `flare
    /// login` or `flare logout`, keeps this; a command that writes a share
    /// or a version turns it into [`NextRun::CheckShares`] with
    /// `Failed::after_a_write`.
    AfterSuccess,
    /// The server may have made the share or the version, and the share API
    /// takes no idempotency key, so a second run could make a second one.
    CheckShares,
}

/// The recovery of [`SharingFailed::BadAnswer`]: check the server and run
/// the command again, or, after a write that may have happened, check
/// `flare shares` first.
fn bad_answer(next: NextRun) -> Recovery {
    match next {
        NextRun::Clean | NextRun::AfterSuccess => Recovery::after(
            "check that the server runs flare with the share API",
            Then::Retry,
        ),
        NextRun::CheckShares => Recovery::after(
            "check that flare shares does not already list the share or the version this command meant to make",
            Then::Retry,
        ),
    }
}

impl Failed {
    /// The same failure for a command that writes a share or a version. A
    /// success answer that flare could not read may follow a write the
    /// server made, so its recovery checks `flare shares` before the
    /// command runs again.
    pub(crate) fn after_a_write(mut self) -> Self {
        if let Self::Sharing(SharingFailed::BadAnswer { next, .. }) = &mut self
            && *next == NextRun::AfterSuccess
        {
            *next = NextRun::CheckShares;
        }
        self
    }

    /// The same failure naming `flares`, the flares a `flare activity`
    /// asked about, when it is a refusal of the share API.
    pub(crate) fn for_flares(self, flares: &[ShareId]) -> Self {
        match self {
            Self::Refused(refused) => Self::Refused(refused.for_flares(flares)),
            other => other,
        }
    }
}

/// `flare activity` with each of `flares` in the order given, or with a
/// placeholder when there are none.
pub(crate) fn activity_call(flares: &[ShareId]) -> Call {
    let call = Call::new(&["activity"]);
    if flares.is_empty() {
        return call.arg("flares");
    }
    flares
        .iter()
        .fold(call, |call, flare| call.value("flares", flare.as_str()))
}

fn for_server(server: Option<&str>) -> String {
    server.map_or_else(String::new, |server| format!(" for {server}"))
}

/// A refusal of the share API, with the server's code and its detail.
#[derive(Debug)]
pub struct Refused {
    code: ApiCode,
    detail: Option<String>,
    server: String,
    /// The share that a partly finished share created, when the server
    /// named one that parses as a share id.
    share: Option<ShareId>,
    /// The flares of the `flare activity` that was refused, which a
    /// `bad_cursor` recovery names.
    flares: Vec<ShareId>,
}

impl Refused {
    /// The refusal `code` from `server`, with the server's `detail`.
    pub(crate) fn new(code: ApiCode, detail: Option<String>, server: &str) -> Self {
        Self {
            code,
            detail,
            server: server.to_owned(),
            share: None,
            flares: Vec::new(),
        }
    }

    /// The same refusal of a `flare activity` that asked about `flares`.
    pub(crate) fn for_flares(mut self, flares: &[ShareId]) -> Self {
        self.flares = flares.to_vec();
        self
    }

    /// The same refusal naming the share that the problem document's
    /// `share` field names. A value that is not a share id is dropped, and
    /// the recovery then names the share with a placeholder.
    pub(crate) fn naming(mut self, share: Option<&str>) -> Self {
        self.share = share.and_then(|text| ShareId::try_from(text).ok());
        self
    }

    /// The typed call that replaces the API's hint for `partly_shared` and
    /// `bad_cursor`, whose hints name `flare grant` and `flare activity` in
    /// prose for API callers, or `None` for every other code. crumbkit
    /// renders the call against flare's commands, so a renamed command or
    /// flag fails the tests in place of printing a stale command.
    fn typed_call(&self) -> Option<Recovery> {
        match self.code {
            ApiCode::PartlyShared => {
                let grant = Call::new(&["grant"]);
                let grant = match &self.share {
                    Some(share) => grant.value("id", share.as_str()),
                    None => grant.arg("id"),
                };
                Some(Recovery::Run(grant.arg("with").arg("re")))
            }
            ApiCode::BadCursor => Some(Recovery::Run(activity_call(&self.flares))),
            _ => None,
        }
    }
}

impl Refused {
    /// The server's code.
    pub(crate) fn api_code(&self) -> ApiCode {
        self.code
    }
}

impl fmt::Display for Refused {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code.title())?;
        match &self.detail {
            Some(detail) => write!(formatter, ": {detail}"),
            None => Ok(()),
        }
    }
}

impl std::error::Error for Refused {}

impl Failure for Refused {
    fn code(&self) -> Code<'_> {
        Code::from_spelling(self.code.text())
    }

    /// The recovery for the code. `partly_shared` and `bad_cursor` take the
    /// typed call that `Refused::typed_call` builds. Every other code
    /// takes the API's recovery, where a `run` recovery, which the API uses
    /// only for `flare login`, becomes the `terminal` sign-in that
    /// `sign_in` builds, because only a person can finish it.
    fn recovery(&self) -> Recovery {
        if let Some(typed) = self.typed_call() {
            return typed;
        }
        match self.code.recovery(&self.server) {
            ApiRecovery::Run { .. } => sign_in(Some(&self.server)),
            ApiRecovery::Retry => Recovery::Retry,
            ApiRecovery::Hint { text } => Recovery::hint(text),
        }
    }

    /// A bad cursor can also be replaced by the cursor that the last `flare
    /// activity` printed.
    fn alternatives(&self) -> Vec<Recovery> {
        match self.code {
            ApiCode::BadCursor => vec![Recovery::after(
                "find the cursor that the last flare activity printed",
                Then::Run(activity_call(&self.flares).arg("since")),
            )],
            _ => Vec::new(),
        }
    }
}

/// The recovery of a failure that needs a new sign-in. Only a person can
/// finish the browser sign-in, so with a known server it is a `terminal`
/// recovery for `flare login <server>`. crumbkit renders a terminal call
/// only when every argument has a value, and with no saved login flare does
/// not know the server, so in that case the recovery is a hint that names
/// the command and the server's address the person supplies.
fn sign_in(server: Option<&str>) -> Recovery {
    match server {
        Some(server) => {
            Recovery::Terminal(Call::new(&["login"]).value("server", server.to_owned()))
        }
        None => Recovery::hint(
            "a person runs flare login with their flare server's address, such as https://flare.example.org, in a terminal",
        ),
    }
}

/// The number of the share API's codes.
const API_COUNT: usize = ApiCode::ALL.len();

/// Every code of the share API, which [`Refused`] carries.
#[expect(
    clippy::indexing_slicing,
    reason = "the loop's bound keeps its index inside both arrays, and a const block cannot call get"
)]
pub(crate) const API_CODES: [Code<'static>; API_COUNT] = {
    let mut codes = [LOGIN_NEEDED; API_COUNT];
    let mut index = 0;
    while index < API_COUNT {
        codes[index] = Code::from_spelling(ApiCode::ALL[index].text());
        index += 1;
    }
    codes
};

#[cfg(test)]
mod tests;
