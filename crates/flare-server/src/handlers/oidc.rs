//! OpenID Connect sign-in on both sites.
//!
//! On the local site, the sign-in host `localhost:<port>` serves
//! `/auth/oidc/start` and `/auth/oidc/callback` and 404 for everything
//! else. It never sets the session cookie: the callback hands the session
//! to the launch host's finish route.
//!
//! On the hosted site, the same two paths are routes of the table that
//! name the hosted site only, and run after `hosted::check` and
//! `checks::cross_site` like every hosted route. The callback passes the
//! session that `hosted::check` parsed to `complete` as the old one, sets
//! `__Host-flare_session` and answers 303 to the return path.
//!
//! Both sites read the kept-state cookie with `cookies.rs`'s byte-level
//! parser, so a foreign non-ASCII cookie cannot fail a sign-in.

use super::Ctx;
use crate::checks;
use crate::cookies;
use crate::handover::Handed;
use crate::metrics::{Event, Reason};
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::secret::{self, HOSTED_CLEAR_KEPT, HOSTED_KEPT_COOKIE, KEPT_COOKIE};
use crate::sign_in::HostedSignIn;
use flare_auth_loopback::ReturnPath;
use flare_auth_oidc::{Clock as OidcClock, ReturnTo, SignInRefusal, SignedIn};
use flare_core::{Credential, PageStore, RecordStore};
use http::Method;
use std::net::IpAddr;

/// Clears the kept-state cookie.
const CLEAR_KEPT: &str = "flare_oidc_state=; Path=/auth/oidc; HttpOnly; SameSite=Lax; Max-Age=0";

pub(crate) async fn handle<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    port: u16,
    peer: IpAddr,
    method: &Method,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    checks::sign_in_host(peer, ctx.headers, port)?;
    let path = ctx.uri.path();
    let known = matches!(path, "/auth/oidc/start" | "/auth/oidc/callback");
    let Some(oidc) = ctx.shared.site.oidc().filter(|_| known) else {
        return Err(Code::NotFound.into());
    };
    if method != Method::GET {
        return Err(Code::MethodNotAllowed.into());
    }
    if path == "/auth/oidc/start" {
        let return_to = return_to(ctx)?;
        let start = oidc.start(return_to).await.map_err(refused)?;
        return Ok(Reply::see_other(start.redirect().as_str().to_owned())
            .cookie(secret::kept_cookie(start.kept())));
    }
    let kept = cookies::kept_state(ctx.headers, KEPT_COOKIE);
    let query = ctx.uri.query().unwrap_or_default();
    let answer = match oidc.complete(query, kept.as_ref(), None).await {
        Ok(signed_in) => {
            let handed = Handed {
                session: signed_in.session().clone(),
                return_to: signed_in.return_to().as_str().to_owned(),
                lasts: signed_in.cookie_life(),
            };
            match ctx.shared.handover.issue(handed, ctx.now) {
                Some(issued) => {
                    if let Some(dropped) = issued.dropped {
                        let _ = oidc.sign_out(&dropped).await;
                    }
                    match ctx.shared.site.gate() {
                        Some(gate) => {
                            ctx.shared.count(Event::SignedIn);
                            let origin = gate.loopback().launch_host().origin();
                            Ok(Reply::see_other(secret::finish_url(&origin, &issued.code)))
                        }
                        None => Err(Code::OidcUnavailable),
                    }
                }
                None => {
                    let _ = oidc.sign_out(signed_in.session()).await;
                    ctx.shared.count(Event::SignInRefused {
                        reason: Reason::NoHandoverSlot,
                    });
                    Err(Code::OidcUnavailable)
                }
            }
        }
        Err(refusal) => {
            ctx.shared.count(Event::SignInRefused {
                reason: Reason::of(refusal),
            });
            Err(refused(refusal))
        }
    };
    match answer {
        Ok(reply) => Ok(reply.cookie(CLEAR_KEPT.to_owned())),
        Err(code) => Ok(Problem::from(code)
            .reply(ctx.headers)
            .cookie(CLEAR_KEPT.to_owned())),
    }
}

/// The checked `return_to` of a start: a path on this server.
fn return_to<P, R, S, K, H>(ctx: &Ctx<'_, P, R, S, K, H>) -> Result<ReturnTo, Code>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let back = ctx.query("return_to").ok_or(Code::BadReturn)?;
    ReturnPath::try_from(back.as_str()).map_err(|_| Code::BadReturn)?;
    ReturnTo::try_from(back.as_str()).map_err(|_| Code::BadReturn)
}

/// `GET /auth/oidc/start` on the hosted site: seals the sign-in into
/// `__Host-flare_oidc_state` and redirects to the provider.
pub(crate) async fn hosted_start<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let oidc = ctx.shared.site.oidc().ok_or(Code::NotFound)?;
    let start = oidc.start(return_to(ctx)?).await.map_err(refused)?;
    Ok(Reply::see_other(start.redirect().as_str().to_owned())
        .cookie(secret::hosted_kept_cookie(start.kept())))
}

/// `GET /auth/oidc/callback` on the hosted site. `old` is the session
/// credential `hosted::check` parsed, which a successful sign-in ends. The
/// kept-state cookie is cleared on every answer, and a refusal is logged
/// with its code and refusal, never the state or a cookie.
pub(crate) async fn hosted_callback<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    old: Option<&Credential>,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let oidc = ctx.shared.site.oidc().ok_or(Code::NotFound)?;
    let kept = cookies::kept_state(ctx.headers, HOSTED_KEPT_COOKIE);
    let query = ctx.uri.query().unwrap_or_default();
    let answer = match oidc.complete(query, kept.as_ref(), old).await {
        Ok(signed_in) => {
            ctx.shared.count(Event::SignedIn);
            Ok(signed_in_reply(&signed_in))
        }
        Err(refusal) => {
            ctx.shared.count(Event::SignInRefused {
                reason: Reason::of(refusal),
            });
            let code = refused(refusal);
            tracing::info!(code = code.text(), refusal = ?refusal, "a sign-in was refused");
            Err(code)
        }
    };
    let reply = answer.unwrap_or_else(|code| Problem::from(code).reply(ctx.headers));
    Ok(reply.cookie(HOSTED_CLEAR_KEPT.to_owned()))
}

/// The hosted callback's 303 to the sealed return path, with the session
/// cookie.
fn signed_in_reply(signed_in: &SignedIn) -> Reply {
    let cookie = secret::hosted_session_cookie(signed_in.session_secret(), signed_in.cookie_life());
    let back = signed_in.return_to().as_str();
    let target = ReturnPath::try_from(back).map_or("/flare/", |_| back);
    Reply::see_other(target.to_owned()).cookie(cookie)
}

fn refused(refusal: SignInRefusal) -> Code {
    if refusal.is_temporary() {
        Code::OidcUnavailable
    } else {
        Code::OidcRefused
    }
}
