//! The values that the share API reads from a request's query.

use super::super::Ctx;
use crate::schemas::Schemas;
use crate::sharing::Refusal;
use flare_auth_oidc::Clock as OidcClock;
use flare_contract::ApiCode;
use flare_core::{PageStore, RecordStore, ShareVersion};

/// A number in the query between 1 and `max`, or `default` when absent.
pub(super) fn number<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
    max: usize,
    default: usize,
) -> Result<usize, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match ctx.query("limit") {
        None => Ok(default),
        Some(text) => text
            .parse::<usize>()
            .ok()
            .filter(|value| (1..=max).contains(value))
            .ok_or_else(|| ApiCode::BadQuery.into()),
    }
}

/// The share version that `version` in the query names, or `None` when
/// absent. A value that is not a number from 1 is a bad query.
pub(super) fn version<P, R, S, K>(
    ctx: &Ctx<'_, P, R, S, K>,
) -> Result<Option<ShareVersion>, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match ctx.query("version") {
        None => Ok(None),
        Some(text) => text
            .parse::<u32>()
            .ok()
            .and_then(|number| ShareVersion::try_from(number).ok())
            .map(Some)
            .ok_or_else(|| ApiCode::BadQuery.into()),
    }
}

/// Every value of `name` in the query, decoded.
pub(super) fn every(ctx_uri: &http::Uri, name: &str) -> Vec<String> {
    let query = ctx_uri.query().unwrap_or_default();
    form_urlencoded::parse(query.as_bytes())
        .filter(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .collect()
}

/// The boolean that `name` in the query gives: `true` or `false`, and
/// `false` when absent. Any other value is a bad query.
pub(super) fn flag<P, R, S, K>(ctx: &Ctx<'_, P, R, S, K>, name: &str) -> Result<bool, Refusal>
where
    P: PageStore,
    R: RecordStore,
    S: Schemas,
    K: OidcClock,
{
    match ctx.query(name).as_deref() {
        None | Some("false") => Ok(false),
        Some("true") => Ok(true),
        Some(_) => Err(ApiCode::BadQuery.into()),
    }
}
