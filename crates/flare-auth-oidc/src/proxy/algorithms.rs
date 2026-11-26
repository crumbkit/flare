//! The algorithms a proxy may sign its tokens with, and the keys of its
//! key set that they use.

use super::config::ProxyConfigError;
use crate::keys::KeyRule;

/// An algorithm a proxy may sign its tokens with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProxyAlgorithm {
    /// RSASSA-PKCS1-v1_5 with SHA-256.
    Rs256,
    /// ECDSA on P-256 with SHA-256.
    Es256,
}

impl TryFrom<&str> for ProxyAlgorithm {
    type Error = ProxyConfigError;

    /// Admits `RS256` and `ES256`, exactly as written.
    fn try_from(text: &str) -> Result<Self, ProxyConfigError> {
        match text {
            "RS256" => Ok(Self::Rs256),
            "ES256" => Ok(Self::Es256),
            _ => Err(ProxyConfigError::Algorithm),
        }
    }
}

/// The algorithms a token's header may name: RS256, ES256 or both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProxyAlgorithms {
    rs256: bool,
    es256: bool,
}

impl ProxyAlgorithms {
    /// The algorithms listed. A repeated one counts once.
    ///
    /// # Errors
    ///
    /// Returns [`ProxyConfigError::Algorithm`] when the list is empty.
    pub fn new(listed: &[ProxyAlgorithm]) -> Result<Self, ProxyConfigError> {
        if listed.is_empty() {
            return Err(ProxyConfigError::Algorithm);
        }
        Ok(Self {
            rs256: listed.contains(&ProxyAlgorithm::Rs256),
            es256: listed.contains(&ProxyAlgorithm::Es256),
        })
    }

    /// Whether a token header's `alg` value is one of these.
    #[must_use]
    pub fn lists(&self, alg: &str) -> bool {
        (self.rs256 && alg == "RS256") || (self.es256 && alg == "ES256")
    }

    /// The keys of the key set that these algorithms use, at most 20.
    pub(crate) fn key_rule(self) -> KeyRule {
        KeyRule {
            rs256: self.rs256,
            es256: self.es256,
            max_keys: Some(MAX_KEYS),
            keeps_held_keys: true,
        }
    }
}

/// The most keys the proxy's key set may hold.
const MAX_KEYS: usize = 20;

#[cfg(test)]
mod tests;
