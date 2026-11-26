//! The names of teams and groups, and the audience an everyone grant
//! records from the deployment's admit rule.

use crate::auth::Principal;
use crate::email::Email;
use crate::error::{ParseError, Subject};
use crate::name::PrincipalId;
use std::fmt;

/// The longest team name, in bytes.
const MAX_TEAM_NAME: usize = 40;

/// The longest mail domain, in bytes.
const MAX_DOMAIN: usize = 253;

/// The grantee id of the everyone group.
const EVERYONE: &str = "everyone";

/// The prefix of a team's grantee id and of a `--with` identifier that
/// names a team.
const TEAM_PREFIX: &str = "team:";

/// A team's name: 1 to 40 lowercase ASCII letters, digits and dashes. One
/// team at a time has a name in a deployment; once a team's delete
/// finishes, the name is free for a new team.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TeamName(String);

impl TryFrom<&str> for TeamName {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::TeamName;
        if text.is_empty() {
            return Err(ParseError::Empty { subject });
        }
        if text.len() > MAX_TEAM_NAME {
            return Err(ParseError::TooLong {
                subject,
                max: MAX_TEAM_NAME,
            });
        }
        let clean = text
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-');
        if !clean {
            return Err(ParseError::Malformed {
                subject,
                rule: "lowercase letters, digits and dashes",
            });
        }
        Ok(Self(text.to_owned()))
    }
}

impl TeamName {
    /// The name as text.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for TeamName {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// A group that a share can be granted to. A group grant gives Reader and
/// never Admin.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Group {
    /// The members of one team.
    Team(TeamName),
    /// Everyone signed in whom the grant's recorded audience covers.
    Everyone,
}

impl Group {
    /// The id that the group's grant is keyed by: `team:<name>` or
    /// `everyone`. No person has such an id: an address holds an `@`, which
    /// no group id holds, and a username is never `everyone` and holds no
    /// `:`.
    #[must_use]
    pub fn principal_id(&self) -> PrincipalId {
        PrincipalId::of_group(self.to_string())
    }

    /// The group whose grantee id is `id`, or `None` for a person's id.
    #[must_use]
    pub fn of_grantee(id: &PrincipalId) -> Option<Self> {
        match Self::parse(id.as_str()) {
            Some(Ok(group)) => Some(group),
            _ => None,
        }
    }

    /// The group that a `--with` identifier names: `everyone`, or
    /// `team:<name>`. Any other identifier names a person and answers
    /// `None`; a `team:` identifier with a malformed name answers the
    /// name's error.
    #[must_use]
    pub fn parse(identifier: &str) -> Option<Result<Self, ParseError>> {
        if identifier == EVERYONE {
            return Some(Ok(Self::Everyone));
        }
        identifier
            .strip_prefix(TEAM_PREFIX)
            .map(|name| TeamName::try_from(name).map(Self::Team))
    }
}

impl fmt::Display for Group {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Team(name) => write!(formatter, "{TEAM_PREFIX}{name}"),
            Self::Everyone => formatter.write_str(EVERYONE),
        }
    }
}

/// A mail domain, such as `example.com`, held in lower case.
///
/// Shortcut: a domain is 1 to 253 ASCII letters, digits, dots and dashes
/// with at least one dot, and is not checked label by label, because the
/// deployment's admit rule already checked the Google Workspace domain it
/// comes from and flare only compares it with an address's domain.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MailDomain(String);

impl TryFrom<&str> for MailDomain {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        let subject = Subject::MailDomain;
        let clean = text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-');
        if text.is_empty() || text.len() > MAX_DOMAIN || !clean || !text.contains('.') {
            return Err(ParseError::Malformed {
                subject,
                rule: "a domain of letters, digits, dots and dashes with at least one dot",
            });
        }
        Ok(Self(text.to_ascii_lowercase()))
    }
}

impl MailDomain {
    /// The domain in lower case.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether `email` is at exactly this domain. A subdomain does not
    /// match.
    #[must_use]
    pub fn holds(&self, email: &Email) -> bool {
        email
            .as_str()
            .rsplit_once('@')
            .is_some_and(|(_, domain)| domain == self.0)
    }
}

/// Who an everyone grant opens a share to, recorded when the grant is
/// written so that a later, wider admit rule does not widen it.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Audience {
    /// Every signed-in person whose address is at this domain.
    Domain(MailDomain),
    /// Every person the deployment's email list admits, while the admit
    /// rule is an email list.
    Admitted,
    /// Every signed-in person, of either identifier kind, while the
    /// deployment's `everyone_signed_in` setting is on. A grant to everyone
    /// records it only under a rule that admits anyone.
    SignedIn,
}

/// The text of [`Audience::Admitted`], which no domain spells, because a
/// domain holds a dot.
const ADMITTED: &str = "admitted";

/// The text of [`Audience::SignedIn`], which no domain spells, because a
/// domain holds a dot.
const SIGNED_IN: &str = "signed-in";

impl Audience {
    /// The audience as stored: the domain, `admitted` or `signed-in`.
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::Domain(domain) => domain.as_str(),
            Self::Admitted => ADMITTED,
            Self::SignedIn => SIGNED_IN,
        }
    }
}

impl TryFrom<&str> for Audience {
    type Error = ParseError;

    fn try_from(text: &str) -> Result<Self, ParseError> {
        match text {
            ADMITTED => return Ok(Self::Admitted),
            SIGNED_IN => return Ok(Self::SignedIn),
            _ => {}
        }
        MailDomain::try_from(text)
            .map(Self::Domain)
            .map_err(|_| ParseError::Malformed {
                subject: Subject::Audience,
                rule: "a mail domain or the word admitted or signed-in",
            })
    }
}

/// The deployment's admit rule, as the share service needs it to write and
/// to read everyone grants.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Admission {
    /// The accounts of one Google Workspace domain sign in.
    Domain(MailDomain),
    /// The addresses on an email list sign in.
    Listed,
    /// Every verified account signs in, so an everyone grant would open a
    /// share to every account of the provider. It is refused unless the
    /// deployment's `everyone_signed_in` setting is on, when it records
    /// [`Audience::SignedIn`].
    Anyone,
}

impl Admission {
    /// The audience that a new everyone grant records, or `None` when the
    /// grant is refused: under a rule that admits anyone while
    /// `everyone_signed_in` is off.
    #[must_use]
    pub fn audience(&self, everyone_signed_in: bool) -> Option<Audience> {
        match self {
            Self::Domain(domain) => Some(Audience::Domain(domain.clone())),
            Self::Listed => Some(Audience::Admitted),
            Self::Anyone if everyone_signed_in => Some(Audience::SignedIn),
            Self::Anyone => None,
        }
    }

    /// Whether an everyone grant that recorded `audience` opens a share to
    /// the signed-in principal `who` under this rule. A domain audience
    /// holds a principal whose verified email is at that domain under any
    /// rule. The admitted audience holds only while the rule is still an
    /// email list, because a signed-in person was then admitted by the
    /// list. The signed-in audience holds every signed-in person while
    /// `everyone_signed_in` is on, and no one while it is off.
    #[must_use]
    pub fn opens(&self, audience: &Audience, who: &Principal, everyone_signed_in: bool) -> bool {
        match audience {
            Audience::Domain(domain) => who.email().is_some_and(|email| domain.holds(email)),
            Audience::Admitted => matches!(self, Self::Listed),
            Audience::SignedIn => everyone_signed_in,
        }
    }
}
