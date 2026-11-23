//! [`ProblemDocument`]: every refusal of the JSON API, as an RFC 9457
//! problem document with a `flare::<area>::<kind>` code and the recovery
//! that the `flare` command prints, in crumbkit's JSON shape.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Defines [`ApiCode`] with its spelling, status, title and recovery.
macro_rules! codes {
    ($($variant:ident => ($text:literal, $status:literal, $title:literal, $recovery:expr),)*) => {
        /// The codes of the JSON API's refusals.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[non_exhaustive]
        pub enum ApiCode {
            $(#[doc = $title] $variant,)*
        }

        impl ApiCode {
            /// Every code.
            pub const ALL: &'static [Self] = &[$(Self::$variant,)*];

            /// The code's spelling, such as `flare::shares::not_found`.
            #[must_use]
            pub const fn text(self) -> &'static str {
                match self { $(Self::$variant => $text,)* }
            }

            /// The HTTP status.
            #[must_use]
            pub const fn status(self) -> u16 {
                match self { $(Self::$variant => $status,)* }
            }

            /// The one-line title.
            #[must_use]
            pub const fn title(self) -> &'static str {
                match self { $(Self::$variant => $title,)* }
            }

            /// What the caller does next. `server` is the deployment's
            /// origin, which `flare login` names.
            ///
            /// The hints of `PartlyShared` and `BadCursor` stay as wire text
            /// for API callers. The `flare` command prints typed calls of
            /// `flare grant` and `flare activity` in their place, so it
            /// never prints those two hints.
            #[must_use]
            pub fn recovery(self, server: &str) -> Recovery {
                let make: fn(&str) -> Recovery = match self { $(Self::$variant => $recovery,)* };
                make(server)
            }

            /// The code that `text` spells, if any.
            #[must_use]
            pub fn from_text(text: &str) -> Option<Self> {
                Self::ALL.iter().copied().find(|code| code.text() == text)
            }
        }
    };
}

fn hint(text: &str) -> Recovery {
    Recovery::Hint {
        text: text.to_owned(),
    }
}

codes! {
    ShareNotFound => ("flare::shares::not_found", 404, "No flare with this id is shared with you.",
        |_| hint("check the flare id, or ask its owner to share it with you")),
    NotACreator => ("flare::shares::not_a_creator", 403, "Only the deployment's creators may share.",
        |_| hint("ask the operator of this flare deployment to add your address to its creators list")),
    KindNotServed => ("flare::shares::kind_not_served", 415, "This flare serves only Markdown shares.",
        |_| hint("share a .md file; uploaded HTML comes in a later version")),
    ShareInvalid => ("flare::shares::invalid", 400, "The share's title or content is not valid.",
        |_| hint("fix the title or the file as the detail says")),
    ShareTooLarge => ("flare::shares::too_large", 413, "A Markdown share holds at most 256 KiB.",
        |_| hint("split the file into shares of at most 256 KiB")),
    ShareForbidden => ("flare::shares::forbidden", 403, "Your role on this flare does not let you make this change.",
        |_| hint("ask the flare's owner to make the change")),
    TooManyShares => ("flare::shares::too_many", 409, "You own as many shares as one person may.",
        |_| hint("ask the operator to raise the limit")),
    TooManyVersions => ("flare::versions::too_many", 409, "A share holds at most 100 versions.",
        |_| hint("share the file again as a new share")),
    TooManyFlares => ("flare::shares::too_many_flares", 400, "A request names at most 16 flares.",
        |_| hint("name at most 16 flares with --re or in one activity call")),
    PartlyShared => ("flare::shares::partly_shared", 409, "The share was created, and some of its grants or responses were not written.",
        |_| hint("run flare grant with the share id, --with and --re to finish")),
    NoVerifiedEmail => ("flare::shares::no_email", 403, "Your sign-in has no verified email address.",
        |server| Recovery::Run { call: format!("flare login {server}") }),
    SharesUnavailable => ("flare::shares::unavailable", 503, "The share stores did not answer.",
        |_| Recovery::Retry),
    EveryoneUnbounded => ("flare::grants::everyone_unbounded", 409, "This deployment admits anyone to sign in, so a flare cannot be opened to everyone.",
        |_| hint("name people or a team with --with")),
    TeamNotFound => ("flare::teams::not_found", 404, "No team with this name has you as a member.",
        |_| hint("ask the team's owner to add you")),
    TeamNameInvalid => ("flare::teams::invalid_name", 400, "A team's name is 1 to 40 lowercase letters, digits and dashes.",
        |_| hint("name the team with lowercase letters, digits and dashes, such as design-review")),
    TeamNameTaken => ("flare::teams::name_taken", 409, "A team with this name exists or existed, and a team name is never reused.",
        |_| hint("pick another name")),
    TeamNotACreator => ("flare::teams::not_a_creator", 403, "Only the deployment's creators may create a team.",
        |_| hint("ask the operator of this flare deployment to add your address to its creators list")),
    TeamForbidden => ("flare::teams::forbidden", 403, "Only the team's owner may make this change.",
        |_| hint("ask the team's owner to make the change")),
    TeamOwnerCannotLeave => ("flare::teams::owner_cannot_leave", 409, "The team's owner cannot leave the team.",
        |_| hint("delete the team instead")),
    TooManyMembers => ("flare::teams::too_many_members", 409, "A team has at most 256 members.",
        |_| hint("remove a member you no longer need")),
    TooManyTeamsOwned => ("flare::teams::too_many_owned", 409, "One person owns at most 20 teams.",
        |_| hint("delete a team you no longer need")),
    TooManyTeamsJoined => ("flare::teams::too_many_joined", 409, "One person belongs to at most 32 teams.",
        |_| hint("the person leaves a team, or its owner removes them")),
    TeamsManagedElsewhere => ("flare::teams::managed_elsewhere", 403, "This deployment manages its teams in its own directory, so flare cannot change them.",
        |_| hint("change the team in the deployment's directory")),
    TeamsUnavailable => ("flare::teams::unavailable", 503, "The team store did not answer.",
        |_| Recovery::Retry),
    TooManyGrants => ("flare::grants::too_many", 409, "A flare is shared with at most 256 people.",
        |_| hint("revoke a grant you no longer need")),
    GrantChanged => ("flare::grants::changed", 409, "A grant changed while yours was written.",
        |_| Recovery::Retry),
    NotAnAddress => ("flare::recipients::not_an_address", 400, "Each --with names an email address on this deployment.",
        |_| hint("name each recipient by address, such as --with ana@example.org")),
    RecipientsUnavailable => ("flare::recipients::unavailable", 503, "The recipients directory did not answer.",
        |_| Recovery::Retry),
    SignInNeeded => ("flare::tokens::sign_in_needed", 401, "Your token is missing, unknown, expired or revoked, or this server no longer admits its owner.",
        |server| Recovery::Run { call: format!("flare login {server}") }),
    CodeRefused => ("flare::tokens::code_refused", 403, "The login code is unknown, used or expired.",
        |server| Recovery::Run { call: format!("flare login {server}") }),
    TokenNotFound => ("flare::tokens::not_found", 404, "You hold no token with this id.",
        |_| hint("name one of your own tokens")),
    TokensUnavailable => ("flare::tokens::unavailable", 503, "The token store did not answer.",
        |_| Recovery::Retry),
    BadCursor => ("flare::activity::bad_cursor", 400, "The cursor is not one this flare wrote.",
        |_| hint("pass the cursor that the last flare activity printed, or none to start over")),
    ActivityUnavailable => ("flare::activity::unavailable", 503, "The activity store did not answer.",
        |_| Recovery::Retry),
    BadBody => ("flare::requests::bad_body", 400, "The request's body is not the JSON this route takes.",
        |_| hint("send the body that schema/flare-api.schema.json describes")),
    BadQuery => ("flare::requests::bad_query", 400, "The request's query is missing a value or holds a malformed one.",
        |_| hint("send the query that docs/hosting.md describes")),
}

/// What the caller does next, in the shape of crumbkit's JSON failure
/// document: an object tagged by `kind`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "lowercase", deny_unknown_fields)]
pub enum Recovery {
    /// Run this command line.
    Run {
        /// The command line, such as `flare login https://flare.example.org`.
        call: String,
    },
    /// Run the same command again later.
    Retry,
    /// Follow this advice.
    Hint {
        /// The advice.
        text: String,
    },
}

/// An RFC 9457 problem document with flare's code and recovery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[non_exhaustive]
pub struct ProblemDocument {
    /// Always `about:blank`: the code names the problem.
    #[serde(rename = "type")]
    pub kind: String,
    /// The one-line title.
    pub title: String,
    /// The HTTP status.
    pub status: u16,
    /// The stable code, `flare::<area>::<kind>`.
    pub code: String,
    /// What the caller does next.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recovery: Option<Recovery>,
    /// What exactly is wrong, when the title does not say. It never holds
    /// an address or a secret.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// The share that a partly finished share created.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share: Option<String>,
}

impl ProblemDocument {
    /// The document for `code` on the deployment at `server`.
    #[must_use]
    pub fn new(code: ApiCode, server: &str) -> Self {
        Self {
            kind: "about:blank".to_owned(),
            title: code.title().to_owned(),
            status: code.status(),
            code: code.text().to_owned(),
            recovery: Some(code.recovery(server)),
            detail: None,
            share: None,
        }
    }

    /// The same document with `detail`.
    #[must_use]
    pub fn with_detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    /// The same document naming `share`.
    #[must_use]
    pub fn with_share(mut self, share: impl Into<String>) -> Self {
        self.share = Some(share.into());
        self
    }
}

#[cfg(test)]
mod tests;
