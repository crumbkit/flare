//! Who calls the share API: the owner of the request's flare token, the
//! person the site's sign-in names for a bearer that is not a flare token,
//! or the person of a page's session cookie.

use super::super::Ctx;
use crate::schemas::Schemas;
use crate::sharing::{Api, Refusal, TokenCheck, bearer_of};
use crate::sign_in::HostedSignIn;
use flare_auth_oidc::Clock as OidcClock;
use flare_contract::ApiCode;
use flare_core::{Credential, PageStore, Principal, RecordStore, Timestamp};

/// The caller that the request's bearer token or, on a route that takes
/// one, session cookie proves. A cookie's person must still pass the admit
/// rule, as a token's owner does.
pub(super) async fn caller<P, R, S, K, H>(
    ctx: &Ctx<'_, P, R, S, K, H>,
    api: &dyn Api,
    credential: Option<&Credential>,
    now: Timestamp,
) -> Result<Principal, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
    H: HostedSignIn<K>,
{
    let credential = credential.ok_or(ApiCode::SignInNeeded)?;
    if matches!(credential, Credential::Session(_)) {
        // `hosted::check` passes a cookie only to the routes that take one.
        return match ctx.shared.site.authenticate(credential).await {
            Ok(admitted) if api.admits(&admitted.principal) => Ok(admitted.principal),
            Ok(_) | Err(flare_core::AuthError::Rejected) => Err(ApiCode::SignInNeeded.into()),
            Err(_) => Err(ApiCode::TokensUnavailable.into()),
        };
    }
    let token = bearer_of(credential).ok_or(ApiCode::SignInNeeded)?;
    match api.authenticate(token, now).await {
        TokenCheck::Principal(principal) => Ok(principal),
        TokenCheck::Refused(code) => Err(code.into()),
        TokenCheck::NotOurs => match ctx.shared.site.authenticate(credential).await {
            Ok(admitted) => Ok(admitted.principal),
            Err(flare_core::AuthError::Rejected) => Err(ApiCode::SignInNeeded.into()),
            Err(_) => Err(ApiCode::TokensUnavailable.into()),
        },
    }
}
