//! [`Refused`]: a refusal of the share API, with the server's code, and
//! its recovery.

use super::calls::{activity_call, shares_call};
use super::next_run::check_shares;
use crate::codes::search::Searched;
use crate::codes::sign_in::sign_in;
use crumbkit::{Call, Code, Failure, Field, Recovery, Then};
use flare_contract::{ApiCode, Recovery as ApiRecovery};
use flare_core::ShareId;
use std::fmt;

/// A refusal of the share API, with the server's code and its detail.
#[derive(Debug)]
pub struct Refused {
    pub(super) code: ApiCode,
    pub(super) detail: Option<Box<str>>,
    pub(super) server: Box<str>,
    /// The share that a partly finished share created, when the server
    /// named one that parses as a share id.
    pub(super) share: Option<ShareId>,
    /// The flares of the `flare activity` that was refused, which a
    /// `bad_cursor` recovery names.
    pub(super) flares: Vec<ShareId>,
    /// Whether the `flare shares` that was refused listed the shares
    /// granted to the caller, which a `shares::bad_cursor` recovery names.
    pub(super) with_me: bool,
    /// Whether the refused request was a write of a share or a version
    /// whose outcome the server does not know, which `shares::unavailable`
    /// says after a write, so the recovery checks `flare shares` first.
    pub(super) maybe_written: bool,
    /// The same command with the key it sent, when the refused request was
    /// a keyed write, such as a comment, that `comments::unavailable` says
    /// may have happened.
    pub(super) again: Option<Box<Call>>,
    /// The query and errors of a search that did not run.
    pub(in crate::codes) searched: Box<Searched>,
    /// The token command that gave the refused token, when the config's
    /// `client` section names one, which a `sign_in_needed` recovery names
    /// in place of `flare login`.
    pub(super) helper: Option<Box<str>>,
}

impl Refused {
    /// The refusal `code` from `server`, with the server's `detail`.
    pub(crate) fn new(code: ApiCode, detail: Option<String>, server: &str) -> Self {
        Self {
            code,
            detail: detail.map(String::into_boxed_str),
            server: server.into(),
            share: None,
            flares: Vec::new(),
            with_me: false,
            maybe_written: false,
            again: None,
            searched: Box::default(),
            helper: None,
        }
    }

    /// The same refusal of a request whose token came from the token
    /// command `helper`, when there is one.
    pub(crate) fn with_helper(mut self, helper: Option<&str>) -> Self {
        self.helper = helper.map(Box::from);
        self
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

    fn causes(&self) -> Vec<String> {
        self.searched.causes()
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
        if let (ApiCode::SignInNeeded, Some(helper)) = (self.code, &self.helper) {
            return Recovery::hint(format!(
                "sign in the way the token command {helper} expects, then run the command again; client.token_command in flare's config file names it"
            ));
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
        let server = (!self.server.is_empty()).then(|| Field::new("server", &*self.server));
        let share = self
            .share
            .as_ref()
            .map(|share| Field::new("share", share.as_str()));
        server
            .into_iter()
            .chain(share)
            .chain(self.searched.fields())
            .collect()
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
