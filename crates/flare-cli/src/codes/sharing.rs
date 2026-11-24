//! The failures of `flare login`, `logout`, `share`, `show`, `grant`,
//! `shares` and `activity`. [`SharingFailed`] holds the command's own, and
//! [`Refused`] carries a refusal of the share API with the server's code, so
//! a caller sees `flare::shares::not_a_creator` whether it reads the API or
//! the command. No message holds a token or a login code.

mod calls;
mod next_run;

pub(crate) use calls::{activity_call, shares_call};
pub use next_run::NextRun;

use super::Failed;
use super::sign_in::sign_in;
use crumbkit::{Call, Code, Failure, Field, Recovery, Then};
use flare_contract::{ApiCode, Recovery as ApiRecovery};
use flare_core::ShareId;
use next_run::{bad_answer, check_shares, retry};
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
/// The server answered that it was too busy for the request, or failed or
/// stopped while it ran.
pub const NETWORK_SERVER_ERROR: Code<'static> = Code::from_spelling("flare::network::server_error");
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
    #[failure(
        code = LOGIN_NEEDED,
        recovery = sign_in(server.as_deref()),
        field(server = server.as_deref())
    )]
    LoginNeeded {
        /// The server of the saved login, when there is one.
        server: Option<String>,
    },
    /// The browser sign-in did not hand back a login code.
    #[error("the browser sign-in did not finish: {problem}")]
    #[failure(
        code = LOGIN_FAILED,
        recovery = sign_in(Some(server)),
        field(server = server)
    )]
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
        field(action = *action),
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
    #[failure(
        code = NETWORK_UNREACHABLE,
        recovery = retry(next),
        field(server = server)
    )]
    Unreachable {
        /// The server.
        server: String,
        /// What went wrong.
        problem: String,
        /// What the silence means for running the command again.
        next: NextRun,
    },
    /// The server was too busy for the request, or failed or stopped while
    /// it ran.
    #[error("{server} could not finish the request: {title}")]
    #[failure(
        code = NETWORK_SERVER_ERROR,
        recovery = retry(next),
        field(server = server),
        field(answer = *answer)
    )]
    ServerError {
        /// The server.
        server: String,
        /// The server's code: `server::busy`, `server::deadline` or
        /// `server::fault`.
        answer: &'static str,
        /// The server's title.
        title: String,
        /// What the answer means for running the command again.
        next: NextRun,
    },
    /// The server's answer is not the share API's answer.
    #[error("{server} answered with something flare cannot read: {problem}")]
    #[failure(
        code = NETWORK_BAD_ANSWER,
        recovery = bad_answer(next),
        field(server = server)
    )]
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

impl Failed {
    /// The same failure of a `flare shares` with `--with-me` when `with_me`
    /// holds, when it is a refusal of the share API.
    pub(crate) fn for_shares(self, with_me: bool) -> Self {
        match self {
            Self::Refused(refused) => Self::Refused(refused.for_shares(with_me)),
            other => other,
        }
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
    /// Whether the `flare shares` that was refused listed the shares
    /// granted to the caller, which a `shares::bad_cursor` recovery names.
    with_me: bool,
    /// Whether the refused request was a write of a share or a version
    /// whose outcome the server does not know, which `shares::unavailable`
    /// says after a write, so the recovery checks `flare shares` first.
    maybe_written: bool,
    /// The same command with the key it sent, when the refused request was
    /// a keyed write, such as a comment, that `comments::unavailable` says
    /// may have happened.
    again: Option<Box<Call>>,
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
            with_me: false,
            maybe_written: false,
            again: None,
        }
    }

    /// The same refusal of a `flare activity` that asked about `flares`.
    pub(crate) fn for_flares(mut self, flares: &[ShareId]) -> Self {
        self.flares = flares.to_vec();
        self
    }

    /// The same refusal of a `flare shares` that listed the shares granted
    /// to the caller when `with_me` holds.
    pub(crate) fn for_shares(mut self, with_me: bool) -> Self {
        self.with_me = with_me;
        self
    }

    /// The same refusal naming the share that the problem document's
    /// `share` field names. A value that is not a share id is dropped, and
    /// the recovery then names the share with a placeholder.
    pub(crate) fn naming(mut self, share: Option<&str>) -> Self {
        self.share = share.and_then(|text| ShareId::try_from(text).ok());
        self
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

    /// The recovery for the code. A `shares::unavailable` refusal of a
    /// write checks `flare shares` first, and a `comments::unavailable`
    /// refusal of a keyed write repeats the command with its key. `partly_shared` and `bad_cursor`
    /// take the typed call that `Refused::typed_call` builds. `not_found`
    /// takes the first of the API hint's two repairs, and
    /// `Refused::alternatives` gives the second. Every other
    /// code takes the API's recovery, where a `run` recovery, which the API
    /// uses only for `flare login`, becomes the `terminal` sign-in that
    /// `sign_in` builds, because only a person can finish it.
    fn recovery(&self) -> Recovery {
        if self.maybe_written {
            return check_shares();
        }
        if let Some(again) = &self.again {
            return Recovery::Run(Call::clone(again));
        }
        if let Some(typed) = self.typed_call() {
            return typed;
        }
        if self.code == ApiCode::ShareNotFound {
            return Recovery::hint("check the flare id");
        }
        match self.code.recovery(&self.server) {
            ApiRecovery::Run { .. } => sign_in(Some(&self.server)),
            ApiRecovery::Retry => Recovery::Retry,
            ApiRecovery::Hint { text } => Recovery::hint(text),
        }
    }

    /// The server, which a dry run has none of, and the share a partly
    /// finished share created, when the server named one.
    fn fields(&self) -> Vec<Field<'_>> {
        let server = (!self.server.is_empty()).then(|| Field::new("server", self.server.as_str()));
        let share = self
            .share
            .as_ref()
            .map(|share| Field::new("share", share.as_str()));
        server.into_iter().chain(share).collect()
    }

    /// A bad cursor can also be replaced by the cursor that the last `flare
    /// activity` or `flare shares` printed, a flare that is not found can
    /// be shared with the caller by its owner, and a missing team can be
    /// solved by asking the team's owner to add the caller.
    fn alternatives(&self) -> Vec<Recovery> {
        match self.code {
            ApiCode::BadCursor => vec![Recovery::after(
                "find the cursor that the last flare activity printed",
                Then::Run(activity_call(&self.flares).arg("since")),
            )],
            ApiCode::ShareNotFound => {
                vec![Recovery::hint("ask the flare's owner to share it with you")]
            }
            ApiCode::SharesBadCursor => vec![Recovery::after(
                "find the cursor that the last flare shares printed",
                Then::Run(shares_call(self.with_me).arg("cursor")),
            )],
            ApiCode::TeamNotFound => vec![Recovery::hint("ask the team's owner to add you")],
            _ => Vec::new(),
        }
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
