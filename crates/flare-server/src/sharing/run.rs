//! The generic implementation of [`Api`] over flare-core's stores.

use super::convert::{
    content_error, flare_error, item, recipient_error, role, share_error, standing, summary,
};
use super::tokens::{code_digest, digest, new_secret, random_id, same, split, token_text};
use super::{Answer, Api, Boxed, Call, Refusal, Scope, Sharing, TokenCheck};
use flare_contract::{
    ActivityPage, ApiCode, Cursor, ExchangeRequest, Format, OpenedShare, Role as WireRole,
    ShareList, ShareRequest, Shared, TokenIssued, VersionRequest, utc,
};
use flare_core::{
    DocumentText, Email, FlareError, Flared, GrantStore, LoginCode, NewFlare, NewToken, Position,
    Principal, Recipients, ResponseStore, ShareContent, ShareError, ShareId, ShareStore,
    ShareTitle, Timestamp, TokenId, TokenLabel, TokenStore, TokenStoreError,
};
use secrecy::SecretString;
use serde::Serialize;

/// How many times a create draws a new id after `IdTaken`.
const DRAWS: usize = 3;

/// A JSON answer with `status`.
fn json(status: u16, body: &impl Serialize) -> Answer {
    serde_json::to_string(body)
        .map(|text| (status, Some(text)))
        .map_err(|_| ApiCode::SharesUnavailable.into())
}

/// The share id that `text` names; a malformed id is a missing share.
fn share_id(text: &str) -> Result<ShareId, Refusal> {
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
fn shared(flared: &Flared, role: WireRole) -> Shared {
    Shared {
        share: summary(&flared.share, role),
        granted: flared
            .granted
            .iter()
            .map(|email| email.as_str().to_owned())
            .collect(),
        responds_to: flared
            .responds_to
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
    }
}

impl<S, G, R, T, X> Sharing<S, G, R, T, X>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
{
    /// The address each identifier names.
    async fn recipients(&self, with: &[String]) -> Result<Vec<Email>, Refusal> {
        let mut resolved = Vec::with_capacity(with.len());
        for identifier in with {
            let email = self
                .recipients
                .resolve(identifier)
                .await
                .map_err(|error| recipient_error(error, identifier))?;
            resolved.push(email);
        }
        Ok(resolved)
    }

    async fn create(&self, who: &Principal, request: ShareRequest, now: Timestamp) -> Answer {
        if request.format == Format::Html {
            return Err(ApiCode::KindNotServed.into());
        }
        let title = ShareTitle::try_from(request.title.as_str()).map_err(|e| content_error(&e))?;
        let text =
            DocumentText::try_from(request.content.as_str()).map_err(|e| content_error(&e))?;
        let with = self.recipients(&request.with).await?;
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
            );
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
        if request.format == Format::Html {
            return Err(ApiCode::KindNotServed.into());
        }
        let text =
            DocumentText::try_from(request.content.as_str()).map_err(|e| content_error(&e))?;
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

    async fn list(&self, who: &Principal, scope: Scope, limit: usize) -> Answer {
        let shares = self.flares.shares();
        let (mut items, truncated) = match scope {
            Scope::Mine => {
                let listed = shares.mine(who).await.map_err(share_error)?;
                let truncated = listed.truncated();
                let items = listed
                    .items()
                    .iter()
                    .map(|share| summary(share, WireRole::Owner))
                    .collect::<Vec<_>>();
                (items, truncated)
            }
            Scope::WithMe => {
                let listed = shares.shared_with_me(who).await.map_err(share_error)?;
                let truncated = listed.truncated();
                let items = listed
                    .items()
                    .iter()
                    .map(|entry| summary(&entry.share, role(entry.role)))
                    .collect::<Vec<_>>();
                (items, truncated)
            }
        };
        let more = truncated || items.len() > limit;
        items.truncate(limit);
        json(200, &ShareList { items, more })
    }

    async fn open(&self, who: &Principal, id: &str) -> Answer {
        let id = share_id(id)?;
        let opened = self
            .flares
            .shares()
            .open(who, &id, None)
            .await
            .map_err(share_error)?;
        let content = match opened.version.content() {
            ShareContent::Document(text) => text.as_str().to_owned(),
            _ => return Err(ApiCode::KindNotServed.into()),
        };
        let share = summary(&opened.share, standing(opened.standing));
        json(200, &OpenedShare { share, content })
    }

    async fn grant(
        &self,
        who: &Principal,
        id: &str,
        request: flare_contract::GrantRequest,
        now: Timestamp,
    ) -> Answer {
        let id = share_id(id)?;
        let with = self.recipients(&request.with).await?;
        let re = request
            .re
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>()?;
        let flared = self
            .flares
            .grant(who, &id, &with, &re, now)
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

    async fn activity(
        &self,
        who: &Principal,
        flares: &[String],
        since: Option<&str>,
        limit: usize,
    ) -> Answer {
        let after = match since {
            Some(text) => Cursor::try_from(text)
                .map_err(|_| ApiCode::BadCursor)?
                .position(),
            None => 0,
        };
        let ids = flares
            .iter()
            .map(|text| share_id(text))
            .collect::<Result<Vec<_>, _>>();
        if flares.len() > flare_core::MAX_FLARES {
            return Err(ApiCode::TooManyFlares.into());
        }
        let page = self
            .flares
            .activity(who, &ids?, Position::new(after), limit)
            .await
            .map_err(flare_error)?;
        json(
            200,
            &ActivityPage {
                items: page.items.iter().map(item).collect(),
                cursor: Cursor::new(page.last.get()).to_string(),
                more: page.more,
            },
        )
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

impl<S, G, R, T, X> Api for Sharing<S, G, R, T, X>
where
    S: ShareStore,
    G: GrantStore,
    R: ResponseStore,
    T: TokenStore,
    X: Recipients,
{
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
                Call::List { scope, limit } => self.list(who, scope, limit).await,
                Call::Open(id) => self.open(who, &id).await,
                Call::Publish(id, request) => self.publish(who, &id, request, now).await,
                Call::Grant(id, request) => self.grant(who, &id, request, now).await,
                Call::Activity {
                    flares,
                    since,
                    limit,
                } => self.activity(who, &flares, since.as_deref(), limit).await,
                Call::Revoke(id) => self.revoke(who, &id, now).await,
            }
        })
    }
}
