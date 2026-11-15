//! The server record: what `server.json` holds about the running server.

use crate::host::LaunchHost;
use crate::probe::Nonce;
use crate::secret::{compose, is_hex, proof, same};
use secrecy::SecretString;
use serde::Deserialize;

/// The version of `server.json` that this crate writes.
pub(crate) const VERSION: u64 = 1;

/// The running server's launch host and bearer token, as the server file
/// holds them.
#[derive(Debug, Clone)]
pub struct ServerRecord {
    host: LaunchHost,
    token: SecretString,
}

/// Why a server file's bytes are not a record.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unreadable {
    /// The file's version is newer than this crate knows.
    Newer(u64),
    /// The file is not a version 1 record.
    Malformed,
}

#[derive(Deserialize)]
struct Version {
    version: u64,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Fields {
    #[serde(rename = "version")]
    _version: u64,
    host: String,
    port: u16,
    token: String,
}

impl ServerRecord {
    pub(crate) fn new(host: LaunchHost, token: SecretString) -> Self {
        Self { host, token }
    }

    /// The launch host and port of the server.
    #[must_use]
    pub fn launch_host(&self) -> &LaunchHost {
        &self.host
    }

    /// The server's bearer token.
    #[must_use]
    pub fn token(&self) -> &SecretString {
        &self.token
    }

    /// Whether `proof` is the proof for `nonce` that a server holding this
    /// record's token gives, compared in constant time.
    #[must_use]
    pub fn proof_matches(&self, nonce: &Nonce, proof_given: &str) -> bool {
        same(
            proof(&self.token, nonce.as_str()).as_bytes(),
            proof_given.as_bytes(),
        )
    }

    /// Whether `other` names the same token, compared in constant time.
    pub(crate) fn same_token(&self, other: &Self) -> bool {
        crate::secret::same_secret(&self.token, &other.token)
    }

    /// The record as the JSON document of version 1.
    pub(crate) fn to_json(&self) -> SecretString {
        let (name, port) = (self.host.name(), self.host.port());
        compose(&self.token, |token| {
            format!(
                "{{\"version\":{VERSION},\"host\":\"{name}\",\"port\":{port},\"token\":\"{token}\"}}\n"
            )
        })
    }

    /// Reads a record. A version above 1 is [`Unreadable::Newer`], and
    /// anything else that is not a whole version 1 record is
    /// [`Unreadable::Malformed`].
    pub(crate) fn parse(bytes: &[u8]) -> Result<Self, Unreadable> {
        let version: Version = serde_json::from_slice(bytes).map_err(|_| Unreadable::Malformed)?;
        if version.version > VERSION {
            return Err(Unreadable::Newer(version.version));
        }
        let fields: Fields = serde_json::from_slice(bytes).map_err(|_| Unreadable::Malformed)?;
        if version.version != VERSION || !is_hex(&fields.token, 64) {
            return Err(Unreadable::Malformed);
        }
        let host = LaunchHost::parse(&fields.host, fields.port).ok_or(Unreadable::Malformed)?;
        Ok(Self::new(host, SecretString::from(fields.token)))
    }
}
