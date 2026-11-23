//! `flare comments`, `comment`, `reply`, `comment-edit` and
//! `comment-delete`: each calls the server's comment API with the saved
//! login's token.

use crate::cli::comments::Text;
use crate::cli::{CommentArgs, CommentDeleteArgs, CommentEditArgs, CommentsArgs, ReplyArgs};
use crate::client::Api;
use crate::codes::{Failed, Refused};
use crate::documents::{CommentOut, CommentRow};
use crate::env::Snapshot;
use crate::sharing::{body, read_file, session, share_id};
use crumbkit::Listing;
use flare_auth_loopback::Nonce;
use flare_contract::{
    ApiCode, Comment, CommentEdit, CommentOrReply, CommentThread, NewComment, Reply,
};
use flare_core::{CommentId, CommentKey};
use reqwest::Method;

/// The most bytes of a comment file flare reads. A comment holds at most
/// 8,000 characters, which is at most 32,000 bytes, and the server refuses a
/// longer one with `flare::comments::invalid`.
const MAX_BODY_FILE: u64 = 64 * 1024;

/// The comment id that `text` names; anything else is a missing comment, as
/// the server would answer.
fn comment_id(api: &Api, text: &str) -> Result<String, Failed> {
    CommentId::try_from(text)
        .map(|id| id.as_str().to_owned())
        .map_err(|_| Refused::new(ApiCode::CommentNotFound, None, api.origin()).into())
}

/// The text of a comment, from the argument or the file.
fn text(args: &Text) -> Result<String, Failed> {
    match (&args.body, &args.body_file) {
        (Some(text), _) => Ok(text.clone()),
        (None, Some(path)) => read_file(path, MAX_BODY_FILE),
        // The argument group makes one of the two required.
        (None, None) => Ok(String::new()),
    }
}

/// The key that `given` names, or a new random one.
fn key(api: &Api, given: Option<&str>) -> Result<String, Failed> {
    match given {
        Some(text) => CommentKey::try_from(text)
            .map(|key| key.as_str().to_owned())
            .map_err(|_| Refused::new(ApiCode::CommentInvalid, None, api.origin()).into()),
        None => Nonce::new()
            .map(|nonce| nonce.as_str().to_owned())
            .map_err(|_| Failed::System {
                problem: "the operating system gave no random bytes for a comment key".to_owned(),
            }),
    }
}

fn out(answer: CommentOrReply) -> Result<CommentOut, Failed> {
    Ok(match answer {
        CommentOrReply::Comment(comment) => CommentOut {
            id: comment.id,
            flare: comment.flare,
            parent: None,
            author: comment.author,
            at: comment.at,
            edited: comment.edited,
            deleted: comment.deleted,
            body: comment.body,
        },
        CommentOrReply::Reply(reply) => CommentOut {
            id: reply.id,
            flare: reply.flare,
            parent: Some(reply.parent),
            author: reply.author,
            at: reply.at,
            edited: reply.edited,
            deleted: reply.deleted,
            body: reply.body,
        },
        // A kind a newer server sends cannot be shown.
        _ => {
            return Err(Failed::System {
                problem: "the server answered with a kind of comment this flare does not know"
                    .to_owned(),
            });
        }
    })
}

fn comment_row(comment: Comment) -> CommentRow {
    CommentRow {
        id: comment.id,
        parent: None,
        author: comment.author,
        at: comment.at,
        edited: comment.edited,
        deleted: comment.deleted,
        body: comment.body,
    }
}

fn reply_row(reply: Reply) -> CommentRow {
    CommentRow {
        id: reply.id,
        parent: Some(reply.parent),
        author: reply.author,
        at: reply.at,
        edited: reply.edited,
        deleted: reply.deleted,
        body: reply.body,
    }
}

/// Runs `flare comments`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn comments(
    args: &CommentsArgs,
    env: &Snapshot,
) -> Result<Listing<CommentRow>, Failed> {
    let api = session(env)?;
    let flare = share_id(&api, &args.flare)?;
    let target = format!("/api/comments?flare={}", flare.as_str());
    let answer: Option<CommentThread> = api.send(Method::GET, &target, None::<&()>).await?;
    let mut rows = Vec::new();
    for mut comment in body(&api, answer)?.comments {
        let replies = std::mem::take(&mut comment.replies);
        rows.push(comment_row(comment));
        rows.extend(replies.into_iter().map(reply_row));
    }
    Ok(Listing::uncounted(rows, false).described("comments and replies"))
}

async fn post(api: &Api, request: &NewComment) -> Result<CommentOut, Failed> {
    let answer: Option<CommentOrReply> = api
        .send(Method::POST, "/api/comments", Some(request))
        .await?;
    out(body(api, answer)?)
}

/// Runs `flare comment`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn comment(args: &CommentArgs, env: &Snapshot) -> Result<CommentOut, Failed> {
    let api = session(env)?;
    let flare = share_id(&api, &args.flare)?;
    let request = NewComment {
        flare: flare.as_str().to_owned(),
        parent: None,
        body: text(&args.text)?,
        key: key(&api, args.key.as_deref())?,
    };
    post(&api, &request).await
}

/// Runs `flare reply`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn reply(args: &ReplyArgs, env: &Snapshot) -> Result<CommentOut, Failed> {
    let api = session(env)?;
    let flare = share_id(&api, &args.flare)?;
    let request = NewComment {
        flare: flare.as_str().to_owned(),
        parent: Some(comment_id(&api, &args.comment)?),
        body: text(&args.text)?,
        key: key(&api, args.key.as_deref())?,
    };
    post(&api, &request).await
}

/// Runs `flare comment-edit`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn edit(args: &CommentEditArgs, env: &Snapshot) -> Result<CommentOut, Failed> {
    let api = session(env)?;
    let target = format!("/api/comments/{}", comment_id(&api, &args.comment)?);
    let request = CommentEdit {
        body: text(&args.text)?,
    };
    let answer: Option<CommentOrReply> = api.send(Method::PATCH, &target, Some(&request)).await?;
    out(body(&api, answer)?)
}

/// Runs `flare comment-delete`, after the caller has confirmed.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn delete(args: &CommentDeleteArgs, env: &Snapshot) -> Result<CommentOut, Failed> {
    let api = session(env)?;
    let target = format!("/api/comments/{}", comment_id(&api, &args.comment)?);
    let answer: Option<CommentOrReply> = api.send(Method::DELETE, &target, None::<&()>).await?;
    out(body(&api, answer)?)
}
