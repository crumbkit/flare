//! `hosted.identity`: how the hosted site names people. `"email"`, the
//! default, names each person by their verified email. `{"username_claim":
//! "<claim>"}` names each person by the username in that claim of the
//! sign-in's token, and keeps the verified email beside it when there is
//! one. Every grant, team and token of the deployment uses the same kind.

use flare_auth_oidc::{Identity, IdentityClaim};
use serde_json::Value;

/// The kind that `value`, the `hosted.identity` key, names.
///
/// # Errors
///
/// Returns the problem, naming the key.
pub(super) fn identity(value: Option<&Value>) -> Result<Identity, String> {
    let bad = || {
        "hosted.identity must be \"email\" or {\"username_claim\": \"<claim>\"}, where the claim is 1 to 64 letters, digits, dots, hyphens, underscores or colons"
            .to_owned()
    };
    match value {
        None => Ok(Identity::Email),
        Some(Value::String(word)) if word == "email" => Ok(Identity::Email),
        Some(Value::Object(map)) if map.len() == 1 => {
            let claim = map
                .get("username_claim")
                .and_then(Value::as_str)
                .ok_or_else(bad)?;
            IdentityClaim::try_from(claim)
                .map(Identity::Username)
                .map_err(|_| bad())
        }
        Some(_) => Err(bad()),
    }
}

#[cfg(test)]
mod tests;
