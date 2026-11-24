//! The typed calls of `flare activity` and `flare shares` that the
//! recoveries and the next steps of those commands name.

use crumbkit::Call;
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
