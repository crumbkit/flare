//! Who may create a share, and how a `--with` identifier names a person.

use crate::auth::Principal;
use crate::email::Email;
use crate::error::{ParseError, Subject};
use crate::person::{IdentityKind, Person};
use std::collections::HashMap;
use std::future::Future;

/// A whole email domain, such as `example.com`, written `@example.com` in
/// a creators list and held in lower case.
///
/// Shortcut: only an exact domain is accepted. Wildcards and subdomains
/// are refused, so `@example.com` does not admit `ana@mail.example.com`.
/// A domain is a run of visible ASCII characters without a second `@` or
/// a `*`, and is not checked against DNS syntax, because the provider has
/// verified the address and flare only compares its domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatorDomain(String);

impl TryFrom<&str> for CreatorDomain {
    type Error = ParseError;

    /// Admits `@` and a domain, such as `@example.com`.
    fn try_from(text: &str) -> Result<Self, ParseError> {
        let malformed = ParseError::Malformed {
            subject: Subject::Email,
            rule: "a domain entry is an @ and a domain, such as @example.com, without a * or a second @",
        };
        let Some(domain) = text.strip_prefix('@') else {
            return Err(malformed);
        };
        let clean = domain
            .bytes()
            .all(|byte| byte.is_ascii_graphic() && byte != b'@' && byte != b'*');
        if domain.is_empty() || domain.len() > 253 || !clean {
            return Err(malformed);
        }
        Ok(Self(domain.to_ascii_lowercase()))
    }
}

impl CreatorDomain {
    /// Whether `email` is at exactly this domain.
    fn holds(&self, email: &Email) -> bool {
        email
            .as_str()
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain == self.0)
    }
}

/// Why a [`Creators`] rule could not say whether a person may create.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CreatorsError {
    /// The directory that the rule asks did not answer.
    #[error("the creators directory is unavailable")]
    Unavailable,
}

/// Who may create a share, responses included, and a team. Each app that
/// adopts flare supplies its own rule, such as one that asks its directory
/// for a group; [`AllAdmitted`] is the default.
///
/// [`Flares`](super::Flares) asks the rule only about a principal with a
/// person of the deployment's identifier kind, after the sign-in's admit
/// rule let them in, so an implementation never sees a principal without
/// one.
pub trait Creators: Send + Sync {
    /// Whether `who` may create a share or a team.
    ///
    /// # Errors
    ///
    /// Returns [`CreatorsError::Unavailable`] when the rule's directory
    /// does not answer.
    fn may_create(
        &self,
        who: &Principal,
    ) -> impl Future<Output = Result<bool, CreatorsError>> + Send;
}

/// The default [`Creators`]: every person the sign-in admits may create.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AllAdmitted;

impl Creators for AllAdmitted {
    async fn may_create(&self, _who: &Principal) -> Result<bool, CreatorsError> {
        Ok(true)
    }
}

/// [`Creators`] from a fixed list: these people, named by address or by
/// username, and everyone whose verified email is at one of these domains.
/// A principal with neither may not create.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreatorList {
    people: Vec<Person>,
    domains: Vec<CreatorDomain>,
}

impl CreatorList {
    /// Only these people.
    #[must_use]
    pub fn only(people: Vec<Person>) -> Self {
        Self::listed(people, Vec::new())
    }

    /// These people and everyone whose verified email is at these domains.
    #[must_use]
    pub fn listed(people: Vec<Person>, domains: Vec<CreatorDomain>) -> Self {
        Self { people, domains }
    }
}

impl Creators for CreatorList {
    async fn may_create(&self, who: &Principal) -> Result<bool, CreatorsError> {
        let listed = who
            .person()
            .is_some_and(|person| self.people.contains(person));
        let at_domain = who
            .email()
            .is_some_and(|email| self.domains.iter().any(|domain| domain.holds(email)));
        Ok(listed || at_domain)
    }
}

/// Why a `--with` identifier names nobody.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RecipientError {
    /// The identifier is not an email address, and the deployment resolves
    /// only addresses.
    #[error("the recipient is not an email address")]
    NotAnAddress,
    /// The identifier is not a username, and the deployment names people
    /// by username.
    #[error("the recipient is not a username")]
    NotAUsername,
    /// The deployment's directory could not answer.
    #[error("the recipients directory is unavailable")]
    Unavailable,
}

/// Turns a `--with` identifier into the person that grants bind to. A
/// deployment supplies its own, such as one that looks a name up in its
/// directory.
pub trait Recipients: Send + Sync {
    /// The person that `identifier` names.
    ///
    /// # Errors
    ///
    /// Returns [`RecipientError::NotAnAddress`] or
    /// [`RecipientError::NotAUsername`] for an identifier the deployment
    /// cannot resolve, and [`RecipientError::Unavailable`].
    fn resolve(
        &self,
        identifier: &str,
    ) -> impl Future<Output = Result<Person, RecipientError>> + Send;
}

/// The refusal of an identifier that is not a person of `kind`.
fn not_of(kind: IdentityKind) -> RecipientError {
    match kind {
        IdentityKind::Email => RecipientError::NotAnAddress,
        IdentityKind::Username => RecipientError::NotAUsername,
    }
}

/// The default [`Recipients`]: an identifier must be a person of the
/// deployment's kind, an email address or a username.
///
/// Shortcut: names that are not of the kind, user ids and `team:`
/// recipients are refused, and nothing is looked up. A deployment that
/// needs them supplies its own `Recipients`.
#[derive(Debug, Clone, Copy, Default)]
pub struct ByIdentity(pub IdentityKind);

impl Recipients for ByIdentity {
    async fn resolve(&self, identifier: &str) -> Result<Person, RecipientError> {
        self.0.person(identifier).map_err(|_| not_of(self.0))
    }
}

/// [`Recipients`] for a deployment that maps names to people: an
/// identifier is a configured name, else a person of the deployment's
/// kind.
///
/// Shortcut: the map is fixed when the server starts. Names are matched
/// in lower case, and user ids, `team:` recipients and a directory lookup
/// are not supported. An empty map resolves people of the kind only, like
/// [`ByIdentity`].
#[derive(Debug, Clone, Default)]
pub struct NamedRecipients {
    names: HashMap<String, Person>,
    kind: IdentityKind,
}

impl NamedRecipients {
    /// Recipients that resolve each of these names to its person, and any
    /// other identifier as an email address. A name is held in lower case.
    #[must_use]
    pub fn new(names: impl IntoIterator<Item = (String, Person)>) -> Self {
        Self {
            names: names
                .into_iter()
                .map(|(name, person)| (name.to_ascii_lowercase(), person))
                .collect(),
            kind: IdentityKind::Email,
        }
    }

    /// The same recipients, resolving an identifier that is not a name as
    /// a person of `kind`.
    #[must_use]
    pub fn with_identity(mut self, kind: IdentityKind) -> Self {
        self.kind = kind;
        self
    }
}

impl Recipients for NamedRecipients {
    async fn resolve(&self, identifier: &str) -> Result<Person, RecipientError> {
        if let Some(person) = self.names.get(&identifier.to_ascii_lowercase()) {
            return Ok(person.clone());
        }
        ByIdentity(self.kind).resolve(identifier).await
    }
}
