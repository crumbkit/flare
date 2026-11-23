//! `flare login` and `flare logout`.
//!
//! `flare login <server>` listens on a loopback port, opens the browser at
//! the server's `/cli/login`, takes the login code that the signed-in
//! browser brings back, exchanges it for a token, saves the token in the
//! Keychain and saves the login. The label is the saved login's label for
//! that server, else a new `flare-cli-<8 hex>`, so a second login on this
//! computer makes the server revoke the first login's token. `flare
//! logout` revokes the token on the server and forgets it here.

mod callback;
pub(crate) mod saved;
#[cfg(test)]
mod tests;

use crate::cli::LoginArgs;
use crate::client::Api;
use crate::codes::{Failed, SharingFailed};
use crate::documents::{LoggedIn, LoggedOut};
use crate::env::Snapshot;
use crate::{keychain, opener};
use callback::Callback;
use flare_auth_loopback::Nonce;
use flare_contract::{ApiCode, ExchangeRequest, TokenIssued};
use reqwest::{Method, Url};
use saved::Saved;
use secrecy::{ExposeSecret as _, SecretString};
use std::time::Duration;

/// How long `flare login` waits for the browser to come back.
const LOGIN_WAIT: Duration = Duration::from_secs(300);

/// The origin of `server`: an `https` URL, or an `http` URL on a loopback
/// host, with no path, query or user.
///
/// # Errors
///
/// Returns [`SharingFailed::InvalidServer`].
pub(crate) fn origin(server: &str) -> Result<String, Failed> {
    let invalid = |problem: &str| -> Failed {
        SharingFailed::InvalidServer {
            server: server.to_owned(),
            problem: problem.to_owned(),
        }
        .into()
    };
    let url = Url::parse(server).map_err(|_| invalid("it is not a URL"))?;
    let loopback = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    match url.scheme() {
        "https" => {}
        "http" if loopback => {}
        _ => return Err(invalid("flare sends its token only over https")),
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(invalid("it holds a user name or password"));
    }
    if url.path() != "/" || url.query().is_some() || url.fragment().is_some() {
        return Err(invalid("it holds a path, a query or a fragment"));
    }
    Ok(url.origin().ascii_serialization())
}

fn random_hex(env_problem: &str) -> Result<String, Failed> {
    Nonce::new()
        .map(|nonce| nonce.as_str().to_owned())
        .map_err(|_| Failed::System {
            problem: format!("the operating system gave no random bytes for {env_problem}"),
        })
}

/// Runs `flare login`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn login(args: &LoginArgs, env: &Snapshot) -> Result<LoggedIn, Failed> {
    let origin = origin(&args.server)?;
    let earlier = saved::load(env)?.filter(|saved| saved.server == origin);
    let label = match (&args.label, earlier) {
        (Some(label), _) => label.clone(),
        (None, Some(earlier)) => earlier.label,
        (None, None) => format!(
            "flare-cli-{}",
            random_hex("the label")?.get(..8).unwrap_or("0")
        ),
    };
    let state = random_hex("the login state")?;
    let callback = Callback::bind().await?;
    let url = format!("{origin}/cli/login?port={}&state={state}", callback.port());
    let mut opening = {
        let env = env.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "login joins the opener's thread on every path before it returns, and the opener's own deadline bounds it (SVC-1)"
        )]
        tokio::task::spawn_blocking(move || opener::open(&url, &env))
    };
    // An opener that fails ends the login at once; one that succeeds
    // leaves the wait for the browser running.
    let waiting = callback.code(&state, LOGIN_WAIT);
    tokio::pin!(waiting);
    let (code, opened) = tokio::select! {
        code = &mut waiting => (code, None),
        opened = &mut opening => {
            joined(opened)?;
            (waiting.await, Some(()))
        }
    };
    if opened.is_none() {
        // The opener has a deadline of its own, so this join ends.
        let opened = opening.await;
        let code = code?;
        joined(opened)?;
        return finish(env, &origin, &code, &label).await;
    }
    finish(env, &origin, &code?, &label).await
}

/// The opener's result from its thread.
fn joined(opened: Result<Result<(), Failed>, tokio::task::JoinError>) -> Result<(), Failed> {
    opened.map_err(|error| Failed::System {
        problem: format!("the browser opener's thread failed: {error}"),
    })?
}

/// Exchanges the code, saves the token and the login, and reports it.
async fn finish(
    env: &Snapshot,
    origin: &str,
    code: &SecretString,
    label: &str,
) -> Result<LoggedIn, Failed> {
    let origin = origin.to_owned();
    let issued = exchange(&origin, code, label).await?;
    let token = SecretString::from(issued.token);
    keychain::save(env, &origin, &token)?;
    saved::store(
        env,
        &Saved {
            server: origin.clone(),
            token_id: issued.id,
            label: label.to_owned(),
            owner: issued.owner.clone(),
        },
    )?;
    Ok(LoggedIn {
        server: origin,
        owner: issued.owner,
        label: label.to_owned(),
        expires: issued.expires,
    })
}

/// Trades the login code for a token.
async fn exchange(origin: &str, code: &SecretString, label: &str) -> Result<TokenIssued, Failed> {
    let api = Api::new(origin, None)?;
    let request = ExchangeRequest {
        code: code.expose_secret().to_owned(),
        label: label.to_owned(),
    };
    api.send(Method::POST, "/api/tokens/exchange", Some(&request))
        .await?
        .ok_or_else(|| {
            SharingFailed::BadAnswer {
                server: origin.to_owned(),
                problem: "the exchange answered with no token".to_owned(),
            }
            .into()
        })
}

/// Runs `flare logout`: revokes the token on the server, then deletes it
/// from the Keychain and forgets the login. A token that the server no
/// longer knows counts as revoked.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn logout(env: &Snapshot) -> Result<LoggedOut, Failed> {
    let saved = saved::required(env)?;
    if let Some(token) = keychain::read(env, &saved.server)? {
        let api = Api::new(&saved.server, Some(token))?;
        let target = format!("/api/tokens/{}", saved.token_id);
        match api
            .send::<serde_json::Value>(Method::DELETE, &target, None::<&()>)
            .await
        {
            Ok(_) => {}
            Err(Failed::Refused(refused))
                if matches!(
                    refused.api_code(),
                    ApiCode::SignInNeeded | ApiCode::TokenNotFound
                ) => {}
            Err(other) => return Err(other),
        }
        keychain::delete(env, &saved.server)?;
    }
    saved::forget(env)?;
    Ok(LoggedOut {
        server: saved.server,
        label: saved.label,
    })
}
