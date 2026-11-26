//! `GET /cli/login`, which hands a signed-in browser's login code to the
//! `flare` command.

use super::super::Ctx;
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::sharing::{Api, LoginRedirect, Refusal};
use crate::sign_in::{HostedRefusal, HostedSignIn, Unsigned};
use flare_auth_oidc::Clock as OidcClock;
use flare_contract::ApiCode;
use flare_core::{Credential, PageStore, RecordStore, Timestamp};

/// The shortest and longest state that `flare login` sends.
const STATE_CHARS: std::ops::RangeInclusive<usize> = 16..=128;

/// `GET /cli/login?port=<n>&state=<s>`: a signed-in browser gets a 303 to
/// the CLI's loopback listener with a login code; a browser with no
/// session is sent to sign in and comes back here, or, behind a proxy, is
/// sent to the proxy's sign-in address. A credential the sign-in refuses
/// for another reason gets the page answer: 401, 403 or 503.
pub(super) async fn cli_login<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    api: &dyn Api,
    credential: Option<&Credential>,
    now: Timestamp,
) -> Result<Reply, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let port = ctx
        .query("port")
        .and_then(|text| text.parse::<u16>().ok())
        .filter(|port| *port != 0)
        .ok_or(ApiCode::BadQuery)?;
    let state = ctx
        .query("state")
        .filter(|state| {
            STATE_CHARS.contains(&state.len())
                && state
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
        })
        .ok_or(ApiCode::BadQuery)?;
    // A cookie that names no one goes to sign in. Any other refusal gets the
    // answer a page gets, so the person sees why and is not sent round to
    // the sign-in again.
    let signed_in = match credential {
        Some(credential @ Credential::Session(_)) => {
            match ctx.shared.site.sign_in(credential).await {
                Ok(admitted) => Some(admitted),
                Err(HostedRefusal::SignInNeeded) => None,
                Err(refusal) => return Ok(ctx.refused(refusal).reply(ctx.headers)),
            }
        }
        _ => None,
    };
    let Some(email) = signed_in.and_then(|admitted| admitted.principal.email().cloned()) else {
        // A sign-in that is not flare's own sends the browser to its own
        // address, which the proxy in front of the site serves.
        if let Some(Unsigned::SignInAt(at)) =
            ctx.shared.site.hosted_sign_in().map(HostedSignIn::unsigned)
        {
            let at = at.ok_or(ApiCode::SignInNeeded)?;
            return Ok(Reply::see_other(at.to_owned()));
        }
        let back = format!("/cli/login?port={port}&state={state}");
        let encoded: String = form_urlencoded::byte_serialize(back.as_bytes()).collect();
        return Ok(Reply::see_other(format!(
            "/auth/oidc/start?return_to={encoded}"
        )));
    };
    let code = api.issue_code(&email, port, now).await?;
    Ok(Reply::see_other(
        LoginRedirect::new(port, state, code).location(),
    ))
}
