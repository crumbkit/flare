//! The local principal and what a server serves.

use flare_core::{ParseError, Principal, PrincipalId, RecordPath, Resource, Subject};

/// The principal of the person who runs the server, with the id
/// `local:<name>`. The caller supplies the name, such as the account name,
/// because the crate reads no environment variable.
///
/// # Errors
///
/// Returns a [`ParseError`] when `name` is empty or the id is not printable
/// ASCII without spaces.
pub fn local_principal(name: &str) -> Result<Principal, ParseError> {
    if name.is_empty() {
        return Err(ParseError::Empty {
            subject: Subject::PrincipalId,
        });
    }
    let id = PrincipalId::try_from(format!("local:{name}").as_str())?;
    Ok(Principal::new(id))
}

/// What a server serves to its local principal.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum Served {
    /// Every page, binding and record. `flare serve` uses this.
    Everything,
    /// Every page and binding, and only the records at or under these
    /// paths. `/tasks` covers `/tasks` and `/tasks/a`, and not `/tasksx`.
    RecordsUnder(Vec<RecordPath>),
}

impl Served {
    /// Whether the server serves `resource`. A kind of resource that
    /// flare-core adds later is not served until this crate names it.
    pub(crate) fn reaches(&self, resource: &Resource) -> bool {
        match (self, resource) {
            (_, Resource::Page(_) | Resource::Binding(_))
            | (Self::Everything, Resource::Record(_)) => true,
            (Self::RecordsUnder(roots), Resource::Record(path)) => {
                roots.iter().any(|root| under(root, path))
            }
            _ => false,
        }
    }
}

fn under(root: &RecordPath, path: &RecordPath) -> bool {
    let (root, path) = (root.as_str(), path.as_str());
    path == root
        || path
            .strip_prefix(root)
            .is_some_and(|rest| rest.starts_with('/'))
}
