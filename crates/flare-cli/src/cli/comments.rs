//! The arguments of `flare comments`, `comment`, `reply`, `comment-edit`
//! and `comment-delete`.

use clap::Args;
use crumbkit::Yes;
use std::path::PathBuf;

/// Where a comment's text comes from: exactly one of `--body` and
/// `--body-file`.
#[derive(Debug, Args)]
#[group(required = true, multiple = false)]
pub(crate) struct Text {
    /// The text, 1 to 8,000 characters
    #[arg(long, value_name = "TEXT")]
    pub(crate) body: Option<String>,
    /// A file that holds the text, which is how to send more than one line
    #[arg(long, value_name = "FILE")]
    pub(crate) body_file: Option<PathBuf>,
}

/// The arguments of `flare comments`.
#[derive(Debug, Args)]
pub(crate) struct CommentsArgs {
    /// The flare whose comments to list
    #[arg(value_name = "FLARE")]
    pub(crate) flare: String,
}

/// The arguments of `flare comment`.
#[derive(Debug, Args)]
pub(crate) struct CommentArgs {
    /// The flare to comment on
    #[arg(value_name = "FLARE")]
    pub(crate) flare: String,
    /// The text of the comment.
    #[command(flatten)]
    pub(crate) text: Text,
    /// The key that makes a repeat of this call write nothing; without it, a new random key
    #[arg(long, value_name = "KEY")]
    pub(crate) key: Option<String>,
}

/// The arguments of `flare reply`.
#[derive(Debug, Args)]
pub(crate) struct ReplyArgs {
    /// The comment to reply to, which is not itself a reply
    #[arg(value_name = "COMMENT")]
    pub(crate) comment: String,
    /// The flare the comment is on
    #[arg(long, value_name = "FLARE")]
    pub(crate) flare: String,
    /// The text of the reply.
    #[command(flatten)]
    pub(crate) text: Text,
    /// The key that makes a repeat of this call write nothing; without it, a new random key
    #[arg(long, value_name = "KEY")]
    pub(crate) key: Option<String>,
}

/// The arguments of `flare comment-edit`.
#[derive(Debug, Args)]
pub(crate) struct CommentEditArgs {
    /// The comment or reply to edit, which only its author may
    #[arg(value_name = "COMMENT")]
    pub(crate) comment: String,
    /// The new text.
    #[command(flatten)]
    pub(crate) text: Text,
}

/// The arguments of `flare comment-delete`.
#[derive(Debug, Args)]
pub(crate) struct CommentDeleteArgs {
    /// The comment or reply to delete, which only its author may
    #[arg(value_name = "COMMENT")]
    pub(crate) comment: String,
    /// The confirmation that `--yes` gives without a question.
    #[command(flatten)]
    pub(crate) yes: Yes,
}
