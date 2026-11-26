//! The launch host's sign-in routes: `/launch/codes` for `flare open`,
//! `/launch` for the opener's link, and `/auth/oidc/finish` for the
//! OpenID Connect hand-over. Only `/launch` and `/auth/oidc/finish` set
//! the session cookie.

use crate::docs;
use crate::handlers::Ctx;
use crate::handover::Redeemed;
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::secret;
use crate::sign_in::HostedSignIn;
use flare_auth_loopback::{CodeError, ReturnPath, SignInError};
use flare_auth_oidc::Clock as OidcClock;
use flare_core::{PageStore, RecordStore};
use http::StatusCode;

/// Where `flare open` sends the browser when it names nothing.
const HOME: &str = "/flare/";

/// `POST /launch/codes?return_to=`: a launch code, its opener file, and
/// the opener's path as JSON.
pub(crate) fn codes<P, R, S, K, H>(ctx: &Ctx<'_, P, R, S, K, H>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let target = ctx.query("return_to").unwrap_or_else(|| HOME.to_owned());
    let target = ReturnPath::try_from(target.as_str()).map_err(|_| Code::BadQuery)?;
    let (Some(gate), Some(state)) = (ctx.shared.site.gate(), ctx.shared.site.state()) else {
        return Err(Code::NotFound.into());
    };
    let loopback = gate.loopback();
    let code = loopback
        .issue_code(target, ctx.now)
        .map_err(|error| match error {
            CodeError::TooMany => Code::TooManyCodes,
            _ => Code::LaunchUnavailable,
        })?;
    let opener = state
        .write_opener(&loopback.opener_page(&code))
        .map_err(|_| Code::LaunchUnavailable)?;
    if loopback.keep_opener(&code, opener.clone()).is_err() {
        let _ = state.remove_opener(&opener);
        return Err(Code::LaunchUnavailable.into());
    }
    let Some(path) = opener.to_str() else {
        let _ = state.remove_opener(&opener);
        return Err(Code::LaunchUnavailable.into());
    };
    let answer = serde_json::json!({ "opener": path });
    Ok(Reply::with_body(
        StatusCode::OK,
        "application/json",
        answer.to_string(),
    ))
}

/// `GET /launch?code=`: signs the browser in, removes the opener, and
/// sends the browser on to the code's target.
pub(crate) fn launch<P, R, S, K, H>(ctx: &Ctx<'_, P, R, S, K, H>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let code = ctx.query("code").ok_or(Code::BadQuery)?;
    let (Some(gate), Some(state)) = (ctx.shared.site.gate(), ctx.shared.site.state()) else {
        return Err(Code::NotFound.into());
    };
    let signed_in = gate
        .loopback()
        .sign_in(&code, ctx.now)
        .map_err(|error| match error {
            SignInError::Unavailable => Code::LaunchUnavailable,
            _ => Code::LaunchRefused,
        })?;
    if let Some(opener) = signed_in.opener() {
        let _ = state.remove_opener(opener);
    }
    let document = docs::launch(signed_in.target().as_str());
    Ok(Reply::html(StatusCode::OK, document)
        .cookie(secret::loopback_cookie(signed_in.set_cookie())))
}

/// `GET /auth/oidc/finish?code=`: redeems a hand-over code and sets the
/// OpenID Connect session's cookie on the launch host.
pub(crate) async fn finish<P, R, S, K, H>(ctx: &Ctx<'_, P, R, S, K, H>) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let code = ctx.query("code").ok_or(Code::BadQuery)?;
    let handed = match ctx.shared.handover.redeem(&code, ctx.now) {
        Redeemed::Handed(handed) => handed,
        Redeemed::Expired(session) => {
            if let Some(oidc) = ctx.shared.site.oidc() {
                let _ = oidc.sign_out(&session).await;
            }
            return Err(Code::HandoverUnknown.into());
        }
        Redeemed::Unknown => return Err(Code::HandoverUnknown.into()),
        Redeemed::Unusable => return Err(Code::LaunchUnavailable.into()),
    };
    let cookie =
        secret::session_cookie(&handed.session, handed.lasts).ok_or(Code::HandoverUnknown)?;
    let target =
        ReturnPath::try_from(handed.return_to.as_str()).map_or(HOME, |_| handed.return_to.as_str());
    Ok(Reply::html(StatusCode::OK, docs::launch(target)).cookie(cookie))
}
