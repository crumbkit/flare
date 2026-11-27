//! `GET /s/<id>` on the hosted site: the reading page of a Markdown share
//! for a signed-in reader that `Shares::open` admits. `?version=<n>` shows
//! an older version. A missing share, a refused share and a missing version
//! give the same not-found page.

use super::Ctx;
use crate::assets;
use crate::auth::Admitted;
use crate::content::Claims;
use crate::frame;
use crate::problem::{Code, Help, Problem};
use crate::reading;
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::sharing::{ReadRefusal, Reading};
use crate::sign_in::HostedSignIn;
use flare_auth_oidc::Clock as OidcClock;
use flare_core::{PageStore, RecordStore};
use http::StatusCode;
use std::time::UNIX_EPOCH;

/// The reading page of share `id`.
pub(crate) async fn page<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    admitted: &Admitted,
    id: &str,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let api = ctx.shared.sharing.as_deref().ok_or(Code::NotFound)?;
    let version = ctx.query("version");
    if ctx.uri.query().is_some_and(|query| !query.is_empty()) && version.is_none() {
        return Err(Code::ShareNotFound.into());
    }
    let reading = match api.read(&admitted.principal, id, version.as_deref()).await {
        Ok(reading) => reading,
        Err(ReadRefusal::NotFound) => return Err(Code::ShareNotFound.into()),
        Err(ReadRefusal::Unavailable) => {
            let again = ctx
                .uri
                .path_and_query()
                .map_or_else(|| ctx.uri.path().to_owned(), ToString::to_string);
            return Err(Problem::with_help(
                Code::SharesUnavailable,
                Help::Retry(again),
            ));
        }
    };
    if reading.html {
        return framed(ctx, admitted, &reading).await;
    }
    let article = flare_render::article(&reading.markdown, &reading.images)
        .map_err(|_| Code::CannotRender)?;
    let reader = admitted.principal.email().map_or_else(
        || admitted.principal.id().as_str().to_owned(),
        |email| email.as_str().to_owned(),
    );
    let entries = reading::Entries {
        reading: assets::entry(),
        comments: assets::comments_entry(),
    };
    let document = reading::document(&reading, &reader, &article, entries);
    let reply = Reply::html(StatusCode::OK, document);
    Ok(if reading::loads_script(&article, entries) {
        reply.with_scripts()
    } else {
        reply
    })
}

/// The reading page of an uploaded HTML share: a view link for the reader,
/// minted for ten minutes, in a frame from the content host. Only this page
/// may frame the content host's origin. A site with no content host answers
/// a share that it should not hold with the not-found page.
async fn framed<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    admitted: &Admitted,
    reading: &Reading,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let host = ctx.shared.content.as_ref().ok_or(Code::ShareNotFound)?;
    let claims = Claims {
        share: reading.id.clone(),
        version: reading.version,
        viewer: admitted.principal.clone(),
    };
    let now = ctx
        .shared
        .clock
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let token = host
        .mint(&claims, now)
        .await
        .map_err(|_| Code::SharesUnavailable)?;
    let src = format!("{}/v/{token}", host.origin().as_str());
    let (date, _) = reading::date(reading.published);
    let caption = format!("Version {}, {date}", reading.version);
    let frame = frame::uploaded(&src, &caption);
    let reader = admitted.principal.email().map_or_else(
        || admitted.principal.id().as_str().to_owned(),
        |email| email.as_str().to_owned(),
    );
    let entries = reading::Entries {
        reading: None,
        comments: assets::comments_entry(),
    };
    let document = reading::framed_document(reading, &reader, &frame, entries);
    let reply =
        Reply::html(StatusCode::OK, document).with_frame_src(host.origin().as_str().to_owned());
    Ok(if entries.comments.is_some() {
        reply.with_scripts()
    } else {
        reply
    })
}
