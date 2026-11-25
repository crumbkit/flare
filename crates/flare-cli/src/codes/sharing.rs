//! The failures of `flare login`, `logout`, `share`, `show`, `grant`,
//! `shares` and `activity`. [`SharingFailed`] holds the command's own, and
//! [`Refused`] carries a refusal of the share API with the server's code, so
//! a caller sees `flare::shares::not_a_creator` whether it reads the API or
//! the command. No message holds a token or a login code.

mod calls;
mod next_run;
mod refused;

pub(crate) use calls::{activity_call, shares_call};
pub use next_run::NextRun;
pub use refused::Refused;

use super::Failed;
use super::sign_in::sign_in;
use crumbkit::{Code, Failure, Recovery};
use flare_contract::ApiCode;
use flare_core::ShareId;
use next_run::{bad_answer, retry};

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
/// An image the document shows is not in its folder.
pub const IMAGES_MISSING: Code<'static> = Code::from_spelling("flare::images::missing");
/// An image the document shows is outside the document's folder.
pub const IMAGES_OUTSIDE_FOLDER: Code<'static> =
    Code::from_spelling("flare::images::outside_folder");
/// A document with its images makes a body larger than the server reads.
pub const IMAGES_BODY_TOO_LARGE: Code<'static> =
    Code::from_spelling("flare::images::body_too_large");

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
    /// An image the document shows is not where its path points.
    #[error("the document shows the image {image}, and no file is there")]
    #[failure(
        code = IMAGES_MISSING,
        hint = "put the image at that path, relative to the document's folder, or take it out of the document",
        field(image = image)
    )]
    ImageMissing {
        /// The image's path as the Markdown wrote it.
        image: String,
    },
    /// An image the document shows is outside the document's folder.
    #[error("the document shows the image {image}, which is outside the document's folder")]
    #[failure(
        code = IMAGES_OUTSIDE_FOLDER,
        hint = "copy the image into the document's folder and show it by a path inside that folder",
        field(image = image)
    )]
    ImageOutsideFolder {
        /// The image's path as the Markdown wrote it.
        image: String,
    },
    /// One image is larger than the whole body the server reads for a
    /// document with its images.
    #[error(
        "the image {image} is {bytes} bytes, and the server reads at most {limit} for a document with its images"
    )]
    #[failure(
        code = IMAGES_BODY_TOO_LARGE,
        hint = "make the image smaller, such as by saving a screenshot as JPEG, and share again",
        field(image = image)
    )]
    ImageOverUpload {
        /// The image's path as the Markdown wrote it.
        image: String,
        /// The image's length in bytes.
        bytes: usize,
        /// The limit, such as `4 MiB`.
        limit: String,
    },
    /// The document with its images makes a body larger than the server
    /// reads.
    #[error(
        "the document with its images is {bytes} bytes, more than the {limit} the server reads; its largest image is {image}"
    )]
    #[failure(
        code = IMAGES_BODY_TOO_LARGE,
        hint = "make the images smaller, such as by saving screenshots as JPEG, or show fewer in one document",
        field(image = image)
    )]
    UploadTooLarge {
        /// The largest image's path as the Markdown wrote it.
        image: String,
        /// The body's length in bytes.
        bytes: usize,
        /// The limit, such as `4 MiB`.
        limit: String,
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
