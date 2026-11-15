//! The single-use token that an edit form carries (BASE-24). This file is the
//! only one in the crate's source that calls `expose_secret`.

use crate::html::Builder;
use secrecy::{ExposeSecret, SecretString};

/// The server's single-use form token. The server issues it, ties it to the
/// session, binding, slot, record and blob, and checks it when the form is
/// posted. This crate only writes it into a form and reads it back. Its
/// `Debug` output is a placeholder.
#[derive(Debug, Clone)]
pub struct FormToken(SecretString);

impl FormToken {
    /// Wraps the token text that the server issued.
    #[must_use]
    pub fn new(token: impl Into<String>) -> Self {
        Self(SecretString::from(token.into()))
    }

    /// Whether the token has no text.
    pub(crate) fn is_empty(&self) -> bool {
        self.0.expose_secret().is_empty()
    }

    /// The bytes the token takes in a form's post.
    pub(crate) fn encoded_len(&self) -> usize {
        crate::form::encoded(self.0.expose_secret())
    }

    /// Writes the hidden input that carries the token.
    pub(crate) fn hidden_input(&self, builder: &mut Builder) {
        builder.open(
            "input",
            &[
                ("type", "hidden"),
                ("name", "token"),
                ("value", self.0.expose_secret()),
            ],
        );
    }
}

impl ExposeSecret<str> for FormToken {
    /// The token text, for the server to look up.
    fn expose_secret(&self) -> &str {
        self.0.expose_secret()
    }
}
