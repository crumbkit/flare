//! The typed calls of `flare activity` and `flare shares` that the
//! recoveries and the next steps of those commands name, and the typed
//! call that replaces the API's hint for a refusal.

use crumbkit::{Call, Recovery};
use flare_contract::ApiCode;
use flare_core::ShareId;

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

/// `flare shares`, with `--with-me` when `with_me` holds.
pub(crate) fn shares_call(with_me: bool) -> Call {
    let call = Call::new(&["shares"]);
    if with_me { call.flag("with_me") } else { call }
}

impl super::Refused {
    /// The typed call that replaces the API's hint for `partly_shared` and
    /// the two `bad_cursor` codes, whose hints name `flare grant`, `flare
    /// activity` and `flare shares` in prose for API callers, and the
    /// `flare teams list` call that a missing team recovers with, the
    /// `flare teams add` call that a transfer to a non-member recovers with,
    /// and the `flare teams delete` call an owner who cannot leave runs, or
    /// `None` for every other code. crumbkit
    /// renders the call against flare's commands, so a renamed command or
    /// flag fails the tests in place of printing a stale command.
    pub(super) fn typed_call(&self) -> Option<Recovery> {
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
            ApiCode::SharesBadCursor => Some(Recovery::Run(shares_call(self.with_me))),
            ApiCode::TeamNotFound => Some(Recovery::Run(Call::new(&["teams", "list"]))),
            ApiCode::TeamNotAMember => Some(Recovery::Run(
                Call::new(&["teams", "add"]).arg("name").arg("with"),
            )),
            ApiCode::TeamOwnerCannotLeave => Some(Recovery::Run(
                Call::new(&["teams", "delete"]).arg("name").flag("yes"),
            )),
            _ => None,
        }
    }
}
