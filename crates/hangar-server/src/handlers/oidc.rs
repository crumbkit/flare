//! The sign-in host, `localhost:<port>`: `/auth/oidc/start` and
//! `/auth/oidc/callback`, and 404 for everything else. It never sets the
//! session cookie.

use super::Ctx;
use crate::checks;
use crate::handover::Handed;
use crate::problem::{Code, Problem};
use crate::reply::Reply;
use crate::schemas::Schemas;
use crate::secret::{self, KEPT_COOKIE};
use hangar_auth_loopback::ReturnPath;
use hangar_auth_oidc::{Clock as OidcClock, KeptState, ReturnTo, SignInRefusal};
use hangar_core::{PageStore, RecordStore};
use http::Method;
use http::header::COOKIE;
use std::net::IpAddr;

/// Clears the kept-state cookie.
const CLEAR_KEPT: &str = "hangar_oidc_state=; Path=/auth/oidc; HttpOnly; SameSite=Lax; Max-Age=0";

pub(crate) async fn handle<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    port: u16,
    peer: IpAddr,
    method: &Method,
) -> Result<Reply, Problem>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    checks::sign_in_host(peer, ctx.headers, port)?;
    let path = ctx.uri.path();
    let known = matches!(path, "/auth/oidc/start" | "/auth/oidc/callback");
    let Some(oidc) = ctx.shared.gate.oidc().filter(|_| known) else {
        return Err(Code::NotFound.into());
    };
    if method != Method::GET {
        return Err(Code::MethodNotAllowed.into());
    }
    if path == "/auth/oidc/start" {
        let back = ctx.query("return_to").ok_or(Code::BadReturn)?;
        ReturnPath::try_from(back.as_str()).map_err(|_| Code::BadReturn)?;
        let return_to = ReturnTo::try_from(back.as_str()).map_err(|_| Code::BadReturn)?;
        let start = oidc.start(return_to).map_err(refused)?;
        return Ok(Reply::see_other(start.redirect().as_str().to_owned())
            .cookie(secret::kept_cookie(start.kept())));
    }
    let kept = kept_state(ctx);
    let query = ctx.uri.query().unwrap_or_default();
    let answer = match oidc.complete(query, kept.as_ref()).await {
        Ok(signed_in) => {
            let handed = Handed {
                session: signed_in.session().clone(),
                return_to: signed_in.return_to().as_str().to_owned(),
                lasts: signed_in.expires_in(),
            };
            match ctx.shared.handover.issue(handed, ctx.now) {
                Some(issued) => {
                    if let Some(dropped) = issued.dropped {
                        oidc.sign_out(&dropped);
                    }
                    let origin = ctx.shared.gate.loopback().launch_host().origin();
                    Ok(Reply::see_other(secret::finish_url(&origin, &issued.code)))
                }
                None => {
                    oidc.sign_out(signed_in.session());
                    Err(Code::OidcUnavailable)
                }
            }
        }
        Err(refusal) => Err(refused(refusal)),
    };
    match answer {
        Ok(reply) => Ok(reply.cookie(CLEAR_KEPT.to_owned())),
        Err(code) => Ok(Problem::from(code)
            .reply(ctx.headers)
            .cookie(CLEAR_KEPT.to_owned())),
    }
}

fn refused(refusal: SignInRefusal) -> Code {
    if refusal.is_temporary() {
        Code::OidcUnavailable
    } else {
        Code::OidcRefused
    }
}

/// The kept-state cookie, when the request carries exactly one.
fn kept_state<P, R, S, K: OidcClock>(ctx: &Ctx<'_, P, R, S, K>) -> Option<KeptState> {
    let mut found = None;
    for header in &ctx.headers.get_all(COOKIE) {
        let text = header.to_str().ok()?;
        for pair in text.split(';') {
            if let Some((name, value)) = pair.trim().split_once('=')
                && name == KEPT_COOKIE
            {
                if found.is_some() {
                    return None;
                }
                found = Some(KeptState::from_cookie(value));
            }
        }
    }
    found
}
