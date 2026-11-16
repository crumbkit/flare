//! `GET /pages/<name>` (screen 1) and `GET /b/<name>` (screen 2): the band,
//! the frame with the rendered page, and for a binding one edit form per
//! slot with editable fields.

use super::{Ctx, store_problem};
use crate::auth::Admitted;
use crate::docs::{Band, Doc, escape};
use crate::forms::Tied;
use crate::frame;
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::secret;
use hangar_auth_oidc::Clock as OidcClock;
use hangar_core::{
    Access, Binding, BindingName, Credential, Page, PageName, PageStore, PublishedPage, ReadError,
    Record, RecordStore, Resource, Slot, SlotName,
};
use hangar_render::{FormTarget, LocalPath, Notice, Schema, edit_form, render_page};
use http::StatusCode;

/// `GET /pages/<name>`: the page rendered from its samples.
pub(crate) async fn page<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    admitted: &Admitted,
    access: Access,
    name: &str,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let name = PageName::try_from(name).map_err(|_| Code::BadName)?;
    ctx.allow(admitted, access, &Resource::Page(name.clone()))
        .await?;
    let published = ctx
        .shared
        .pages
        .latest(&name)
        .await
        .map_err(|error| store_problem(&error, Code::PageNotFound))?;
    let document = render_page(published.page(), &[]).map_err(|_| Code::CannotRender)?;
    let framed = frame::frame(&document, "Showing the sample records").ok_or(Code::CannotRender)?;
    let band = Band::Page {
        name: name.as_str(),
        version: published.version().as_str(),
    };
    Ok(Reply::html(
        StatusCode::OK,
        Doc::new(name.as_str(), band).main(framed).finish(),
    ))
}

/// A binding, its pinned page and the records that fill it, read and
/// authorized for `access`.
pub(crate) struct Bound {
    pub(crate) binding: Binding,
    pub(crate) published: PublishedPage,
    pub(crate) fills: Vec<(SlotName, Record)>,
}

/// Reads binding `name`, its page at the pin and its records, asking
/// `authorize` for `access` on the binding and on each record.
pub(crate) async fn bound<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    admitted: &Admitted,
    name: &BindingName,
    access: Access,
) -> Result<Bound, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    ctx.allow(admitted, access, &Resource::Binding(name.clone()))
        .await?;
    let pages = &ctx.shared.pages;
    let binding = pages
        .binding(name)
        .await
        .map_err(|error| store_problem(&error, Code::BindingNotFound))?;
    let published = pages
        .at(binding.pin())
        .await
        .map_err(|error| store_problem(&error, Code::PageNotFound))?;
    let (slots, paths): (Vec<SlotName>, Vec<_>) = binding
        .fills()
        .iter()
        .map(|(slot, path)| (slot.clone(), path.clone()))
        .unzip();
    for path in &paths {
        ctx.allow(admitted, access, &Resource::Record(path.clone()))
            .await?;
    }
    let mut fills = Vec::with_capacity(slots.len());
    for (slot, read) in slots
        .into_iter()
        .zip(ctx.shared.records.read_all(&paths).await)
    {
        let record = read.map_err(|error| match error {
            ReadError::NotFound => Code::RecordNotFound,
            _ => Code::BoardUnavailable,
        })?;
        fills.push((slot, record));
    }
    Ok(Bound {
        binding,
        published,
        fills,
    })
}

/// The band of a binding's documents.
pub(crate) fn binding_band(bound: &Bound) -> Band<'_> {
    Band::Binding {
        name: bound.binding.name().as_str(),
        page: bound.published.page().name().as_str(),
        version: bound.published.version().as_str(),
    }
}

/// `GET /b/<name>`: the page rendered from the binding's records, and an
/// edit form for each slot with editable fields when the request has a
/// session.
pub(crate) async fn binding<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    (admitted, access): (&Admitted, Access),
    credential: Option<&Credential>,
    name: &str,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let name = BindingName::try_from(name).map_err(|_| Code::BadName)?;
    let bound = bound(ctx, admitted, &name, access).await?;
    let page = bound.published.page();
    let document = render_page(page, &bound.fills).map_err(|_| Code::CannotRender)?;
    let mut main =
        frame::frame(&document, "Showing the bound records").ok_or(Code::CannotRender)?;
    let notice = match ctx.query("notice").as_deref() {
        Some("saved") => Notice::Saved,
        Some("unchanged") => Notice::Unchanged,
        _ => Notice::Fresh,
    };
    let session = credential.and_then(secret::session_digest);
    if let Some(session) = session {
        for (slot_name, record) in &bound.fills {
            let Some(slot) = editable(page, slot_name) else {
                continue;
            };
            let form = draw(
                ctx,
                &bound.binding,
                slot,
                record,
                session,
                record.blob().clone(),
                notice,
            )
            .await;
            main.push_str(&form.unwrap_or_else(|reason| {
                format!(
                    "<p class=\"hint own\">This {} cannot be edited here: {}</p>",
                    escape(slot_name.as_str()),
                    escape(&reason)
                )
            }));
        }
    }
    let band = binding_band(&bound);
    Ok(Reply::html(
        StatusCode::OK,
        Doc::new(name.as_str(), band).main(main).finish(),
    ))
}

/// The slot `name` of `page` when it lets a form edit a field.
pub(crate) fn editable<'p>(page: &'p Page, name: &SlotName) -> Option<&'p Slot> {
    page.slots()
        .get(name)
        .filter(|slot| !slot.editable_fields().is_empty())
}

/// The schema of `slot`'s record type, parsed.
pub(crate) async fn schema<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    slot: &Slot,
) -> Result<Schema, String>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let text = ctx
        .shared
        .schemas
        .schema(slot.record_type())
        .await
        .map_err(|_| format!("the board has no schema for {}", slot.record_type()))?;
    Schema::parse(&text).map_err(|error| error.to_string())
}

/// A new token for `binding`'s `slot` filled by `record`, tied to the
/// session and to `blob`.
pub(crate) fn target<P, R, S, K: OidcClock>(
    ctx: &Ctx<'_, P, R, S, K>,
    binding: &Binding,
    slot: &Slot,
    record: &Record,
    session: secret::Digest,
    blob: hangar_core::Blob,
) -> Result<FormTarget, String> {
    let tied = Tied {
        session,
        binding: binding.name().clone(),
        slot: slot.name().clone(),
        path: record.path().clone(),
        blob,
    };
    let token = ctx
        .shared
        .forms
        .issue(tied, ctx.now)
        .ok_or_else(|| "the server could not make a form token".to_owned())?;
    let action = LocalPath::try_from(format!("/b/{}", binding.name()).as_str())
        .map_err(|error| error.to_string())?;
    Ok(FormTarget::new(action, token, binding.name().clone()))
}

/// Draws the edit form for `record` in `slot` with a new token.
async fn draw<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    binding: &Binding,
    slot: &Slot,
    record: &Record,
    session: secret::Digest,
    blob: hangar_core::Blob,
    notice: Notice<'_>,
) -> Result<String, String>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    let schema = schema(ctx, slot).await?;
    let target = target(ctx, binding, slot, record, session, blob)?;
    edit_form(slot, record, &schema, &target, notice)
        .map(|html| format!("<div class=\"own\">{}</div>", html.as_str()))
        .map_err(|error| error.to_string())
}
