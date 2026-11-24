use super::Refused;
use crate::cli::Cli;
use clap::CommandFactory as _;
use crumbkit::{Failure as _, Recovery};
use flare_contract::{ApiCode, Recovery as ApiRecovery};

const SERVER: &str = "https://flare.example.org";

/// The names of flare's commands, such as `grant` and `activity`.
fn commands() -> Vec<String> {
    Cli::command()
        .get_subcommands()
        .map(|command| command.get_name().to_owned())
        .collect()
}

/// Whether `text` names a flare command, as `flare grant` or `flare
/// activity`.
fn names_a_command(text: &str, commands: &[String]) -> bool {
    text.split("flare ").skip(1).any(|after| {
        let word: String = after
            .chars()
            .take_while(|letter| letter.is_ascii_alphanumeric() || *letter == '-')
            .collect();
        commands.contains(&word)
    })
}

#[test]
fn no_share_api_hint_that_names_a_flare_command_reaches_the_cli_as_a_hint() {
    let commands = commands();
    let mut checked = 0;
    for code in ApiCode::ALL.iter().copied() {
        let ApiRecovery::Hint { text } = code.recovery(SERVER) else {
            continue;
        };
        if !names_a_command(&text, &commands) {
            continue;
        }
        checked += 1;
        let recovery = Refused::new(code, None, SERVER).recovery();
        assert!(
            matches!(recovery, Recovery::Run(_) | Recovery::After { .. }),
            "{}: the hint `{text}` names a flare command, and the CLI prints {recovery:?}",
            code.text()
        );
    }
    assert!(
        checked >= 2,
        "the hints of partly_shared and bad_cursor name commands"
    );
}

#[test]
fn a_hint_that_names_flare_without_a_command_is_not_a_command() {
    let commands = commands();
    assert!(names_a_command(
        "run flare grant with the share id",
        &commands
    ));
    assert!(!names_a_command(
        "ask the operator of this flare deployment",
        &commands
    ));
    assert!(!names_a_command("check the flare id", &commands));
}
