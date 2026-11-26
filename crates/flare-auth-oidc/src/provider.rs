//! The HTTP client for the provider, discovery and the metadata checks.
//! Every reply is untrusted input of at most 64 KiB.

use crate::config::{Deadlines, IssuerUrl};
use crate::refusal::{ConnectError, MetadataFault};
use serde::Deserialize;
use url::Url;

/// The largest reply the crate reads from the provider.
const MAX_REPLY: usize = 64 * 1024;

/// Why a request to the provider gave no usable reply.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReplyError {
    /// The request failed, passed its deadline, or the reply was over
    /// 64 KiB.
    Unreachable,
}

/// A reply's status and its whole body.
#[derive(Debug)]
pub(crate) struct Reply {
    pub(crate) status: u16,
    pub(crate) body: Vec<u8>,
}

/// The provider's endpoints, from its checked metadata.
#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    pub(crate) authorization: Url,
    pub(crate) token: Url,
    pub(crate) keys: Url,
}

/// An HTTP client that follows no redirects and keeps the deadlines.
#[derive(Debug, Clone)]
pub(crate) struct Provider {
    client: reqwest::Client,
    pub(crate) endpoints: Endpoints,
}

/// Builds the HTTP client.
pub(crate) fn client(deadlines: Deadlines) -> Result<reqwest::Client, ConnectError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(deadlines.connect)
        .timeout(deadlines.total)
        .build()
        .map_err(|_| ConnectError::Unreachable)
}

/// Reads a reply's body, refusing one over 64 KiB.
async fn read(mut response: reqwest::Response) -> Result<Reply, ReplyError> {
    let status = response.status().as_u16();
    let declared = response.content_length().unwrap_or(0);
    if usize::try_from(declared).map_or(true, |length| length > MAX_REPLY) {
        return Err(ReplyError::Unreachable);
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| ReplyError::Unreachable)?
    {
        if body.len() + chunk.len() > MAX_REPLY {
            return Err(ReplyError::Unreachable);
        }
        body.extend_from_slice(&chunk);
    }
    Ok(Reply { status, body })
}

/// Sends `request` and reads its reply.
pub(crate) async fn send(request: reqwest::RequestBuilder) -> Result<Reply, ReplyError> {
    let response = request.send().await.map_err(|_| ReplyError::Unreachable)?;
    read(response).await
}

/// GETs `url` and returns the body of a 2xx reply of at most 64 KiB. Any
/// other status is [`ReplyError::Unreachable`].
pub(crate) async fn fetch(client: &reqwest::Client, url: Url) -> Result<Vec<u8>, ReplyError> {
    let reply = send(client.get(url)).await?;
    if (200..300).contains(&reply.status) {
        Ok(reply.body)
    } else {
        Err(ReplyError::Unreachable)
    }
}

/// The fields of the discovery document that the crate reads.
#[derive(Deserialize)]
struct Metadata {
    issuer: Option<String>,
    authorization_endpoint: Option<String>,
    token_endpoint: Option<String>,
    jwks_uri: Option<String>,
    id_token_signing_alg_values_supported: Option<Vec<String>>,
    code_challenge_methods_supported: Option<Vec<String>>,
}

fn endpoint(issuer: &IssuerUrl, text: &str) -> Result<Url, MetadataFault> {
    let url = Url::parse(text).map_err(|_| MetadataFault::InsecureEndpoint)?;
    if issuer.admits_endpoint(&url) {
        Ok(url)
    } else {
        Err(MetadataFault::InsecureEndpoint)
    }
}

fn lists(values: Option<&Vec<String>>, wanted: &str) -> bool {
    values.is_some_and(|values| values.iter().any(|value| value == wanted))
}

/// Checks the discovery document. The issuer must match; the authorisation
/// endpoint, the token endpoint and the key set URI must be present and
/// use https; RS256 and S256 must be listed.
pub(crate) fn check_metadata(issuer: &IssuerUrl, body: &[u8]) -> Result<Endpoints, MetadataFault> {
    let metadata: Metadata = serde_json::from_slice(body).map_err(|_| MetadataFault::Malformed)?;
    if metadata.issuer.as_deref() != Some(issuer.as_str()) {
        return Err(MetadataFault::IssuerMismatch);
    }
    let authorization = metadata
        .authorization_endpoint
        .ok_or(MetadataFault::NoAuthorizationEndpoint)?;
    let token = metadata
        .token_endpoint
        .ok_or(MetadataFault::NoTokenEndpoint)?;
    let keys = metadata.jwks_uri.ok_or(MetadataFault::NoKeySetUri)?;
    let endpoints = Endpoints {
        authorization: endpoint(issuer, &authorization)?,
        token: endpoint(issuer, &token)?,
        keys: endpoint(issuer, &keys)?,
    };
    if !lists(
        metadata.id_token_signing_alg_values_supported.as_ref(),
        "RS256",
    ) {
        return Err(MetadataFault::NoRs256);
    }
    if !lists(metadata.code_challenge_methods_supported.as_ref(), "S256") {
        return Err(MetadataFault::NoS256);
    }
    Ok(endpoints)
}

impl Provider {
    /// Fetches and checks the discovery document.
    pub(crate) async fn discover(
        issuer: &IssuerUrl,
        deadlines: Deadlines,
    ) -> Result<Self, ConnectError> {
        let client = client(deadlines)?;
        let url = issuer
            .discovery_url()
            .map_err(|_| ConnectError::Unreachable)?;
        let reply = send(client.get(url))
            .await
            .map_err(|_| ConnectError::Unreachable)?;
        if !(200..300).contains(&reply.status) {
            return Err(ConnectError::Unreachable);
        }
        let endpoints = check_metadata(issuer, &reply.body).map_err(ConnectError::Metadata)?;
        Ok(Self { client, endpoints })
    }

    /// Fetches the key set's bytes. A status other than 2xx is
    /// [`ReplyError::Unreachable`].
    pub(crate) async fn fetch_keys(&self) -> Result<Vec<u8>, ReplyError> {
        fetch(&self.client, self.endpoints.keys.clone()).await
    }

    /// The client, for the token request.
    pub(crate) fn http(&self) -> &reqwest::Client {
        &self.client
    }
}
