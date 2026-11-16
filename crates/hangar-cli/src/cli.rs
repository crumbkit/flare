//! `hangar`'s command-line grammar and tool record.

use crate::documents::{BIND, OPEN, PUBLISH, SERVE};
use clap::{Args, Parser, Subcommand};
use crumbkit::commands::{COMMANDS_LIST, CommandsList};
use crumbkit::help::{Effects, Example, Help, Leaf};
use crumbkit::tool::{Exit, Failures};
use crumbkit::{DryRun, NoInput, Tool, ViewFlags};
use crumbkit_usage::{SELF_USAGE, SelfUsage};
use std::path::PathBuf;

/// Serve a board's pages and bindings on this machine, open them in a
/// browser, publish pages and save bindings.
#[derive(Debug, Parser)]
#[command(name = "hangar", version)]
pub struct Cli {
    /// The command.
    #[command(subcommand)]
    pub(crate) command: Verb,
    /// The flags that choose how the result prints.
    #[command(flatten)]
    pub(crate) view: ViewFlags,
    /// The flag that tells hangar that nobody can answer a question.
    #[command(flatten)]
    pub(crate) no_input: NoInput,
}

/// hangar's commands.
#[derive(Debug, Subcommand)]
pub(crate) enum Verb {
    /// Serve a board's pages and bindings on this machine until Ctrl-C
    Serve(ServeArgs),
    /// Open the running server's index, a page or a binding in a browser
    Open(OpenArgs),
    /// Publish a new version of a page from an HTML or Markdown file
    Publish(PublishArgs),
    /// Save a binding that fills a page's slots with records
    Bind(BindArgs),
    /// Explore hangar's commands
    Commands {
        /// What to do with the commands.
        #[command(subcommand)]
        command: CommandsVerb,
    },
    /// Review hangar's own use
    #[command(name = "self")]
    SelfCommand {
        /// What to review.
        #[command(subcommand)]
        command: SelfVerb,
    },
}

/// The commands for hangar's own commands.
#[derive(Debug, Subcommand)]
pub(crate) enum CommandsVerb {
    /// List a branch of hangar's commands with their arguments and examples
    List(CommandsList),
}

/// The commands that review hangar itself.
#[derive(Debug, Subcommand)]
pub(crate) enum SelfVerb {
    /// Report the patterns in hangar's usage log where callers waste effort
    Usage(SelfUsage),
}

/// Where the board is, and the program that reads it.
#[derive(Debug, Args)]
pub(crate) struct BoardArgs {
    /// The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
    #[arg(long, value_name = "DIR")]
    pub(crate) board: Option<PathBuf>,
    /// The board program; without it, HANGAR_BOARD, then the first board on PATH
    #[arg(long, value_name = "PATH")]
    pub(crate) board_program: Option<PathBuf>,
}

/// The arguments of `hangar serve`.
#[derive(Debug, Args)]
pub(crate) struct ServeArgs {
    /// Where the board is and the program that reads it.
    #[command(flatten)]
    pub(crate) board: BoardArgs,
    /// The launch host's port on 127.0.0.1; 0 picks a free port
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub(crate) port: u16,
    /// The config file; without it, $XDG_CONFIG_HOME/hangar/config.json or ~/.config/hangar/config.json
    #[arg(long, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,
    /// Stop after this long without a request, such as 30m or 8h
    #[arg(long, value_name = "DURATION", default_value = "8h", value_parser = crate::duration::parse)]
    pub(crate) idle_exit: std::time::Duration,
}

/// The arguments of `hangar open`.
#[derive(Debug, Args)]
pub(crate) struct OpenArgs {
    /// The page or binding to open; without it, the index
    #[arg(value_name = "NAME")]
    pub(crate) name: Option<String>,
    /// The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
    #[arg(long, value_name = "DIR")]
    pub(crate) board: Option<PathBuf>,
}

/// The arguments of `hangar publish`.
#[derive(Debug, Args)]
pub(crate) struct PublishArgs {
    /// The page's file: .html for HTML, .md for Markdown
    #[arg(value_name = "FILE")]
    pub(crate) file: PathBuf,
    /// The page's name, one segment of lowercase letters, digits and dashes
    #[arg(long, value_name = "NAME")]
    pub(crate) name: String,
    /// A JSON file that lists the page's slots
    #[arg(long, value_name = "FILE")]
    pub(crate) slots: Option<PathBuf>,
    /// Where the board is and the program that reads it.
    #[command(flatten)]
    pub(crate) board: BoardArgs,
    /// Check the page and print what would be published, and publish nothing.
    #[command(flatten)]
    pub(crate) dry_run: DryRun,
}

/// The arguments of `hangar bind`.
#[derive(Debug, Args)]
pub(crate) struct BindArgs {
    /// The page the binding fills
    #[arg(value_name = "PAGE")]
    pub(crate) page: String,
    /// The binding's name, one segment of lowercase letters, digits and dashes
    #[arg(long, value_name = "NAME")]
    pub(crate) name: String,
    /// Each slot and the record that fills it, as SLOT=PATH, separated by spaces
    #[arg(value_name = "SLOT=PATH", required = true, num_args = 1..)]
    pub(crate) fills: Vec<String>,
    /// Where the board is and the program that reads it.
    #[command(flatten)]
    pub(crate) board: BoardArgs,
    /// Check the fills and print what would be saved, and save nothing.
    #[command(flatten)]
    pub(crate) dry_run: DryRun,
}

/// Why hangar's commands are verbs at the root.
const ROOT: &str =
    "hangar has one resource, the board it serves, so each command names what a person does to it";

/// hangar's help and failures.
pub const TOOL: Tool = Tool::new()
    .help(
        Help::new()
            .root_verbs(&["serve", "open", "publish", "bind"], ROOT)
            .leaves(&[
                Leaf::new(&["serve"])
                    .examples(&[
                        Example::new(&[], "Serve the board in the current directory"),
                        Example::new(
                            &["--board", "../notes", "--idle-exit", "1h"],
                            "Serve another board and stop after an hour without a request",
                        ),
                    ])
                    .effects(Effects::additive())
                    .document(SERVE),
                Leaf::new(&["open"])
                    .examples(&[
                        Example::new(&[], "Open the index of the running server"),
                        Example::new(&["kitchen"], "Open the binding or page named kitchen"),
                    ])
                    .effects(Effects::additive())
                    .document(OPEN),
                Leaf::new(&["publish"])
                    .examples(&[
                        Example::new(
                            &["card.html", "--name", "task-card", "--slots", "slots.json"],
                            "Publish card.html as the page task-card with the slots in slots.json",
                        ),
                        Example::new(
                            &["card.md", "--name", "task-card", "--dry-run"],
                            "Check card.md and publish nothing",
                        ),
                    ])
                    .effects(Effects::additive())
                    .document(PUBLISH),
                Leaf::new(&["bind"])
                    .examples(&[Example::new(
                        &["task-card", "--name", "kitchen", "task=/tasks/tap"],
                        "Save the binding kitchen, which fills task-card's task slot with /tasks/tap",
                    )])
                    .effects(Effects::additive().all_or_none())
                    .document(BIND),
                Leaf::new(&["commands", "list"])
                    .examples(&[
                        Example::new(&[], "List every command"),
                        Example::new(&["serve"], "List hangar serve with its arguments"),
                    ])
                    .effects(Effects::read_only())
                    .document(COMMANDS_LIST),
                Leaf::new(&["self", "usage"])
                    .examples(&[Example::new(&[], "Report the findings of the last 7 days")])
                    .effects(Effects::read_only())
                    .document(SELF_USAGE),
            ]),
    )
    .failures(
        Failures::new()
            .exits(&[Exit::new(130, "Ctrl-C stopped the server.").commands(&[&["serve"]])]),
    );
