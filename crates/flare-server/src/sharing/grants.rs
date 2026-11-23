//! The share API's grant calls: resolving `--with` identifiers into people
//! and groups, granting, listing and revoking.

use super::convert::{flare_error, grant_entry, recipient_error, share_error};
use super::run::{json, share_id, shared};
use super::{Answer, Refusal, Sharing};
use flare_contract::{ApiCode, GrantList, GrantRequest, RevokeRequest, Revoked, Role as WireRole};
use flare_core::{
    Email, GrantStore, Group, Principal, Recipients, ResponseStore, ShareStore, Teams, Timestamp,
    TokenStore,
};

/// The refusal of a request that names nobody to revoke.
fn nobody() -> Refusal {
    Refusal {
        code: ApiCode::BadBody,
        detail: Some("name a person or a team in with, or set everyone".to_owned()),
        share: None,
    }
}

impl<S, G, R, T, X, M> Sharing<S, G, R, T, X, M>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
{
    /// The people and the groups that the identifiers name, with
    /// everyone added when `everyone` is set. `team:<name>` names a team
    /// and `everyone` names everyone; the directory resolves every other
    /// identifier to an address. A malformed team name answers the same
    /// refusal as a team the caller is not in.
    pub(super) async fn named(
        &self,
        with: &[String],
        everyone: bool,
    ) -> Result<(Vec<Email>, Vec<Group>), Refusal> {
        let mut people = Vec::new();
        let mut groups = Vec::new();
        for identifier in with {
            match Group::parse(identifier) {
                Some(Ok(group)) => groups.push(group),
                Some(Err(_)) => return Err(ApiCode::TeamNotFound.into()),
                None => people.push(
                    self.recipients
                        .resolve(identifier)
                        .await
                        .map_err(|error| recipient_error(error, identifier))?,
                ),
            }
        }
        if everyone {
            groups.push(Group::Everyone);
        }
        Ok((people, groups))
    }

    pub(super) async fn grant(
        &self,
        who: &Principal,
        id: &str,
        request: GrantRequest,
        now: Timestamp,
    ) -> Answer {
        let id = share_id(id)?;
        let (with, groups) = self.named(&request.with, request.everyone).await?;
        let re = request
            .re
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>()?;
        let flared = self
            .flares
            .grant_all(who, &id, &with, &groups, &re, now)
            .await
            .map_err(flare_error)?;
        let owner = who.email().map(Email::principal_id);
        let role = if owner.as_ref() == Some(flared.share.owner()) {
            WireRole::Owner
        } else {
            WireRole::Admin
        };
        json(200, &shared(&flared, role))
    }

    pub(super) async fn list_grants(&self, who: &Principal, id: &str) -> Answer {
        let id = share_id(id)?;
        let listed = self
            .flares
            .shares()
            .listed_grants(who, &id)
            .await
            .map_err(share_error)?;
        json(
            200,
            &GrantList {
                share: id.as_str().to_owned(),
                items: listed.iter().map(grant_entry).collect(),
            },
        )
    }

    pub(super) async fn revoke_grants(
        &self,
        who: &Principal,
        id: &str,
        request: RevokeRequest,
    ) -> Answer {
        if request.with.is_empty() && !request.everyone {
            return Err(nobody());
        }
        let id = share_id(id)?;
        let (with, groups) = self.named(&request.with, request.everyone).await?;
        let revoked = self
            .flares
            .revoke(who, &id, &with, &groups)
            .await
            .map_err(flare_error)?;
        json(
            200,
            &Revoked {
                share: id.as_str().to_owned(),
                revoked: revoked
                    .people
                    .iter()
                    .map(|email| email.as_str().to_owned())
                    .chain(revoked.groups.iter().map(ToString::to_string))
                    .collect(),
            },
        )
    }
}
