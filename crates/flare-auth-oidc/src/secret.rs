//! Where the client secret comes from.

use crate::refusal::SecretError;
use secrecy::SecretString;

/// Supplies the client secret. `connect` calls it once.
pub trait ClientSecretSource: Send + Sync {
    /// Reads the client secret.
    ///
    /// # Errors
    ///
    /// Returns a [`SecretError`] that says why the secret could not be read.
    fn client_secret(&self) -> Result<SecretString, SecretError>;
}

/// A client secret that the caller already holds, for tests and for
/// platforms without the macOS Keychain.
#[derive(Debug, Clone)]
pub struct FixedSecret(SecretString);

impl FixedSecret {
    /// Holds `secret`.
    #[must_use]
    pub fn new(secret: SecretString) -> Self {
        Self(secret)
    }
}

impl ClientSecretSource for FixedSecret {
    fn client_secret(&self) -> Result<SecretString, SecretError> {
        Ok(self.0.clone())
    }
}
