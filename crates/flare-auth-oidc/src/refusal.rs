//! Why `connect` failed, why a sign-in was refused and why the client secret
//! could not be read. No value here carries a secret, code, token or email.

/// Why the client secret could not be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SecretError {
    /// The program that reads the secret is missing or cannot start, as on
    /// a system without `/usr/bin/security`.
    #[error("the program that reads the client secret is unavailable")]
    Unavailable,
    /// The Keychain service name is empty, longer than 255 bytes, starts
    /// with `-` or holds a control character.
    #[error(
        "the Keychain service name must be 1 to 255 bytes, must not start with '-' and must hold no control character"
    )]
    Service,
    /// The program exited with a status other than zero, as it does when
    /// the Keychain holds no such item.
    #[error("the program that reads the client secret failed")]
    Failed,
    /// The program or the file gave nothing but a newline.
    #[error("the client secret is empty")]
    Empty,
    /// The program or the file gave more than 4 KiB.
    #[error("the client secret is longer than 4 KiB")]
    TooLong,
    /// The program or the file gave bytes that are not UTF-8.
    #[error("the client secret is not UTF-8 text")]
    NotText,
    /// The program ran past its deadline and was stopped.
    #[error("the program that reads the client secret ran past its deadline")]
    TimedOut,
    /// The secret file is missing or could not be opened or read.
    #[error("the client secret file could not be opened or read")]
    FileUnreadable,
    /// The secret file is a directory, a FIFO or anything else that is not
    /// a regular file.
    #[error("the client secret file is not a regular file")]
    NotAFile,
    /// The secret file is owned by a user other than the one this process
    /// runs as and other than root.
    #[error("the client secret file must be owned by the user flare runs as or by root")]
    FileOwner,
    /// The secret file is writable by its group or by other users.
    #[error("the client secret file must not be writable by its group or by other users")]
    FileWritable,
}

/// What is wrong with the provider's discovery document. Each fault is
/// permanent: the issuer in the config or the provider needs fixing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum MetadataFault {
    /// The document is not JSON of the expected shape.
    #[error("the provider's metadata is not a JSON document of the expected shape")]
    Malformed,
    /// The document's `issuer` differs from the configured issuer.
    #[error("the provider's metadata names another issuer")]
    IssuerMismatch,
    /// The document has no `authorization_endpoint`.
    #[error("the provider's metadata has no authorization endpoint")]
    NoAuthorizationEndpoint,
    /// The document has no `token_endpoint`.
    #[error("the provider's metadata has no token endpoint")]
    NoTokenEndpoint,
    /// The document has no `jwks_uri`.
    #[error("the provider's metadata has no key set URI")]
    NoKeySetUri,
    /// An endpoint is not an https URL.
    #[error("an endpoint in the provider's metadata does not use https")]
    InsecureEndpoint,
    /// `id_token_signing_alg_values_supported` does not list RS256.
    #[error("the provider does not sign ID tokens with RS256")]
    NoRs256,
    /// `code_challenge_methods_supported` does not list S256.
    #[error("the provider does not support the S256 PKCE method")]
    NoS256,
}

/// Why [`OidcAuthenticator::connect`](crate::OidcAuthenticator::connect)
/// failed. No authenticator exists afterwards.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ConnectError {
    /// The client secret could not be read.
    #[error("the client secret could not be read: {0}")]
    Secret(#[source] SecretError),
    /// The discovery document could not be fetched in time, or was over
    /// 64 KiB.
    #[error("the provider's discovery document could not be fetched")]
    Unreachable,
    /// The discovery document is wrong.
    #[error("the provider's metadata is wrong: {0}")]
    Metadata(#[source] MetadataFault),
    /// The key set could not be fetched or holds no usable RS256 key.
    #[error("the provider's key set could not be fetched or holds no usable RS256 signing key")]
    Keys,
    /// The system gave no random bytes for the key that seals pending
    /// sign-ins.
    #[error("no random bytes were available for the sign-in sealing key")]
    NoRandomness,
}

/// Which check an ID token failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum TokenFault {
    /// The header's `alg` is not RS256.
    #[error("the ID token is not signed with RS256")]
    Algorithm,
    /// The header names no key in the provider's key set.
    #[error("the ID token names a key that the provider does not publish")]
    UnknownKey,
    /// The signature does not verify.
    #[error("the ID token's signature does not verify")]
    Signature,
    /// `iss` differs from the configured issuer.
    #[error("the ID token names another issuer")]
    Issuer,
    /// `aud` is not the client ID alone, or `azp` names another client.
    #[error("the ID token is meant for another client")]
    Audience,
    /// `exp` passed more than 60 seconds ago.
    #[error("the ID token has expired")]
    Expired,
    /// `nonce` differs from the sign-in's nonce.
    #[error("the ID token's nonce does not match the sign-in")]
    Nonce,
    /// The token is not three base64url parts of JSON of the expected
    /// shape.
    #[error("the ID token is malformed")]
    Malformed,
}

/// Why a sign-in was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SignInRefusal {
    /// The callback query is over 4,096 bytes, has no state, repeats a
    /// parameter, or has neither or both of `code` and `error`.
    #[error("the callback query is malformed")]
    Malformed,
    /// The browser sent no kept state.
    #[error("the browser sent no sign-in state")]
    NoKeptState,
    /// The query's state differs from the kept state.
    #[error("the callback's state differs from the browser's")]
    WrongState,
    /// The kept value does not open under this process's key, so it was
    /// changed, sealed by another process or for another client, issued 10
    /// minutes or more ago, or issued more than 60 seconds ahead of the
    /// clock.
    #[error("the sign-in is unknown or expired")]
    UnknownState,
    /// The provider sent an `error` instead of a code.
    #[error("the provider refused the sign-in")]
    ProviderDenied,
    /// The token endpoint refused the code.
    #[error("the provider refused the authorization code")]
    CodeRejected,
    /// The provider did not answer in time, answered with a server error or
    /// sent more than 64 KiB.
    #[error("the provider is unavailable")]
    ProviderUnavailable,
    /// The provider answered with something the protocol does not allow,
    /// such as a token reply with no ID token or a key set with no usable
    /// key.
    #[error("the provider sent a reply the protocol does not allow")]
    ProviderMisbehaved,
    /// The ID token failed a check.
    #[error("the ID token was refused: {0}")]
    TokenRejected(#[source] TokenFault),
    /// The ID token has no email, or does not mark it verified.
    #[error("the account's email is missing or not verified")]
    EmailUnverified,
    /// The email is not on the allow-list, or under
    /// [`Admit::Domain`](crate::Admit::Domain) it cannot be a principal id.
    #[error("the account is not allowed to sign in")]
    NotAllowed,
    /// 1,024 sessions are live.
    #[error("too many sessions are live")]
    TooManySessions,
    /// The system's random number generator failed.
    #[error("no random bytes were available")]
    NoRandomness,
    /// The pending sign-in could not be sealed into the kept value: this
    /// process has sealed 2^64 values under its key, or the value is too
    /// long to seal. A restart draws a new key.
    #[error("the sign-in state could not be sealed")]
    CannotSeal,
    /// Under [`Admit::Domain`](crate::Admit::Domain), the ID token has no
    /// `hd` claim, as for a personal Google account, even one whose email
    /// ends in the configured domain.
    #[error("the account belongs to no Google Workspace organisation")]
    NoHostedDomain,
    /// Under [`Admit::Domain`](crate::Admit::Domain), the ID token's `hd`
    /// claim is not a string equal to the configured domain.
    #[error("the account belongs to another Google Workspace organisation")]
    OtherHostedDomain,
}

impl SignInRefusal {
    /// Whether the same sign-in may succeed later without any change, so a
    /// server answers 503 rather than 4xx.
    #[must_use]
    pub fn is_temporary(&self) -> bool {
        matches!(
            self,
            Self::ProviderUnavailable
                | Self::TooManySessions
                | Self::NoRandomness
                | Self::CannotSeal
        )
    }
}
