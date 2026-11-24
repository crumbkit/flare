//! The client of a flare server's share API. It is one of the modules that
//! read the token (BASE-24), to put it in the bearer header.
//!
//! The client follows no redirect, so the token never goes to another
//! origin. A refusal's problem document becomes [`Refused`] with the
//! server's code; any other answer that is not the expected JSON is
//! [`SharingFailed::BadAnswer`].

use crate::codes::{Failed, NextRun, Refused, SharingFailed};
use flare_contract::{ApiCode, ProblemDocument};
use reqwest::{Client, Method, StatusCode};
use secrecy::{ExposeSecret as _, SecretString};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::time::Duration;

/// How long one request may take.
const DEADLINE: Duration = Duration::from_secs(30);

/// A client of one server's share API.
pub(crate) struct Api {
    client: Client,
    origin: String,
    token: Option<SecretString>,
}

impl Api {
    /// A client of `origin` that sends `token` as its bearer, when given.
    ///
    /// # Errors
    ///
    /// Returns [`Failed::System`] when the HTTP client cannot be built.
    pub(crate) fn new(origin: &str, token: Option<SecretString>) -> Result<Self, Failed> {
        let client = Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(DEADLINE)
            .build()
            .map_err(|error| Failed::System {
                problem: format!("the HTTP client could not be built: {error}"),
            })?;
        Ok(Self {
            client,
            origin: origin.to_owned(),
            token,
        })
    }

    /// The server's origin.
    pub(crate) fn origin(&self) -> &str {
        &self.origin
    }

    /// A bad answer with `status`. After a success status the server may
    /// have done what the request asked, which the command that sent it
    /// weighs with [`Failed::after_a_write`].
    fn bad(&self, status: StatusCode, problem: impl Into<String>) -> Failed {
        SharingFailed::BadAnswer {
            server: self.origin.clone(),
            problem: problem.into(),
            next: if status.is_success() {
                NextRun::AfterSuccess
            } else {
                NextRun::Clean
            },
        }
        .into()
    }

    /// Sends `method` to `target`, a path with its query, with `body` as
    /// JSON, and reads the JSON answer. An answer with no body is `None`.
    ///
    /// # Errors
    ///
    /// Returns [`Refused`] for a problem document,
    /// [`SharingFailed::Unreachable`] when the server does not answer, and
    /// [`SharingFailed::BadAnswer`] for any other answer.
    pub(crate) async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        target: &str,
        body: Option<&impl Serialize>,
    ) -> Result<Option<T>, Failed> {
        let mut request = self
            .client
            .request(method, format!("{}{target}", self.origin))
            .header("accept", "application/json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token.expose_secret());
        }
        if let Some(body) = body {
            let bytes = serde_json::to_vec(body).map_err(|error| Failed::System {
                problem: format!("the request could not be written: {error}"),
            })?;
            request = request
                .header("content-type", "application/json")
                .body(bytes);
        }
        let reply = request.send().await.map_err(|error| {
            Failed::from(SharingFailed::Unreachable {
                server: self.origin.clone(),
                problem: without_url(&error),
            })
        })?;
        let status = reply.status();
        let bytes = reply
            .bytes()
            .await
            .map_err(|_| self.bad(status, "the answer was cut short"))?;
        if status.is_success() {
            if bytes.is_empty() || status == StatusCode::NO_CONTENT {
                return Ok(None);
            }
            return serde_json::from_slice(&bytes).map(Some).map_err(|error| {
                self.bad(
                    status,
                    format!("the answer is not the expected JSON: {error}"),
                )
            });
        }
        let problem: ProblemDocument = serde_json::from_slice(&bytes).map_err(|_| {
            self.bad(
                status,
                format!("it answered {status} with no problem document"),
            )
        })?;
        match ApiCode::from_text(&problem.code) {
            Some(code) => Err(Refused::new(code, problem.detail, &self.origin)
                .naming(problem.share.as_deref())
                .into()),
            None => Err(self.bad(status, format!(
                "it answered {status} with the code {}, which this flare does not know; a newer flare may",
                problem.code
            ))),
        }
    }
}

/// What went wrong with a request, without its URL, whose query can hold a
/// cursor or a flare's id.
fn without_url(error: &reqwest::Error) -> String {
    if error.is_timeout() {
        format!("no answer within {} seconds", DEADLINE.as_secs())
    } else if error.is_connect() {
        "the connection was refused or could not be made".to_owned()
    } else {
        "the request failed".to_owned()
    }
}
