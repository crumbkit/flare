//! [`Flares::create_team`], which applies the creators rule to a new team,
//! and [`Flares::delete_team`], which removes a team's grants from every
//! share before it removes the team.
//!
//! A delete runs in three steps, each of which a repeat by the owner
//! continues:
//!
//! 1. [`Teams::delete`] marks the team deleting, so it opens nothing.
//! 2. The team's grants are removed through its list of grants across
//!    shares, at most [`MAX_SWEPT`] in one call, and the team's activity
//!    index entries for each of those shares go with them.
//! 3. [`Teams::finish_delete`] removes the member pairs and the team, which
//!    frees the name.
//!
//! A create of a name that no team has first removes any grant
//! still listed under the name, as one left by a grant that stopped before
//! it could remove itself, so a new team never inherits a grant. One create
//! call removes at most [`MAX_SWEPT`] such grants. When more are left, the
//! call creates nothing and answers [`TeamsError::Unavailable`], and a
//! repeat removes the next ones. A deployment whose teams flare does not
//! manage ([`Teams::manages`] is false) answers
//! [`TeamsError::ManagedElsewhere`] before any grant is read or removed.

use super::{Creators, Flares, Unfinished};
use crate::auth::Principal;
use crate::store::{CommentStore, CursorStore, GrantStore, ListKey, ResponseStore, ShareStore};
use crate::teams::{Group, Team, TeamName, Teams, TeamsError};
use crate::time::Timestamp;

/// The most grants one [`Flares::delete_team`] or [`Flares::create_team`]
/// call removes. A delete of a team with more answers
/// [`TeamDeletion::finished`] false, and the next call goes on;
/// `flare teams delete` repeats the call until it finishes. A create that
/// finds more left under a free name answers [`TeamsError::Unavailable`],
/// and a repeat goes on.
pub const MAX_SWEPT: usize = 200;

/// What one [`Flares::delete_team`] call did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub struct TeamDeletion {
    /// Whether the team is gone: every grant removed, the members removed
    /// and the name free. When false, the team is deleting and another
    /// call goes on.
    pub finished: bool,
    /// How many share grants this call removed.
    pub removed: usize,
}

impl<
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: Teams,
    K: Creators,
> Flares<S, G, R, C, U, T, K>
{
    /// Creates a team owned by the caller, who must be a creator. When no
    /// team has the name, grants still listed under it are removed first,
    /// at most [`MAX_SWEPT`] in one call.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotACreator`],
    /// [`TeamsError::ManagedElsewhere`] when flare does not manage the
    /// deployment's teams, [`TeamsError::Unavailable`] when the creators
    /// rule cannot answer, when leftover grants cannot be removed or when
    /// more than [`MAX_SWEPT`] are left (a repeat removes the next ones),
    /// and the refusals of [`Teams::create`].
    pub async fn create_team(
        &self,
        who: &Principal,
        name: TeamName,
        at: Timestamp,
    ) -> Result<Team, TeamsError> {
        let Ok(owner) = self.shares.person(who) else {
            return Err(TeamsError::NotACreator);
        };
        match self.creators.may_create(who).await {
            Ok(true) => {}
            Ok(false) => return Err(TeamsError::NotACreator),
            Err(_) => return Err(TeamsError::Unavailable),
        }
        let teams = self.shares.teams();
        if !teams.manages() {
            return Err(TeamsError::ManagedElsewhere);
        }
        if !teams.taken(&name).await? {
            let (_, more) = self.sweep(&Group::Team(name.clone())).await?;
            if more {
                return Err(TeamsError::Unavailable);
            }
        }
        teams.create(owner, name, at).await
    }

    /// Deletes a team for its owner: marks it deleting, removes its grants
    /// from every share, then removes its members and the team. A call
    /// that removes [`MAX_SWEPT`] grants and finds more answers with
    /// `finished` false; the owner's next call goes on from there.
    ///
    /// # Errors
    ///
    /// Returns [`TeamsError::NotFound`], [`TeamsError::Forbidden`],
    /// [`TeamsError::ManagedElsewhere`] and [`TeamsError::Unavailable`].
    /// After `Unavailable` the team may be deleting, and a repeat by the
    /// owner continues the delete.
    pub async fn delete_team(
        &self,
        who: &Principal,
        name: &TeamName,
    ) -> Result<TeamDeletion, TeamsError> {
        let actor = self.shares.person(who).map_err(|_| TeamsError::NotFound)?;
        let teams = self.shares.teams();
        teams.delete(actor, name).await?;
        let (removed, more) = self.sweep(&Group::Team(name.clone())).await?;
        if !more {
            teams.finish_delete(actor, name).await?;
        }
        Ok(TeamDeletion {
            finished: !more,
            removed,
        })
    }

    /// Removes at most [`MAX_SWEPT`] grants of `group`, each after the
    /// group's index entries for that share, and answers how many grants
    /// went and whether more remain. Entries go first, so a call that
    /// stops leaves every remaining grant listed for the next call.
    async fn sweep(&self, group: &Group) -> Result<(usize, bool), TeamsError> {
        let unavailable = |_| TeamsError::Unavailable;
        let grantee = group.principal_id();
        let mut removed = 0;
        let mut after = None;
        loop {
            let left = MAX_SWEPT.saturating_sub(removed);
            // One grant more than the budget shows whether more remain.
            let (grants, more) = self
                .shares
                .group_grants(group, after.as_ref(), left.saturating_add(1))
                .await
                .map_err(unavailable)?;
            for (index, grant) in grants.iter().enumerate() {
                if index == left {
                    return Ok((removed, true));
                }
                after = Some(ListKey::of_grant(grant));
                self.unindex(grantee.clone(), grant.share())
                    .await
                    .map_err(|Unfinished| TeamsError::Unavailable)?;
                let ended = self
                    .shares
                    .end_grant(grant.share(), &grantee, grant.role())
                    .await
                    .map_err(unavailable)?;
                if ended {
                    removed += 1;
                }
            }
            if !more {
                return Ok((removed, false));
            }
        }
    }
}
