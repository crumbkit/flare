//! A deliberately wrong grant store, for the tests that show the grant
//! store suite catches the flaw each clause exists to catch. Like a store
//! on a table of items, it keeps the grant items, a person index and a
//! counter per share apart, and each flaw breaks one clause and keeps
//! every other rule.

use flare_core::{
    Grant, GrantRevoke, GrantStore, GrantStoreError, GrantWrite, Listing, MAX_GRANTS, MAX_LISTED,
    PrincipalId, Role, ShareId,
};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Barrier, Mutex};

/// The one rule a [`FlawedGrants`] breaks.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GrantFlaw {
    /// `grant` does not compare the expected role.
    IgnoresExpectedOnGrant,
    /// `revoke` does not compare the expected role.
    IgnoresExpectedOnRevoke,
    /// `grant` does not check the acting admin.
    IgnoresAdminOnGrant,
    /// `revoke` does not check the acting admin.
    IgnoresAdminOnRevoke,
    /// `grant` has no limit.
    NoLimit,
    /// `revoke` does not lower the share's counter.
    KeepsCountAfterRevoke,
    /// The limit also refuses a role change.
    RefusesRoleChangeAtLimit,
    /// The expected role is compared before the acting admin is checked.
    ChangedBeforeStaleAuthority,
    /// `revoke` leaves the person index entry.
    KeepsPersonEntryAfterRevoke,
    /// `get` finds the person's grant on any share.
    ShowsGrantOnOtherShares,
    /// `grant` compares, waits for the other caller and then writes, so two
    /// grants from no grant that run together both succeed. Only the
    /// concurrency clause may run against it.
    LosesAGrantInARace,
    /// A role change takes the write's time as the time of first grant.
    RewritesCreated,
    /// A revoke keeps the item aside, and a grant from no grant revives it.
    RevivesARevokedGrant,
    /// A grant on a share with no grants answers `Unavailable`, as a store
    /// that looked for the share would.
    RefusesAnUnseenShare,
    /// `granted_to` cuts the list before it learns whether more exist.
    WrongTruncation,
    /// `granted_to` lists the oldest first.
    WrongOrder,
    /// The admin check remembers an admin it once saw and never reads again.
    CachesAdminAuthority,
}

#[derive(Default)]
struct GrantState {
    items: BTreeMap<(ShareId, PrincipalId), Grant>,
    people: BTreeMap<PrincipalId, BTreeMap<ShareId, Grant>>,
    counts: BTreeMap<ShareId, usize>,
    aside: BTreeMap<(ShareId, PrincipalId), Grant>,
    known_admins: BTreeSet<(ShareId, PrincipalId)>,
}

/// A grant store with one [`GrantFlaw`].
pub(crate) struct FlawedGrants {
    flaw: GrantFlaw,
    state: Mutex<GrantState>,
    barrier: Barrier,
}

impl FlawedGrants {
    pub(crate) fn new(flaw: GrantFlaw) -> Self {
        Self {
            flaw,
            state: Mutex::new(GrantState::default()),
            barrier: Barrier::new(2),
        }
    }

    fn is(&self, flaw: GrantFlaw) -> bool {
        self.flaw == flaw
    }

    fn check_admin(
        &self,
        state: &mut GrantState,
        share: &ShareId,
        admin: Option<&PrincipalId>,
        ignored: bool,
    ) -> Result<(), GrantStoreError> {
        let Some(admin) = admin else { return Ok(()) };
        if ignored {
            return Ok(());
        }
        let key = (share.clone(), admin.clone());
        if self.is(GrantFlaw::CachesAdminAuthority) && state.known_admins.contains(&key) {
            return Ok(());
        }
        let role = state.items.get(&key).map(Grant::role);
        if role == Some(Role::Admin) {
            state.known_admins.insert(key);
            Ok(())
        } else {
            Err(GrantStoreError::StaleAuthority)
        }
    }

    fn check_expected(
        current: Option<Role>,
        expected: Option<Role>,
        ignored: bool,
    ) -> Result<(), GrantStoreError> {
        if current == expected || ignored {
            Ok(())
        } else {
            Err(GrantStoreError::Changed { current })
        }
    }
}

impl GrantStore for FlawedGrants {
    async fn grant(&self, write: GrantWrite) -> Result<Grant, GrantStoreError> {
        let mut state = self.state.lock().unwrap();
        let key = (write.share().clone(), write.grantee().clone());
        if self.is(GrantFlaw::RefusesAnUnseenShare)
            && state.counts.get(write.share()).copied().unwrap_or(0) == 0
        {
            return Err(GrantStoreError::Unavailable);
        }
        let current = state.items.get(&key).cloned();
        let current_role = current.as_ref().map(Grant::role);
        let ignore_expected = self.is(GrantFlaw::IgnoresExpectedOnGrant);
        let ignore_admin = self.is(GrantFlaw::IgnoresAdminOnGrant);
        if self.is(GrantFlaw::ChangedBeforeStaleAuthority) {
            Self::check_expected(current_role, write.expected(), ignore_expected)?;
        }
        self.check_admin(&mut state, write.share(), write.by().admin(), ignore_admin)?;
        Self::check_expected(current_role, write.expected(), ignore_expected)?;
        let count = state.counts.get(write.share()).copied().unwrap_or(0);
        let counts_against = current.is_none() || self.is(GrantFlaw::RefusesRoleChangeAtLimit);
        if counts_against && count >= MAX_GRANTS && !self.is(GrantFlaw::NoLimit) {
            return Err(GrantStoreError::TooMany);
        }
        if self.is(GrantFlaw::LosesAGrantInARace) {
            drop(state);
            self.barrier.wait();
            state = self.state.lock().unwrap();
        }
        let kept = if self.is(GrantFlaw::RevivesARevokedGrant) && current.is_none() {
            state.aside.get(&key).cloned()
        } else if self.is(GrantFlaw::RewritesCreated) {
            None
        } else {
            current.clone()
        };
        let grant = match &kept {
            Some(old) if self.is(GrantFlaw::RevivesARevokedGrant) && current.is_none() => {
                Grant::new(
                    old.share().clone(),
                    old.grantee().clone(),
                    write.role(),
                    old.granted_by().clone(),
                    old.created(),
                )
            }
            _ => write.applied(kept.as_ref()),
        };
        if current.is_none() {
            *state.counts.entry(write.share().clone()).or_default() += 1;
        }
        state.items.insert(key, grant.clone());
        state
            .people
            .entry(write.grantee().clone())
            .or_default()
            .insert(write.share().clone(), grant.clone());
        Ok(grant)
    }

    async fn revoke(&self, revoke: GrantRevoke) -> Result<(), GrantStoreError> {
        let mut state = self.state.lock().unwrap();
        let key = (revoke.share().clone(), revoke.grantee().clone());
        let current = state.items.get(&key).map(Grant::role);
        let ignore_expected = self.is(GrantFlaw::IgnoresExpectedOnRevoke);
        let ignore_admin = self.is(GrantFlaw::IgnoresAdminOnRevoke);
        if self.is(GrantFlaw::ChangedBeforeStaleAuthority) {
            Self::check_expected(current, Some(revoke.expected()), ignore_expected)?;
        }
        self.check_admin(
            &mut state,
            revoke.share(),
            revoke.by().admin(),
            ignore_admin,
        )?;
        Self::check_expected(current, Some(revoke.expected()), ignore_expected)?;
        if let Some(old) = state.items.remove(&key) {
            state.aside.insert(key, old);
            if !self.is(GrantFlaw::KeepsCountAfterRevoke) {
                let count = state.counts.entry(revoke.share().clone()).or_default();
                *count = count.saturating_sub(1);
            }
            if !self.is(GrantFlaw::KeepsPersonEntryAfterRevoke)
                && let Some(shares) = state.people.get_mut(revoke.grantee())
            {
                shares.remove(revoke.share());
            }
        }
        Ok(())
    }

    async fn get(
        &self,
        share: &ShareId,
        grantee: &PrincipalId,
    ) -> Result<Option<Grant>, GrantStoreError> {
        let state = self.state.lock().unwrap();
        if self.is(GrantFlaw::ShowsGrantOnOtherShares) {
            return Ok(state
                .items
                .values()
                .find(|grant| grant.grantee() == grantee)
                .cloned());
        }
        Ok(state.items.get(&(share.clone(), grantee.clone())).cloned())
    }

    async fn of_share(&self, share: &ShareId) -> Result<Vec<Grant>, GrantStoreError> {
        let state = self.state.lock().unwrap();
        let mut grants: Vec<Grant> = state
            .items
            .values()
            .filter(|grant| grant.share() == share)
            .cloned()
            .collect();
        grants.sort_by_key(|grant| (grant.created(), grant.grantee().clone()));
        Ok(grants)
    }

    async fn granted_to(&self, grantee: &PrincipalId) -> Result<Listing<Grant>, GrantStoreError> {
        let state = self.state.lock().unwrap();
        let mut grants: Vec<Grant> = state
            .people
            .get(grantee)
            .map(|shares| shares.values().cloned().collect())
            .unwrap_or_default();
        if self.is(GrantFlaw::WrongOrder) {
            grants.sort_by_key(|grant| (grant.created(), grant.share().clone()));
        } else {
            grants.sort_by_key(|grant| (Reverse(grant.created()), grant.share().clone()));
        }
        if self.is(GrantFlaw::WrongTruncation) {
            grants.truncate(MAX_LISTED);
        }
        Ok(Listing::new(grants))
    }
}
