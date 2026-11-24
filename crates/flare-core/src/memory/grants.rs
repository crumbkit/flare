//! The in-memory [`GrantStore`].

use super::locked;
use crate::grant::{MAX_GRANTS, Role};
use crate::store::{GrantStore, GrantStoreError};
use crate::{Grant, GrantRevoke, GrantWrite, Listing, PrincipalId, ShareId};
use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Mutex;

/// The grants of each share, by grantee.
type Grants = BTreeMap<ShareId, BTreeMap<PrincipalId, Grant>>;

/// A [`GrantStore`] that keeps its grants in memory. Every operation is one
/// step under one lock, so the item a write checks is the item it changes.
#[derive(Default)]
pub struct MemoryGrantStore {
    state: Mutex<Grants>,
}

impl std::fmt::Debug for MemoryGrantStore {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("MemoryGrantStore")
            .finish_non_exhaustive()
    }
}

impl MemoryGrantStore {
    /// Builds an empty store.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }
}

/// Answers `StaleAuthority` unless `admin`, when there is one, holds an
/// Admin grant among `grants`.
fn check_admin(
    grants: Option<&BTreeMap<PrincipalId, Grant>>,
    admin: Option<&PrincipalId>,
) -> Result<(), GrantStoreError> {
    let Some(admin) = admin else {
        return Ok(());
    };
    let role = grants.and_then(|grants| grants.get(admin)).map(Grant::role);
    if role == Some(Role::Admin) {
        Ok(())
    } else {
        Err(GrantStoreError::StaleAuthority)
    }
}

impl GrantStore for MemoryGrantStore {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        let mut state = locked(&self.state).ok_or(GrantStoreError::Unavailable)?;
        let share = state.get(write.share());
        check_admin(share, write.by().admin())?;
        let current = share.and_then(|grants| grants.get(write.grantee()));
        let current_role = current.map(Grant::role);
        if current_role != write.expected() {
            return Err(GrantStoreError::Changed {
                current: current_role,
            });
        }
        let count = share.map_or(0, BTreeMap::len);
        if current.is_none() && count >= MAX_GRANTS {
            return Err(GrantStoreError::TooMany);
        }
        let grant = write.applied(current);
        state
            .entry(write.share().clone())
            .or_default()
            .insert(write.grantee().clone(), grant.clone());
        Ok(grant)
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        let mut state = locked(&self.state).ok_or(GrantStoreError::Unavailable)?;
        let share = state.get(revoke.share());
        check_admin(share, revoke.by().admin())?;
        let current = share
            .and_then(|grants| grants.get(revoke.grantee()))
            .map(Grant::role);
        if current != Some(revoke.expected()) {
            return Err(GrantStoreError::Changed { current });
        }
        if let Some(grants) = state.get_mut(revoke.share()) {
            grants.remove(revoke.grantee());
            if grants.is_empty() {
                state.remove(revoke.share());
            }
        }
        Ok(())
    }

    async fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        let state = locked(&self.state).ok_or(GrantStoreError::Unavailable)?;
        Ok(state
            .get(share)
            .and_then(|grants| grants.get(grantee))
            .cloned())
    }

    async fn of_share(&self, share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        let state = locked(&self.state).ok_or(GrantStoreError::Unavailable)?;
        let mut grants: Vec<Grant> = state
            .get(share)
            .map(|grants| grants.values().cloned().collect())
            .unwrap_or_default();
        grants.sort_by_key(|grant| (grant.created(), grant.grantee().clone()));
        Ok(grants)
    }

    async fn granted_to(&self, grantee: &PrincipalId) -> Result<Listing<Grant>, GrantStoreError> {
        let state = locked(&self.state).ok_or(GrantStoreError::Unavailable)?;
        let mut grants: Vec<Grant> = state
            .values()
            .filter_map(|grants| grants.get(grantee).cloned())
            .collect();
        grants.sort_by_key(|grant| (Reverse(grant.created()), grant.share().clone()));
        Ok(Listing::new(grants))
    }
}
