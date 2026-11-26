//! The `proxy` section: the hosted site signs people in with the signed
//! token that a single sign-on proxy in front of it sets as a cookie. It
//! holds no secret. A refusal names the key.

use super::admit::{AdmitSection, admit};
use flare_auth_oidc::{
    Audience, CookieName, IdentityClaim, KeySetUrl, ProxyAlgorithm, ProxyAlgorithms, ProxyConfig,
    ProxyIssuer, ProxyLink,
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ProxySection {
    issuer: String,
    audience: String,
    jwks_url: String,
    cookie: String,
    algorithms: Vec<String>,
    identity_claim: Option<String>,
    admit: AdmitSection,
    sign_in_url: Option<String>,
    sign_out_url: Option<String>,
}

impl ProxySection {
    /// Whether the admit rule admits anyone.
    pub(super) fn admits_anyone(&self) -> bool {
        self.admit.is_anyone()
    }
}

/// `result` with its error prefixed by the key it is for.
fn at<T, E: std::fmt::Display>(key: &str, result: Result<T, E>) -> Result<T, String> {
    result.map_err(|error| format!("proxy.{key} is not valid: {error}"))
}

/// The proxy sign-in's settings from the section.
///
/// # Errors
///
/// Returns the problem, naming the key.
pub(super) fn proxy(section: ProxySection) -> Result<ProxyConfig, String> {
    let listed = section
        .algorithms
        .iter()
        .map(|name| ProxyAlgorithm::try_from(name.as_str()))
        .collect::<Result<Vec<_>, _>>();
    let algorithms = at(
        "algorithms",
        listed.and_then(|listed| ProxyAlgorithms::new(&listed)),
    )?;
    let admit =
        admit(section.admit, "proxy").map_err(|problem| format!("proxy.admit: {problem}"))?;
    let mut config = ProxyConfig::new(
        at("issuer", ProxyIssuer::try_from(section.issuer.as_str()))?,
        at("audience", Audience::try_from(section.audience.as_str()))?,
        at("jwks_url", KeySetUrl::try_from(section.jwks_url.as_str()))?,
        at("cookie", CookieName::try_from(section.cookie.as_str()))?,
        algorithms,
        admit,
    );
    if let Some(claim) = &section.identity_claim {
        config = config.with_identity_claim(at(
            "identity_claim",
            IdentityClaim::try_from(claim.as_str()),
        )?);
    }
    if let Some(link) = &section.sign_in_url {
        config = config.with_sign_in_url(at("sign_in_url", ProxyLink::try_from(link.as_str()))?);
    }
    if let Some(link) = &section.sign_out_url {
        config = config.with_sign_out_url(at("sign_out_url", ProxyLink::try_from(link.as_str()))?);
    }
    Ok(config)
}

#[cfg(test)]
mod tests;
