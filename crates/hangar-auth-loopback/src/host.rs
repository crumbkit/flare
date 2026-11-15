//! The launch host: the random host name `h-<32 hex>.localhost` and the
//! port of one server.

use crate::secret::{NoRandomness, is_hex, random_hex};

/// The host name and port that one server answers on. A new server makes a
/// new name, so a browser keeps its cookie apart from every other local
/// server: browsers send a cookie for a host to every port of that host,
/// and the name is what differs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchHost {
    name: String,
    port: u16,
}

impl LaunchHost {
    /// A new random host name on `port`.
    pub(crate) fn random(port: u16) -> Result<Self, NoRandomness> {
        Ok(Self {
            name: format!("h-{}.localhost", random_hex(16)?),
            port,
        })
    }

    /// The host name and port of a server file, or `None` when the name is
    /// not `h-<32 hex>.localhost` or the port is 0.
    pub(crate) fn parse(name: &str, port: u16) -> Option<Self> {
        let hex = name.strip_prefix("h-")?.strip_suffix(".localhost")?;
        (is_hex(hex, 32) && port != 0).then(|| Self {
            name: name.to_owned(),
            port,
        })
    }

    /// The host name, `h-<32 hex>.localhost`.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The port.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// The value of a request's `Host` header: `<name>:<port>`.
    #[must_use]
    pub fn authority(&self) -> String {
        format!("{}:{}", self.name, self.port)
    }

    /// The origin of the server's pages: `http://<name>:<port>`.
    #[must_use]
    pub fn origin(&self) -> String {
        format!("http://{}", self.authority())
    }
}
