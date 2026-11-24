use super::HeldPort;
use crate::codes::Failed;
use crumbkit::{Call, Failure as _, Recovery, Then};

const BOARD: &str = "../notes";

fn serve(board: Option<&str>) -> Call {
    let call = Call::new(&["serve"]);
    match board {
        Some(board) => call.value("board", board),
        None => call,
    }
}

#[test]
fn not_running_names_flare_serve_with_the_board_the_command_was_given() {
    for board in [None, Some(BOARD)] {
        let failed = Failed::NotRunning {
            reason: "there is no server.json for this board".to_owned(),
            board: board.map(ToOwned::to_owned),
        };
        assert_eq!(failed.recovery(), Recovery::Run(serve(board)), "{board:?}");
    }
}

#[test]
fn already_running_names_flare_open_with_the_board_the_command_was_given() {
    for board in [None, Some(BOARD)] {
        let failed = Failed::AlreadyRunning {
            board: board.map(ToOwned::to_owned),
        };
        let open = Call::new(&["open"]);
        let open = match board {
            Some(board) => open.value("board", board),
            None => open,
        };
        assert_eq!(failed.recovery(), Recovery::Run(open), "{board:?}");
    }
}

#[test]
fn a_held_launch_port_names_flare_serve_without_the_port() {
    let failed = Failed::PortInUse {
        port: 4000,
        held: HeldPort::Launch,
        board: Some(BOARD.to_owned()),
    };
    assert_eq!(failed.recovery(), Recovery::Run(serve(Some(BOARD))));
}

#[test]
fn a_held_sign_in_port_asks_for_the_holder_to_stop_before_flare_serve_runs_again() {
    let failed = Failed::PortInUse {
        port: 8740,
        held: HeldPort::SignIn,
        board: Some(BOARD.to_owned()),
    };
    assert_eq!(
        failed.recovery(),
        Recovery::after(
            "stop the program that holds port 8740, the sign-in port that the provider's redirect URI names",
            Then::Run(serve(Some(BOARD))),
        )
    );
}

#[test]
fn a_held_hosted_port_asks_for_the_holder_to_stop_before_flare_serve_runs_again() {
    let failed = Failed::PortInUse {
        port: 443,
        held: HeldPort::Hosted,
        board: None,
    };
    assert_eq!(
        failed.recovery(),
        Recovery::after(
            "stop the program that holds port 443, which hosted.listen in the config names",
            Then::Run(serve(None)),
        )
    );
}
