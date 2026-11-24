//! The generic implementation of [`Api`] over flare-core's stores.

use super::convert::{flare_error, grant_entry, share_error, standing, summary};
use super::tokens::{code_digest, digest, new_secret, random_id, same, split, token_text};
use super::{
    Answer, Api, Boxed, Call, ReadRefusal, Reading, Refusal, Sharing, TokenCheck, check_share,
    check_version,
};
use flare_contract::{
    ApiCode, ExchangeRequest, OpenedShare, Role as WireRole, ShareRequest, Shared, TokenIssued,
    VersionRequest, utc,
};
use flare_core::{
    CommentStore, CursorStore, Email, FlareError, Flared, GrantStore, LoginCode, NewFlare,
    NewToken, Principal, Recipients, ResponseStore, ShareContent, ShareError, ShareId, ShareStore,
    ShareVersion, Standing, Teams, Timestamp, TokenId, TokenLabel, TokenStore, TokenStoreError,
};
use secrecy::SecretString;
use serde::Serialize;

/// How many times a create draws a new id after `IdTaken`.
const DRAWS: usize = 3;

/// A JSON answer with `status`.
pub(super) fn json(status: u16, body: &impl Serialize) -> Answer {
    serde_json::to_string(body)
        .map(|text| (status, Some(text)))
        .map_err(|_| ApiCode::SharesUnavailable.into())
}

/// The share id that `text` names; a malformed id is a missing share.
pub(super) fn share_id(text: &str) -> Result<ShareId, Refusal> {
    ShareId::try_from(text).map_err(|_| ApiCode::ShareNotFound.into())
}

/// The token store's failure as a code.
fn token_error(error: TokenStoreError) -> ApiCode {
    match error {
        TokenStoreError::CodeRefused => ApiCode::CodeRefused,
        TokenStoreError::NotFound => ApiCode::TokenNotFound,
        _ => ApiCode::TokensUnavailable,
    }
}

/// The `Shared` body of what `share` or `grant` did, for the caller with
/// `role`.
pub(super) fn shared(flared: &Flared, role: WireRole) -> Shared {
    Shared {
        share: summary(&flared.share, role),
        granted: flared
            .granted
            .iter()
            .map(|email| email.as_str().to_owned())
            .chain(flared.groups.iter().map(ToString::to_string))
            .collect(),
        responds_to: flared
            .responds_to
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
    }
}

impl<S, G, R, C, U, T, X, M> Sharing<S, G, R, C, U, T, X, M>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
{
    async fn create(&self, who: &Principal, request: ShareRequest, now: Timestamp) -> Answer {
        let (title, text) = check_share(&request)?;
        let (with, groups) = self.named(&request.with, false).await?;
        let re = request
            .re
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>()?;
        for _ in 0..DRAWS {
            let id = ShareId::from_random(random_id().ok_or(ApiCode::SharesUnavailable)?);
            let flare = NewFlare::new(
                id,
                title.clone(),
                ShareContent::Document(text.clone()),
                with.clone(),
                re.clone(),
            )
            .with_groups(groups.clone());
            match self.flares.share(who, flare, now).await {
                Ok(flared) => return json(201, &shared(&flared, WireRole::Owner)),
                Err(FlareError::Share(ShareError::IdTaken)) => {}
                Err(error) => return Err(flare_error(error)),
            }
        }
        Err(ApiCode::SharesUnavailable.into())
    }

    /// Publishes the next version of a share for its owner. The answer is
    /// the share as the owner sees it, with the number that was written.
    async fn publish(
        &self,
        who: &Principal,
        id: &str,
        request: VersionRequest,
        now: Timestamp,
    ) -> Answer {
        let id = share_id(id)?;
        let text = check_version(&request)?;
        // This does less than an idempotent publish would: it takes no key,
        // so a client that repeats a post whose answer was lost writes a
        // second version with the same content. The owner finds the number
        // with `flare shares`. A key would need a column in every store.
        let shares = self.flares.shares();
        let number = shares
            .publish(who, &id, ShareContent::Document(text), now)
            .await
            .map_err(share_error)?;
        let opened = shares
            .open(who, &id, Some(number))
            .await
            .map_err(share_error)?;
        let mut share = summary(&opened.share, standing(opened.standing));
        share.version = number.get();
        json(201, &share)
    }

    /// Opens version `number` of a share, or its latest. A number above
    /// the latest answers `flare::shares::not_found`, as a share the caller
    /// cannot open does, after the caller's standing is checked.
    async fn open(&self, who: &Principal, id: &str, number: Option<ShareVersion>) -> Answer {
        let id = share_id(id)?;
        let opened = self
            .flares
            .shares()
            .open(who, &id, number)
            .await
            .map_err(share_error)?;
        let content = match opened.version.content() {
            ShareContent::Document(text) => text.as_str().to_owned(),
            _ => return Err(ApiCode::KindNotServed.into()),
        };
        let version = opened.version.number().get();
        let mut share = summary(&opened.share, standing(opened.standing));
        share.comments = self.live_comments(who, &id).await.ok();
        json(
            200,
            &OpenedShare {
                share,
                version,
                content,
            },
        )
    }

    async fn reading(
        &self,
        who: &Principal,
        id: &str,
        version: Option<&str>,
    ) -> Result<Reading, ReadRefusal> {
        let refused = |error: ShareError| match error {
            ShareError::Unavailable => ReadRefusal::Unavailable,
            _ => ReadRefusal::NotFound,
        };
        let id = ShareId::try_from(id).map_err(|_| ReadRefusal::NotFound)?;
        let number = version
            .map(|text| {
                text.parse::<u32>()
                    .ok()
                    .and_then(|number| ShareVersion::try_from(number).ok())
                    .ok_or(ReadRefusal::NotFound)
            })
            .transpose()?;
        let shares = self.flares.shares();
        let opened = shares.open(who, &id, number).await.map_err(refused)?;
        let ShareContent::Document(text) = opened.version.content() else {
            return Err(ReadRefusal::NotFound);
        };
        let with = match opened.standing {
            Standing::Owner | Standing::Admin => Some(
                shares
                    .listed_grants(who, &id)
                    .await
                    .map_err(refused)?
                    .iter()
                    .map(|listed| grant_entry(listed).grantee)
                    .collect(),
            ),
            Standing::Reader | Standing::Stranger => None,
        };
        Ok(Reading {
            title: opened.share.title().as_str().to_owned(),
            owner: opened.share.owner().as_str().to_owned(),
            own: opened.standing == Standing::Owner,
            with,
            version: opened.version.number().get(),
            published: opened.version.created().as_secs(),
            markdown: text.as_str().to_owned(),
        })
    }

    async fn revoke(&self, who: &Principal, id: &str, now: Timestamp) -> Answer {
        let id = TokenId::try_from(id).map_err(|_| ApiCode::TokenNotFound)?;
        let owner = who.email().ok_or(ApiCode::NoVerifiedEmail)?;
        self.tokens
            .revoke(&id, owner, now)
            .await
            .map_err(token_error)?;
        Ok((204, None))
    }

    async fn checked(&self, token: &SecretString, now: Timestamp) -> TokenCheck {
        let Some((id, presented)) = split(token) else {
            return TokenCheck::NotOurs;
        };
        let stored = match self.tokens.get(&id).await {
            Ok(stored) => stored,
            Err(TokenStoreError::NotFound) => return TokenCheck::Refused(ApiCode::SignInNeeded),
            Err(_) => return TokenCheck::Refused(ApiCode::TokensUnavailable),
        };
        let owner = stored.owner();
        if !same(stored.digest(), &presented) || !stored.is_active(now) || !self.admit.admits(owner)
        {
            return TokenCheck::Refused(ApiCode::SignInNeeded);
        }
        let owner = owner.clone();
        TokenCheck::Principal(Principal::with_email(owner.principal_id(), owner))
    }

    async fn code(
        &self,
        owner: &Email,
        port: u16,
        now: Timestamp,
    ) -> Result<SecretString, ApiCode> {
        for _ in 0..DRAWS {
            let code = new_secret().ok_or(ApiCode::TokensUnavailable)?;
            let login = LoginCode::new(digest(&code), owner.clone(), port, now);
            match self.tokens.issue_code(login).await {
                Ok(()) => return Ok(code),
                Err(TokenStoreError::Taken) => {}
                Err(error) => return Err(token_error(error)),
            }
        }
        Err(ApiCode::TokensUnavailable)
    }

    async fn exchanged(&self, request: ExchangeRequest, now: Timestamp) -> Answer {
        let label = TokenLabel::try_from(request.label.as_str()).map_err(|error| Refusal {
            code: ApiCode::BadBody,
            detail: Some(error.to_string()),
            share: None,
        })?;
        let code = code_digest(&request.code).ok_or(ApiCode::CodeRefused)?;
        let used = self
            .tokens
            .use_code(&code, now)
            .await
            .map_err(token_error)?;
        for _ in 0..DRAWS {
            let id = TokenId::from_random(random_id().ok_or(ApiCode::TokensUnavailable)?);
            let secret = new_secret().ok_or(ApiCode::TokensUnavailable)?;
            let token = NewToken::new(
                id.clone(),
                digest(&secret),
                used.owner.clone(),
                label.clone(),
                now,
            );
            let expires = token.clone().stored().expires();
            match self.tokens.create(token).await {
                Ok(_) => {
                    let issued = TokenIssued {
                        id: id.as_str().to_owned(),
                        token: token_text(&id, &secret),
                        owner: used.owner.as_str().to_owned(),
                        label: label.as_str().to_owned(),
                        expires: utc(expires.as_secs()),
                    };
                    return json(201, &issued);
                }
                Err(TokenStoreError::Taken) => {}
                Err(error) => return Err(token_error(error).into()),
            }
        }
        Err(ApiCode::TokensUnavailable.into())
    }
}

impl<S, G, R, C, U, T, X, M> Api for Sharing<S, G, R, C, U, T, X, M>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    C: CommentStore,
    U: CursorStore,
    T: TokenStore,
    X: Recipients,
    M: Teams,
{
    fn admits(&self, who: &Principal) -> bool {
        who.email().is_some_and(|email| self.admit.admits(email))
    }

    fn authenticate<'a>(
        &'a self,
        token: &'a SecretString,
        now: Timestamp,
    ) -> Boxed<'a, TokenCheck> {
        Box::pin(self.checked(token, now))
    }

    fn issue_code<'a>(
        &'a self,
        owner: &'a Email,
        port: u16,
        now: Timestamp,
    ) -> Boxed<'a, Result<SecretString, ApiCode>> {
        Box::pin(self.code(owner, port, now))
    }

    fn exchange(&self, request: ExchangeRequest, now: Timestamp) -> Boxed<'_, Answer> {
        Box::pin(self.exchanged(request, now))
    }

    fn run<'a>(&'a self, who: &'a Principal, call: Call, now: Timestamp) -> Boxed<'a, Answer> {
        Box::pin(async move {
            match call {
                Call::Create(request) => self.create(who, request, now).await,
                Call::List {
                    scope,
                    after,
                    limit,
                    everyone,
                } => self.list(who, scope, after.as_ref(), limit, everyone).await,
                Call::Open(id, number) => self.open(who, &id, number).await,
                Call::Publish(id, request) => self.publish(who, &id, request, now).await,
                Call::Grant(id, request) => self.grant(who, &id, request, now).await,
                Call::ListGrants(id) => self.list_grants(who, &id).await,
                Call::RevokeGrants(id, request) => self.revoke_grants(who, &id, request).await,
                Call::CreateTeam(request) => self.create_team(who, &request, now).await,
                Call::ListTeams => self.list_teams(who).await,
                Call::ShowTeam(name) => self.show_team(who, &name).await,
                Call::DeleteTeam(name) => self.delete_team(who, &name).await,
                Call::TransferTeam(name, request) => self.transfer_team(who, &name, &request).await,
                Call::AddMembers(name, request) => {
                    self.change_members(who, &name, &request, true).await
                }
                Call::RemoveMembers(name, request) => {
                    self.change_members(who, &name, &request, false).await
                }
                Call::Activity {
                    flares,
                    since,
                    since_last,
                    limit,
                } => {
                    self.activity(
                        who,
                        &flares,
                        (since.as_deref(), since_last.as_deref()),
                        limit,
                    )
                    .await
                }
                Call::Comments(flare) => self.thread(who, &flare).await,
                Call::Post(request) => self.post(who, request, now).await,
                Call::Edit(id, request) => self.edit(who, &id, request, now).await,
                Call::Delete(id) => self.delete(who, &id, now).await,
                Call::Cursor(name) => self.cursor(who, &name).await,
                Call::SaveCursor(name, request) => self.save_cursor(who, &name, request).await,
                Call::Revoke(id) => self.revoke(who, &id, now).await,
            }
        })
    }

    fn read<'a>(
        &'a self,
        who: &'a Principal,
        id: &'a str,
        version: Option<&'a str>,
    ) -> Boxed<'a, Result<Reading, ReadRefusal>> {
        Box::pin(self.reading(who, id, version))
    }
}
