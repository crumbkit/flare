//! How a deployment names people: by their verified email, or by a
//! username that a signed claim carries.

use crate::proxy::IdentityClaim;
use flare_core::{IdentityKind, Username};
use serde_json::{Map, Value};

/// How sign-in names the person it admits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum Identity {
    /// The person is their verified email.
    #[default]
    Email,
    /// The person is the username in this claim. The verified email, when
    /// there is one, is kept beside it.
    Username(IdentityClaim),
}

impl Identity {
    /// The kind of identifier that this identity gives.
    #[must_use]
    pub fn kind(&self) -> IdentityKind {
        match self {
            Self::Email => IdentityKind::Email,
            Self::Username(_) => IdentityKind::Username,
        }
    }

    /// The username that `claims` carries under the configured claim:
    /// `None` under the email kind. Under the username kind, `Some(None)`
    /// when the claim is missing, is not a string, or is not a username.
    pub(crate) fn username(&self, claims: &Map<String, Value>) -> Option<Option<Username>> {
        let Self::Username(claim) = self else {
            return None;
        };
        Some(
            claims
                .get(claim.as_str())
                .and_then(Value::as_str)
                .and_then(|text| Username::try_from(text).ok()),
        )
    }
}
