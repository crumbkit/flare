//! `flare`'s command-line grammar and tool record.

pub(crate) mod comments;
mod sharing;

pub(crate) use comments::{
    CommentArgs, CommentDeleteArgs, CommentEditArgs, CommentsArgs, ReplyArgs,
};
pub(crate) use sharing::{ActivityArgs, GrantArgs, LoginArgs, LogoutArgs, ShareArgs, SharesArgs};

use crate::documents::{
    ACTIVITY, BIND, COMMENT, COMMENT_DELETE, COMMENT_EDIT, COMMENTS, GRANT, LOGIN, LOGOUT, OPEN,
    PUBLISH, REPLY, SERVE, SHARE, SHARES,
};
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
    /// Share a Markdown file with people, or as a response to flares
    Share(ShareArgs),
    /// Grant an existing share to more people
    Grant(GrantArgs),
    /// List your shares or the shares granted to you
    ///
    /// The server lists at most 500 shares in one answer and has no cursor
    /// for the rest, so --all lists at most 500 shares and says more follow
    /// when there are more.
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
    /// Explore flare's commands
    Commands {
        /// What to do with the commands.
        #[command(subcommand)]
        command: CommandsVerb,
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

/// Why flare's commands are verbs at the root.
const ROOT: &str = "each command names what a person does with the board flare serves or the server flare shares through";

/// flare's help and failures.
pub const TOOL: Tool = Tool::new()
    .help(
        Help::new()
            .root_verbs(
                &[
                    "serve", "open", "publish", "bind", "login", "logout", "share", "grant",
                    "shares", "activity", "comments", "comment", "reply", "comment-edit", "comment-delete",
                ],
                ROOT,
            )
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
    Leaf::new(&["login"])
        .examples(&[Example::new(
            &["https://flare.example.org"],
            "Sign in to a flare server in the browser and keep its token in the Keychain",
        )])
        .effects(Effects::additive())
        .document(LOGIN),
    Leaf::new(&["logout"])
        .examples(&[Example::new(
            &[],
            "Revoke this computer's token and delete it from the Keychain",
        )])
        .effects(Effects::destructive())
        .document(LOGOUT),
    Leaf::new(&["share"])
        .examples(&[
            Example::new(
                &["review.md", "--with", "ana@example.org,jo@example.org"],
                "Share review.md with two people",
            ),
            Example::new(
                &["answer.md", "--re", "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6"],
                "Share answer.md as a response to a flare",
            ),
        ])
        .effects(Effects::additive())
        .document(SHARE),
    Leaf::new(&["grant"])
        .examples(&[Example::new(
            &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--with", "mei@example.org"],
            "Grant a share to one more person",
        )])
        .effects(Effects::additive())
        .document(GRANT),
    Leaf::new(&["shares"])
        .examples(&[
            Example::new(&[], "List your shares, newest first"),
            Example::new(
                &["--with-me", "--limit", "20"],
                "List the 20 newest shares granted to you",
            ),
        ])
        .effects(Effects::read_only())
        .document(SHARES),
    Leaf::new(&["activity"])
        .examples(&[
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6"],
                "List the responses to a flare, oldest first",
            ),
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--since", "fc1.AAAAAAAAAC8"],
                "List the activity after the cursor an earlier call printed",
            ),
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--since-last", "inbox"],
                "List what is new since the last call that used the name inbox, then save where this one ends",
            ),
        ])
        .effects(Effects::additive().all_or_none())
        .document(ACTIVITY),
    Leaf::new(&["comments"])
        .examples(&[Example::new(
            &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6"],
            "List a flare's comments, each followed by its replies",
        )])
        .effects(Effects::read_only())
        .document(COMMENTS),
    Leaf::new(&["comment"])
        .examples(&[
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--body", "Is this final?"],
                "Comment on a flare",
            ),
            Example::new(
                &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--body-file", "note.txt", "--key", "note-1"],
                "Comment with the text of a file; running it again writes nothing new",
            ),
        ])
        .effects(Effects::additive())
        .document(COMMENT),
    Leaf::new(&["reply"])
        .examples(&[Example::new(
            &["8d1f0c2a9b7e4c53a6f20e1d4b9c7a85", "--flare", "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6", "--body", "Yes, ship it"],
            "Reply to a comment",
        )])
        .effects(Effects::additive())
        .document(REPLY),
    Leaf::new(&["comment-edit"])
        .examples(&[Example::new(
            &["8d1f0c2a9b7e4c53a6f20e1d4b9c7a85", "--body", "Is this final, or a draft?"],
            "Replace the text of your comment",
        )])
        .effects(Effects::additive())
        .document(COMMENT_EDIT),
    Leaf::new(&["comment-delete"])
        .examples(&[Example::new(
            &["8d1f0c2a9b7e4c53a6f20e1d4b9c7a85", "--yes"],
            "Delete your comment without a question",
        )])
        .effects(Effects::destructive())
        .document(COMMENT_DELETE),
                Leaf::new(&["commands", "list"])
                    .examples(&[
                        Example::new(&[], "List every command"),
                        Example::new(&["serve"], "List flare serve with its arguments"),
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
