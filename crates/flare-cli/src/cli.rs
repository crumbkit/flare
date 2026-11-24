//! `flare`'s command-line grammar and tool record.

pub(crate) mod comments;
mod files;
mod schemas;
mod sharing;
mod skills;
mod teams;

pub(crate) use comments::{
    CommentArgs, CommentDeleteArgs, CommentEditArgs, CommentsArgs, ReplyArgs,
};
pub(crate) use files::{FilesVerb, SweepArgs};
pub(crate) use schemas::SchemasCommand;
pub(crate) use skills::{FlareSkills, SKILL};

pub(crate) use sharing::{
    ActivityArgs, GrantArgs, GrantsArgs, LoginArgs, LogoutArgs, RevokeArgs, ShareArgs, SharesArgs,
    ShowArgs,
};
pub(crate) use teams::{ListTeamsArgs, TeamsVerb};

use crate::documents::{BIND, LOGIN, LOGOUT, METRICS, OPEN, PUBLISH, SERVE, SHOW};
use clap::{Args, Parser, Subcommand};
use crumbkit::commands::{COMMANDS_LIST, CommandsList};
use crumbkit::help::{Effects, Example, Help, Leaf};
use crumbkit::tool::{Exit, Failures};
use crumbkit::{DryRun, NoInput, Tool, ViewFlags};
use crumbkit_usage::{SELF_USAGE, SelfUsage};
use std::path::PathBuf;

/// Serve a board's pages and bindings on this machine, open them in a
/// browser, publish pages and save bindings, and share files through a
/// flare server.
#[derive(Debug, Parser)]
#[command(name = "flare", version)]
pub struct Cli {
    /// The command.
    #[command(subcommand)]
    pub(crate) command: Verb,
    /// The flags that choose how the result prints.
    #[command(flatten)]
    pub(crate) view: ViewFlags,
    /// The flag that tells flare that nobody can answer a question.
    #[command(flatten)]
    pub(crate) no_input: NoInput,
}

/// flare's commands.
#[derive(Debug, Subcommand)]
pub(crate) enum Verb {
    /// Serve a board's pages and bindings, locally or behind a TLS proxy, until Ctrl-C
    Serve(ServeArgs),
    /// Open the running server's index, a page or a binding in a browser
    Open(OpenArgs),
    /// Publish a page version from an HTML or Markdown file
    Publish(PublishArgs),
    /// Save a binding that fills a page's slots
    Bind(BindArgs),
    /// Sign in to a flare server and keep its token in the Keychain
    Login(LoginArgs),
    /// Revoke and delete this computer's token
    Logout(LogoutArgs),
    /// Share a Markdown file
    ///
    /// Each image that the Markdown shows by a relative path, such as
    /// img/chart.png, is uploaded with the file: at most 10 images, each a
    /// PNG, JPEG, GIF or WebP file of at most 5 MiB. The server removes each
    /// image's metadata, its GPS position included, and keeps its
    /// orientation and colour profile. An image file that does not exist
    /// fails with flare::images::missing, and one outside the Markdown
    /// file's folder fails with flare::images::outside_folder.
    Share(ShareArgs),
    /// Show one version of a share you can open, or its latest
    Show(ShowArgs),
    /// Grant a share to readers
    Grant(GrantArgs),
    /// End grants on a share
    Revoke(RevokeArgs),
    /// List grants
    Grants(GrantsArgs),
    /// Manage teams
    Teams {
        /// What to do with teams.
        #[command(subcommand)]
        command: TeamsVerb,
    },
    /// List your shares or those granted to you
    ///
    /// The server lists at most 500 shares in one answer. A list cut short
    /// names the next call with --cursor, which resumes the list after the
    /// last share printed, and --all follows the cursor until the list ends.
    Shares(SharesArgs),
    /// List responses and comments since a cursor
    Activity(ActivityArgs),
    /// List a flare's comments and replies
    Comments(CommentsArgs),
    /// Comment on a flare
    Comment(CommentArgs),
    /// Reply to a comment
    Reply(ReplyArgs),
    /// Edit a comment or reply you wrote
    CommentEdit(CommentEditArgs),
    /// Delete a comment or reply you wrote
    CommentDelete(CommentDeleteArgs),
    /// Manage a hosted site's image files
    Files {
        /// What to do with the files.
        #[command(subcommand)]
        command: FilesVerb,
    },
    /// Count the metric lines in a flare server's log, by day
    ///
    /// The counts come from the metric lines that flare serve writes on
    /// standard error when its config sets log.metrics to true. A page
    /// view is one page response, so one person who reloads a page counts
    /// twice. Lines that are not metric lines are skipped and counted.
    Metrics(MetricsArgs),
    /// Explore flare's commands
    Commands {
        /// What to do with the commands.
        #[command(subcommand)]
        command: CommandsVerb,
    },
    /// Explore the JSON documents that flare prints
    Schemas {
        /// What to do with the documents.
        #[command(subcommand)]
        command: SchemasCommand,
    },
    /// Manage flare's agent skill
    Skills {
        /// What to do with the skill.
        #[command(subcommand)]
        command: FlareSkills,
    },
    /// Review flare's own use
    #[command(name = "self")]
    SelfCommand {
        /// What to review.
        #[command(subcommand)]
        command: SelfVerb,
    },
}

/// The commands for flare's own commands.
#[derive(Debug, Subcommand)]
pub(crate) enum CommandsVerb {
    /// List a branch of flare's commands with their arguments and examples
    List(CommandsList),
}

/// The commands that review flare itself.
#[derive(Debug, Subcommand)]
pub(crate) enum SelfVerb {
    /// Report the patterns in flare's usage log where callers waste effort
    Usage(SelfUsage),
}

/// Where the board is, and the program that reads it.
#[derive(Debug, Args)]
pub(crate) struct BoardArgs {
    /// The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
    #[arg(long, value_name = "DIR")]
    pub(crate) board: Option<PathBuf>,
    /// The board program; without it, FLARE_BOARD, then the first board on PATH
    #[arg(long, value_name = "PATH")]
    pub(crate) board_program: Option<PathBuf>,
}

/// The arguments of `flare serve`.
#[derive(Debug, Args)]
pub(crate) struct ServeArgs {
    /// Where the board is and the program that reads it.
    #[command(flatten)]
    pub(crate) board: BoardArgs,
    /// The launch host's port on 127.0.0.1; 0 picks a free port. A hosted server listens on hosted.listen instead
    #[arg(long, value_name = "N", default_value_t = 0)]
    pub(crate) port: u16,
    /// The config file, whose hosted section serves the pages behind a TLS proxy; without it, $XDG_CONFIG_HOME/flare/config.json or ~/.config/flare/config.json
    #[arg(long, value_name = "PATH")]
    pub(crate) config: Option<PathBuf>,
    /// Stop after this long without a request, such as 30m or 8h. A hosted server never stops by itself
    #[arg(long, value_name = "DURATION", default_value = "8h", value_parser = crate::duration::parse)]
    pub(crate) idle_exit: std::time::Duration,
}

/// The arguments of `flare open`.
#[derive(Debug, Args)]
pub(crate) struct OpenArgs {
    /// The page or binding to open; without it, the index
    #[arg(value_name = "NAME")]
    pub(crate) name: Option<String>,
    /// The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
    #[arg(long, value_name = "DIR")]
    pub(crate) board: Option<PathBuf>,
}

/// The arguments of `flare publish`.
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

/// The arguments of `flare bind`.
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

/// The arguments of `flare metrics`.
#[derive(Debug, Args)]
pub(crate) struct MetricsArgs {
    /// The log files to read, oldest first; without them, standard input
    #[arg(value_name = "FILE")]
    pub(crate) files: Vec<PathBuf>,
}

/// Why flare's commands are verbs at the root.
const ROOT: &str = "each command names what a person does with the board flare serves, the server flare shares through or the log a flare server writes";

/// flare's help and failures.
pub const TOOL: Tool = Tool::new()
    .help(
        Help::new()
            .root_verbs(
                &[
                    "serve", "open", "publish", "bind", "login", "logout", "share", "show",
                    "grant", "revoke", "grants", "shares", "activity", "comments", "comment",
                    "reply", "comment-edit", "comment-delete", "metrics",
                ],
                ROOT,
            )
            .leaves(&[
                Leaf::new(&["serve"])
                    // A serve run lasts until Ctrl-C or an idle hour, so the
                    // example check, which stops a run after 10 seconds,
                    // cannot run either example even with a board.
                    .examples(&[
                        Example::new(&[], "Serve the board in the current directory").needs(&["board"]),
                        Example::new(
                            &["--board", "../notes", "--idle-exit", "1h"],
                            "Serve another board and stop after an hour without a request",
                        ).needs(&["board"]),
                    ])
                    .effects(Effects::additive())
                    .document(SERVE),
                Leaf::new(&["open"])
                    .examples(&[
                        Example::new(&[], "Open the index of the running server").needs(&["board", "server"]),
                        Example::new(&["kitchen"], "Open the binding or page named kitchen").needs(&["board", "server"]),
                    ])
                    .effects(Effects::additive())
                    .document(OPEN),
                Leaf::new(&["publish"])
                    .examples(&[
                        Example::new(
                            &["card.html", "--name", "task-card", "--slots", "slots.json"],
                            "Publish card.html as the page task-card with the slots in slots.json",
                        ).needs(&["board"]),
                        Example::new(
                            &["card.md", "--name", "task-card", "--dry-run"],
                            "Check card.md and publish nothing",
                        ).needs(&["board"]),
                    ])
                    .effects(Effects::additive())
                    .document(PUBLISH),
                Leaf::new(&["bind"])
                    .examples(&[Example::new(
                        &["task-card", "--name", "kitchen", "task=/tasks/tap"],
                        "Save the binding kitchen, which fills task-card's task slot with /tasks/tap",
                    ).needs(&["board"])])
                    .effects(Effects::additive().all_or_none())
                    .document(BIND),
    Leaf::new(&["login"])
        .examples(&[Example::new(
            &["https://flare.example.org"],
            "Sign in to a flare server in the browser and keep its token in the Keychain",
        ).needs(&["server", "browser"])])
        .effects(Effects::additive())
        .document(LOGIN),
    Leaf::new(&["logout"])
        .examples(&[Example::new(
            &[],
            "Revoke this computer's token and delete it from the Keychain",
        ).needs(&["server"])])
        .effects(Effects::destructive())
        .document(LOGOUT),
    sharing::SHARE_LEAF,
    Leaf::new(&["show"])
        .examples(&[
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6"],
                "Show the latest version of a share",
            ).needs(&["server"]),
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--version", "1"],
                "Show the first version of a share",
            ).needs(&["server"]),
        ])
        .effects(Effects::read_only())
        .document(SHOW),
    sharing::GRANT_LEAF,
    sharing::REVOKE_LEAF,
    sharing::GRANTS_LEAF,
    teams::CREATE_LEAF,
    teams::ADD_LEAF,
    teams::REMOVE_LEAF,
    teams::LIST_LEAF,
    teams::SHOW_LEAF,
    teams::TRANSFER_LEAF,
    teams::DELETE_LEAF,
    sharing::SHARES_LEAF,
    sharing::ACTIVITY_LEAF,
    comments::COMMENTS_LEAF,
    comments::COMMENT_LEAF,
    comments::REPLY_LEAF,
    comments::EDIT_LEAF,
    comments::DELETE_LEAF,
    Leaf::new(&["metrics"])
        .examples(&[
            Example::new(&["flare.log"], "Count the metric lines in flare.log by day"),
            Example::new(
                &["flare.log.1", "flare.log", "--json"],
                "Count the metric lines of a rotated log and its newer file as JSON",
            ),
        ])
        .effects(Effects::read_only())
        .document(METRICS),
    files::SWEEP_LEAF,
                Leaf::new(&["commands", "list"])
                    .examples(&[
                        Example::new(&[], "List every command"),
                        Example::new(&["serve"], "List flare serve with its arguments"),
                    ])
                    .effects(Effects::read_only())
                    .document(COMMANDS_LIST),
                schemas::LIST_LEAF,
                schemas::SHOW_LEAF,
                Leaf::new(&["self", "usage"])
                    // The report names the first day of the 7, so its output changes
                    // each day and no golden file can hold it.
                    .examples(&[
                        Example::new(&[], "Report the findings of the last 7 days").needs(&["date"])
                    ])
                    .effects(Effects::read_only())
                    .document(SELF_USAGE),
                FlareSkills::install_leaf(&["skills", "install"]),
                FlareSkills::check_leaf(&["skills", "check"]),
                FlareSkills::print_leaf(&["skills", "print"]),
            ]),
    )
    .failures(
        Failures::new()
            .exits(&[Exit::new(130, "Ctrl-C stopped the server.").commands(&[&["serve"]])]),
    );
