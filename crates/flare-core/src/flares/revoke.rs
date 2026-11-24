//! [`Flares::revoke`]: ending a grant and removing the activity index
//! entries it allowed.

use super::{FlareError, Flares};
use crate::auth::Principal;
use crate::email::Email;
use crate::share::ShareId;
use crate::sharing::ShareError;
use crate::store::{GrantStore, ResponseStore, ShareStore};

impl<S: ShareStore, G: GrantStore, R: ResponseStore> Flares<S, G, R> {
    /// Ends the grant of `grantee` on a share, as
    /// [`Shares::revoke`](crate::sharing::Shares::revoke) does, and then
    /// removes their activity index entries for the share as a response.
    /// When `grantee` is the caller's own address, the caller leaves.
    ///
    /// The grant ends before its entries go, so a failure leaves entries
    /// that `activity` skips, because it opens every response again. A
    /// repeated revoke that finds no grant still removes the entries and
    /// then answers the refusal.
    ///
    /// # Errors
    ///
    /// Returns [`FlareError::Share`] with the share service's refusal, and
    /// [`FlareError::ResponsesUnavailable`] when the grant ended and its
    /// entries could not be removed.
    pub async fn revoke(
        &self,
        who: &Principal,
        id: &ShareId,
        grantee: &Email,
    ) -> Result<(), FlareError> {
        let leaving = who.email().map(Email::principal_id) == Some(grantee.principal_id());
        let revoked = self.shares.revoke(who, id, grantee).await;
        // Only a caller allowed to revoke learns that no grant exists, and
        // a person leaving removes only their own entries.
        let unindex = match &revoked {
            Ok(()) | Err(ShareError::GrantNotFound) => true,
            Err(ShareError::NotFound) => leaving,
            Err(_) => false,
        };
        if unindex && self.unindex(grantee.principal_id(), id).await.is_err() {
            return Err(FlareError::ResponsesUnavailable);
        }
        revoked.map_err(FlareError::Share)
    }
}
