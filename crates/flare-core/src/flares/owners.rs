//! Granting a response to the owner of each flare it responds to.

use super::{Flares, Responded};
use crate::PrincipalId;
use crate::auth::Principal;
use crate::grant::Role;
use crate::person::Person;
use crate::share::ShareId;
use crate::sharing::ShareError;
use crate::store::{CommentStore, CursorStore, GrantStore, ResponseStore, ShareStore};
use crate::teams::Teams;
use crate::time::Timestamp;

impl<S: ShareStore, G: GrantStore, R: ResponseStore, C: CommentStore, U: CursorStore, T: Teams, K>
    Flares<S, G, R, C, U, T, K>
{
    /// Grants Reader on `response` to the owner of each flare in `flares`,
    /// once per owner. `who` owns the response. An owner who already holds
    /// a role keeps it, and `who` is skipped because an owner already has
    /// access.
    ///
    /// The owner is the flare owner's principal id, which is their person's
    /// lower-case address or username, so it is parsed back into a
    /// [`Person`] for [`Shares::grant`](crate::sharing::Shares::grant); the
    /// id is the same one that grant keys by. An owner id that parses as
    /// neither kind is a store fault and answers `Unavailable`.
    pub(super) async fn grant_owners(
        &self,
        who: &Principal,
        response: &ShareId,
        flares: &[Responded],
        at: Timestamp,
    ) -> Result<(), ShareError> {
        // The response's owner is the caller's person's id, which can
        // differ from `who.id()` when the sign-in names its own id.
        let caller = self.shares.person(who).ok().map(Person::principal_id);
        let mut owners: Vec<&PrincipalId> = Vec::new();
        for flare in flares {
            if Some(&flare.owner) != caller.as_ref() && !owners.contains(&&flare.owner) {
                owners.push(&flare.owner);
            }
        }
        if owners.is_empty() {
            return Ok(());
        }
        let current = self.shares.grants(who, response).await?;
        for owner in owners {
            let held = current.iter().any(|grant| grant.grantee() == owner);
            if !held {
                let person =
                    Person::try_from(owner.as_str()).map_err(|_| ShareError::Unavailable)?;
                self.shares
                    .grant(who, response, &person, Role::Reader, at)
                    .await?;
            }
        }
        Ok(())
    }
}
