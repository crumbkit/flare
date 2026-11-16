//! `POST /b/<name>`: an edit form's post, handled in the order design.md
//! gives. Each refusal leaves the record unchanged.

use super::Ctx;
use super::view::{self, Bound};
use crate::auth::Admitted;
use crate::docs::{Doc, escape};
use crate::forms::{Tied, Unusable};
use crate::problem::{Code, Help, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::secret;
use crate::tasks;
use hangar_auth_oidc::Clock as OidcClock;
use hangar_core::{
    Access, BindingName, Credential, PageStore, ReadError, Record, RecordStore, Slot, WriteError,
};
use hangar_render::{FormPost, Notice, Outcome, Schema, conflict_view, edit_form};
use http::header::CONTENT_TYPE;
use http_body_util::{BodyExt as _, LengthLimitError, Limited};
use hyper::body::{Bytes, Incoming};

/// How many times a write that the store finds in conflict is read and
/// applied again.
const TRIES: usize = 3;

const FORM: &str = "application/x-www-form-urlencoded";

pub(crate) async fn post<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    (admitted, access): (&Admitted, Access),
    credential: &Credential,
    name: &str,
    body: Incoming,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let name = BindingName::try_from(name).map_err(|_| Code::BadName)?;
    let session = secret::session_digest(credential).ok_or(Code::NeedsSession)?;
    let is_form = ctx
        .headers
        .get(CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|value| {
            value
                .split(';')
                .next()
                .is_some_and(|media| media.trim().eq_ignore_ascii_case(FORM))
        });
    if !is_form {
        return Err(Code::UnsupportedType.into());
    }
    let bytes = read_body(ctx, body).await?;
    let pairs = form_urlencoded::parse(&bytes).into_owned();
    let back = format!("/b/{name}");
    let post = FormPost::from_pairs(pairs)
        .map_err(|_| Problem::with_help(Code::BadPost, Help::Back(back.clone())))?;
    let tied = ctx
        .shared
        .forms
        .take(post.token(), ctx.now)
        .map_err(|Unusable| Code::FormsUnavailable)?
        .ok_or(Code::TokenUnknown)?;
    let claims = Tied {
        session,
        binding: name.clone(),
        slot: post.slot().clone(),
        path: tied.path.clone(),
        blob: post.based_on().clone(),
    };
    if tied != claims || post.through() != &name {
        return Err(Code::TokenMismatch.into());
    }
    let bound = view::bound(ctx, admitted, &name, access).await?;
    let page = bound.published.page();
    let filled = bound.binding.fills().get(&tied.slot);
    let slot = view::editable(page, &tied.slot).filter(|_| filled == Some(&tied.path));
    let (Some(slot), Some((_, record))) = (
        slot,
        bound.fills.iter().find(|(slot, _)| slot == &tied.slot),
    ) else {
        return Err(Code::BindingChanged.into());
    };
    let schema = view::schema(ctx, slot)
        .await
        .map_err(|_| Code::BoardUnavailable)?;
    let again = Again {
        bound: &bound,
        slot,
        schema: &schema,
        session,
        post: &post,
    };
    let edit = match post.clone().into_edit(slot, &schema) {
        Ok(edit) => edit,
        Err(error) => return refused(ctx, &again, record, &error.to_string()),
    };
    let mut record = record.clone();
    for _ in 0..TRIES {
        let write = match edit.apply(&record) {
            Ok(Outcome::Write(write)) => write,
            Ok(Outcome::Unchanged) => {
                return Ok(Reply::see_other(format!("{back}?notice=unchanged")));
            }
            Ok(Outcome::Conflict(conflict)) => {
                let target = view::target(
                    ctx,
                    &bound.binding,
                    slot,
                    &record,
                    session,
                    conflict.newer().clone(),
                )
                .map_err(|_| Code::BoardUnavailable)?;
                let discard =
                    hangar_render::LocalPath::try_from(back.as_str()).map_err(|_| Code::BadName)?;
                let html = conflict_view(&conflict, &target, &discard);
                return Ok(document(
                    &bound,
                    Code::Conflict,
                    &format!("<div class=\"own\">{}</div>", html.as_str()),
                ));
            }
            // hangar_render::Outcome is #[non_exhaustive], so the compiler
            // needs this arm. Write, Unchanged and Conflict are the three
            // variants the lifecycle names; a variant that hangar-render
            // adds later answers 500 until this match names it.
            Ok(_) => return Err(Code::Fault.into()),
            Err(error) => return refused(ctx, &again, &record, &error.to_string()),
        };
        let provenance = write.provenance(admitted.principal.clone());
        match ctx
            .shared
            .records
            .write(write.path(), write.body(), &provenance)
            .await
        {
            Ok(_) => return Ok(Reply::see_other(format!("{back}?notice=saved"))),
            Err(WriteError::Conflict) => {
                record =
                    ctx.shared
                        .records
                        .read(&tied.path)
                        .await
                        .map_err(|error| match error {
                            ReadError::NotFound => Code::RecordNotFound,
                            _ => Code::BoardUnavailable,
                        })?;
            }
            Err(WriteError::Rejected(rejection)) => {
                return refused(ctx, &again, &record, rejection.text());
            }
            Err(WriteError::NotFound) => return Err(Code::RecordNotFound.into()),
            Err(_) => return Err(Code::BoardUnavailable.into()),
        }
    }
    Err(Code::RecordsBusy.into())
}

/// Reads the body through the 1 MiB limit within the read time.
async fn read_body<P, R, S, K: OidcClock>(
    ctx: &Ctx<'_, P, R, S, K>,
    body: Incoming,
) -> Result<Bytes, Problem> {
    let limits = &ctx.shared.limits;
    let collected = tasks::within(
        limits.read_time,
        Limited::new(body, limits.body_bytes).collect(),
    )
    .await
    .ok_or(Code::Slow)?;
    match collected {
        Ok(collected) => Ok(collected.to_bytes()),
        Err(error) if error.is::<LengthLimitError>() => Err(Code::BodyTooLarge.into()),
        Err(_) => Err(Code::BadBody.into()),
    }
}

/// A binding document with `code` and `form` under the band.
fn document(bound: &Bound, code: Code, form: &str) -> Reply {
    let band = view::binding_band(bound);
    let title = bound.binding.name().as_str();
    Reply::html(
        code.status(),
        Doc::new(title, band)
            .code(code.text())
            .main(form.to_owned())
            .finish(),
    )
}

/// What a refused post is drawn again with.
struct Again<'a> {
    bound: &'a Bound,
    slot: &'a Slot,
    schema: &'a Schema,
    session: secret::Digest,
    post: &'a FormPost,
}

/// The form drawn again with the post's entries, its `based_on` and a new
/// token, stating `message`.
fn refused<P, R, S, K: OidcClock>(
    ctx: &Ctx<'_, P, R, S, K>,
    again: &Again<'_>,
    record: &Record,
    message: &str,
) -> Result<Reply, Problem> {
    let Again {
        bound,
        slot,
        schema,
        session,
        post,
    } = *again;
    let target = view::target(
        ctx,
        &bound.binding,
        slot,
        record,
        session,
        post.based_on().clone(),
    )
    .map_err(|_| Code::BoardUnavailable)?;
    let notice = Notice::Refused { post, message };
    let form = edit_form(slot, record, schema, &target, notice).map_or_else(
        |error| format!("<p class=\"hint\">{}</p>", escape(&error.to_string())),
        |html| html.into_string(),
    );
    Ok(document(
        bound,
        Code::Refused,
        &format!("<div class=\"own\">{form}</div>"),
    ))
}
