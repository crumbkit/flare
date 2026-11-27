//! The content host's request handler.

use super::{ContentHost, ViewFailure};
use crate::handlers::Ctx;
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::sharing::ReadRefusal;
use crate::sign_in::HostedSignIn;
use flare_auth_oidc::Clock as OidcClock;
use flare_core::{PageStore, RecordStore};
use http::header::HeaderName;
use http::{HeaderMap, Method, StatusCode};
use std::time::UNIX_EPOCH;

const SEC_FETCH_DEST: HeaderName = HeaderName::from_static("sec-fetch-dest");

/// The route pattern the log names for a view, which never holds the
/// token.
const VIEW: &str = "/v/{token}";

/// The policy of a refusal's text: it loads nothing, and a frame of the
/// reading page may show it.
fn refusal_policy(flare: &str) -> String {
    format!("default-src 'none'; sandbox; frame-ancestors {flare}")
}

/// The policy of an uploaded page, which the frame's own sandbox repeats:
/// scripts and styles in the file run, and nothing loads from anywhere or
/// leaves the frame. `data:` images and fonts are the only other sources.
fn page_policy(flare: &str) -> String {
    format!(
        "sandbox allow-scripts; default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:; font-src data:; connect-src 'none'; form-action 'none'; frame-src 'none'; worker-src 'none'; object-src 'none'; base-uri 'none'; frame-ancestors {flare}"
    )
}

/// A plain-text refusal that the frame shows.
fn refusal(status: StatusCode, text: &'static str, flare: &str) -> Reply {
    Reply::view(
        status,
        "text/plain; charset=utf-8",
        text,
        refusal_policy(flare),
    )
}

/// Whether the request is a navigation of a frame.
fn framed(headers: &HeaderMap) -> bool {
    let mut values = headers.get_all(SEC_FETCH_DEST).iter();
    matches!(
        (values.next(), values.next()),
        (Some(value), None) if value.as_bytes().eq_ignore_ascii_case(b"iframe")
    )
}

/// Whether `token` has only the characters a view token holds.
fn shaped(token: &str) -> bool {
    !token.is_empty()
        && token.len() <= 2048
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b'_'))
}

/// Answers one request that arrived with the content host's name.
pub(crate) async fn serve<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    host: &ContentHost,
    method: &Method,
) -> (Result<Reply, Problem>, &'static str)
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let Some(flare) = ctx
        .shared
        .site
        .origin()
        .map(|origin| origin.as_str().to_owned())
    else {
        return (Err(Code::Host.into()), "-");
    };
    let path = ctx.uri.path();
    let get = method == Method::GET || method == Method::HEAD;
    if path == "/up" && get {
        return (Ok(Reply::empty(StatusCode::OK)), "/up");
    }
    let token = path.strip_prefix("/v/").filter(|token| shaped(token));
    let (true, Some(token), None) = (get, token, ctx.uri.query()) else {
        return (
            Ok(refusal(StatusCode::NOT_FOUND, "Not found.", &flare)),
            "-",
        );
    };
    (view(ctx, host, token, &flare).await, VIEW)
}

/// The view that `token` names.
async fn view<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    host: &ContentHost,
    token: &str,
    flare: &str,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    if !framed(ctx.headers) {
        return Err(Code::NotFramed.into());
    }
    let not_found = || Ok(refusal(StatusCode::NOT_FOUND, "Not found.", flare));
    let now = ctx
        .shared
        .clock
        .wall()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    let claims = match host.open(token, now).await {
        Ok(claims) => claims,
        Err(ViewFailure::Unknown) => return not_found(),
        Err(ViewFailure::Expired) => {
            return Ok(refusal(
                StatusCode::GONE,
                "This view link has expired. Reload the page that holds it.",
                flare,
            ));
        }
        Err(ViewFailure::Unavailable) => return Err(Code::ServerBusy.into()),
    };
    let Some(api) = ctx.shared.sharing.as_deref() else {
        return not_found();
    };
    let file = match api
        .html(&claims.viewer, &claims.share, claims.version)
        .await
    {
        Ok(file) => file,
        Err(ReadRefusal::NotFound) => return not_found(),
        Err(ReadRefusal::Unavailable) => return Err(Code::ServerBusy.into()),
    };
    Ok(Reply::view(
        StatusCode::OK,
        "text/html; charset=utf-8",
        file.bytes().as_bytes().to_vec(),
        page_policy(flare),
    ))
}
