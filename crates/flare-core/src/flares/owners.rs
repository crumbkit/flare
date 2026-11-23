//! Granting a response to the owner of each flare it responds to.

use super::{Flares, Responded};
use crate::PrincipalId;
use crate::auth::Principal;
use crate::email::Email;
use crate::grant::Role;
use crate::share::ShareId;
use crate::sharing::ShareError;
use crate::store::{GrantStore, ResponseStore, ShareStore};
use crate::teams::Teams;
use crate::time::Timestamp;

impl<S: ShareStore, G: GrantStore, R: ResponseStore, T: Teams> Flares<S, G, R, T> {
    /// Grants Reader on `response` to the owner of each flare in `flares`,
    /// once per owner. `who` owns the response. An owner who already holds
    /// a role keeps it, and `who` is skipped because an owner already has
    /// access.
    ///
    /// The owner's address is the flare owner's principal id, which for a
    /// creator is their lower-case address, so it is parsed back into an
    /// [`Email`] for [`Shares::grant`](crate::sharing::Shares::grant); the
    /// id is the same one that grant keys by. An owner whose id is not an
    /// address cannot be a creator, so none is skipped in practice.
    pub(super) async fn grant_owners(
        &self,
        who: &Principal,
        response: &ShareId,
        flares: &[Responded],
        at: Timestamp,
    ) -> Result<(), ShareError> {
        // The response's owner is the caller's address-derived id, which
        // can differ from `who.id()` when the sign-in names its own id.
        let caller = who.email().map(Email::principal_id);
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
            let Ok(address) = Email::try_from(owner.as_str()) else {
                continue;
            };
            if !held {
                self.shares
                    .grant(who, response, &address, Role::Reader, at)
                    .await?;
            }
        }
        Ok(())
    }
}
