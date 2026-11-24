//! The arguments of `flare comments`, `comment`, `reply`, `comment-edit`
//! and `comment-delete`.

use crate::documents::{COMMENT, COMMENT_DELETE, COMMENT_EDIT, COMMENTS, REPLY};
use clap::Args;
use crumbkit::Yes;
use crumbkit::help::{Effects, Example, Leaf};
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

/// The help leaf of `flare comments`.
pub(super) const COMMENTS_LEAF: Leaf = Leaf::new(&["comments"])
    .examples(&[Example::new(
        &["3f9c0a17e2b44d6f8a1c5b7e9d20f4a6"],
        "List a flare's comments, each followed by its replies",
    )
    .needs(&["server"])])
    .effects(Effects::read_only())
    .document(COMMENTS);

/// The help leaf of `flare comment`.
pub(super) const COMMENT_LEAF: Leaf = Leaf::new(&["comment"])
    .examples(&[
        Example::new(
            &[
                "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6",
                "--body",
                "Is this final?",
            ],
            "Comment on a flare",
        )
        .needs(&["server"]),
        Example::new(
            &[
                "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6",
                "--body-file",
                "note.txt",
                "--key",
                "note-1",
            ],
            "Comment with the text of a file; running it again writes nothing new",
        )
        .needs(&["server"]),
    ])
    .effects(Effects::additive())
    .document(COMMENT);

/// The help leaf of `flare reply`.
pub(super) const REPLY_LEAF: Leaf = Leaf::new(&["reply"])
    .examples(&[Example::new(
        &[
            "8d1f0c2a9b7e4c53a6f20e1d4b9c7a85",
            "--flare",
            "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6",
            "--body",
            "Yes, ship it",
        ],
        "Reply to a comment",
    )
    .needs(&["server"])])
    .effects(Effects::additive())
    .document(REPLY);

/// The help leaf of `flare comment-edit`.
pub(super) const EDIT_LEAF: Leaf = Leaf::new(&["comment-edit"])
    .examples(&[Example::new(
        &[
            "8d1f0c2a9b7e4c53a6f20e1d4b9c7a85",
            "--body",
            "Is this final, or a draft?",
        ],
        "Replace the text of your comment",
    )
    .needs(&["server"])])
    .effects(Effects::additive())
    .document(COMMENT_EDIT);

/// The help leaf of `flare comment-delete`.
pub(super) const DELETE_LEAF: Leaf = Leaf::new(&["comment-delete"])
    .examples(&[Example::new(
        &["8d1f0c2a9b7e4c53a6f20e1d4b9c7a85", "--yes"],
        "Delete your comment without a question",
    )
    .needs(&["server"])])
    .effects(Effects::destructive())
    .document(COMMENT_DELETE);
