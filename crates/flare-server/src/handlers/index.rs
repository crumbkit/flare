//! The index of pages, `/flare/`, and the readiness route, `/ready`: the
//! two routes that list the page store.

use super::{Ctx, store_problem};
use crate::auth::Admitted;
use crate::docs::{self, Entry};
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::sign_in::HostedSignIn;
use crate::site::SiteKind;
use flare_auth_oidc::Clock as OidcClock;
use flare_core::{Access, PageStore, RecordStore, Resource};
use http::StatusCode;

pub(super) async fn ready<P, R, S, K, H>(ctx: &Ctx<'_, P, R, S, K, H>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    match ctx.shared.pages.pages().await {
        Ok(_) => Ok(Reply::empty(StatusCode::OK)),
        Err(_) => Err(Code::BoardUnavailable.into()),
    }
}

pub(super) async fn index<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    admitted: &Admitted,
    access: Access,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let pages = &ctx.shared.pages;
    let names = pages
        .pages()
        .await
        .map_err(|error| store_problem(&error, Code::PageNotFound))?;
    let mut entries = Vec::new();
    for name in names {
        let resource = Resource::Page(name.clone());
        if ctx.allow(admitted, access, &resource).await.is_err() {
            continue;
        }
        let mut bindings = Vec::new();
        // Bindings are a local-only feature, so the hosted index lists
        // pages alone.
        let listed = match ctx.shared.site.kind() {
            SiteKind::Local => pages
                .bindings_of(&name)
                .await
                .map_err(|error| store_problem(&error, Code::PageNotFound))?,
            SiteKind::Hosted => Vec::new(),
        };
        for binding in listed {
            let resource = Resource::Binding(binding.name().clone());
            if ctx.allow(admitted, access, &resource).await.is_ok() {
                bindings.push(binding.name().as_str().to_owned());
            }
        }
        entries.push(Entry {
            page: name.as_str().to_owned(),
            bindings,
        });
    }
    Ok(Reply::html(StatusCode::OK, docs::index(&entries)))
}
