//! How the hosted site signs people in: flare's own OpenID Connect sign-in
//! or a single sign-on proxy's signed token.

use flare_auth_oidc::{Admit, Identity, OidcConfig, ProxyConfig};
use std::path::PathBuf;

/// How the hosted site signs people in: exactly one of the two.
#[derive(Debug)]
pub(crate) enum SignIn {
    /// flare's own OpenID Connect sign-in.
    Oidc {
        /// The settings, whose redirect URI is `<origin>/auth/oidc/callback`.
        config: OidcConfig,
        /// The file that holds the client secret.
        secret_file: PathBuf,
    },
    /// The signed token of a single sign-on proxy in front of the site.
    Proxy(ProxyConfig),
}

impl SignIn {
    /// Who the sign-in admits, which the share API's tokens follow too.
    pub(crate) fn admit(&self) -> &Admit {
        match self {
            Self::Oidc { config, .. } => config.admit(),
            Self::Proxy(config) => config.admit(),
        }
    }

    /// How the sign-in names people, which every store follows too.
    pub(crate) fn identity(&self) -> &Identity {
        match self {
            Self::Oidc { config, .. } => config.identity(),
            Self::Proxy(config) => config.identity(),
        }
    }
}
