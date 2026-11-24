//! The recoveries of the failures of `flare serve` and `flare open`. Each
//! names the board that the failed command was given, so the next command
//! serves or opens the same board.

use super::Failed;
use crumbkit::{Call, Recovery, Then};
use std::path::Path;

/// Which port another program holds when `flare serve` cannot listen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeldPort {
    /// The launch port that `--port` named. `flare serve` without `--port`
    /// picks a free one.
    Launch,
    /// The sign-in port, which the provider's redirect URI names, so flare
    /// cannot pick another.
    SignIn,
    /// The port of the address that `hosted.listen` in the config names.
    Hosted,
}

/// `call`, with `--board` when the failed command was given one.
fn with_board(call: Call, board: Option<&str>) -> Call {
    match board {
        Some(board) => call.value("board", board.to_owned()),
        None => call,
    }
}

/// The recovery of `server::not_running`: serve the board.
pub(super) fn serve(board: Option<&str>) -> Recovery {
    Recovery::Run(with_board(Call::new(&["serve"]), board))
}

/// The recovery of `server::already_running`: open the board.
pub(super) fn open(board: Option<&str>) -> Recovery {
    Recovery::Run(with_board(Call::new(&["open"]), board))
}

/// The recovery of `server::port_in_use`. A held launch port is left out
/// of the next `flare serve`; a port that flare cannot change must be
/// freed first.
pub(super) fn port_in_use(port: u16, held: HeldPort, board: Option<&str>) -> Recovery {
    let freed =
        |what: String| Recovery::after(what, Then::Run(with_board(Call::new(&["serve"]), board)));
    match held {
        HeldPort::Launch => serve(board),
        HeldPort::SignIn => freed(format!(
            "stop the program that holds port {port}, the sign-in port that the provider's redirect URI names"
        )),
        HeldPort::Hosted => freed(format!(
            "stop the program that holds port {port}, which hosted.listen in the config names"
        )),
    }
}

impl Failed {
    /// The same failure naming `board`, the `--board` that the failed
    /// command was given, when its recovery names a command.
    pub(crate) fn for_board(mut self, board: Option<&Path>) -> Self {
        let Some(board) = board else {
            return self;
        };
        if let Self::NotRunning { board: slot, .. }
        | Self::AlreadyRunning { board: slot }
        | Self::PortInUse { board: slot, .. } = &mut self
        {
            *slot = Some(board.display().to_string());
        }
        self
    }
}

#[cfg(test)]
mod tests;
