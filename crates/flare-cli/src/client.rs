//! The client of a flare server's share API. It is one of the modules that
//! read the token (BASE-24), to put it in the bearer header.
//!
//! The client follows no redirect, so the token never goes to another
//! origin. A refusal's problem document becomes [`Refused`] with the
//! server's code, and the server's own busy, fault and deadline answers
//! become [`SharingFailed::ServerError`]; any other answer that is not the
//! expected JSON is [`SharingFailed::BadAnswer`]. A share sent with its
//! images goes as a `multipart/form-data` body that [`form`] writes.

use crate::codes::{Failed, NextRun, Refused, SharingFailed};
use crate::sharing::LocalPart;
use flare_contract::{ApiCode, ProblemDocument};
use flare_server::{SERVER_BUSY, SERVER_DEADLINE, SERVER_FAULT};
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
    /// The token command that gave `token`, when the config names one.
    helper: Option<String>,
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
            helper: None,
        })
    }

    /// The same client, whose token came from the token command `helper`.
    pub(crate) fn with_helper(mut self, helper: &str) -> Self {
        self.helper = Some(helper.to_owned());
        self
    }

    /// The server's origin.
    pub(crate) fn origin(&self) -> &str {
        &self.origin
    }

    /// A bad answer with `status`, which flare could not read as the share
    /// API's answer. After a success status, or a 502, 503 or 504 that a
    /// proxy in front of the server may send after the server did the
    /// work, the server may have done what the request asked, which the
    /// command that sent it weighs with [`Failed::after_a_write`]. Any other
    /// status says the request did not succeed.
    fn bad(&self, status: StatusCode, problem: impl Into<String>) -> Failed {
        let maybe_done = status.is_success()
            || matches!(
                status,
                StatusCode::BAD_GATEWAY
                    | StatusCode::SERVICE_UNAVAILABLE
                    | StatusCode::GATEWAY_TIMEOUT
            );
        SharingFailed::BadAnswer {
            server: self.origin.clone(),
            problem: problem.into(),
            next: if maybe_done {
                NextRun::MaybeDone
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
    /// Returns [`Refused`] for a problem document of the share API,
    /// [`SharingFailed::ServerError`] for the server's own failure,
    /// [`SharingFailed::Unreachable`] when the server does not answer, and
    /// [`SharingFailed::BadAnswer`] for any other answer.
    pub(crate) async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        target: &str,
        body: Option<&impl Serialize>,
    ) -> Result<Option<T>, Failed> {
        let mut request = self.request(method, target);
        if let Some(body) = body {
            request = request
                .header("content-type", "application/json")
                .body(json(body)?);
        }
        self.answer(request).await
    }

    /// Sends `method` to `target` as a `multipart/form-data` body with one
    /// `share` part that holds `share` as JSON and one `image` part per
    /// image, named by its path, and reads the JSON answer.
    ///
    /// # Errors
    ///
    /// Returns the failures of [`Api::send`].
    pub(crate) async fn send_form<T: DeserializeOwned>(
        &self,
        method: Method,
        target: &str,
        share: &impl Serialize,
        images: &[LocalPart],
    ) -> Result<Option<T>, Failed> {
        let share = json(share)?;
        let boundary = boundary(&share, images);
        let body = form(&boundary, &share, images);
        let request = self
            .request(method, target)
            .header(
                "content-type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .body(body);
        self.answer(request).await
    }

    /// A request of `method` to `target` with the bearer token.
    fn request(&self, method: Method, target: &str) -> reqwest::RequestBuilder {
        let request = self
            .client
            .request(method, format!("{}{target}", self.origin))
            .header("accept", "application/json");
        match &self.token {
            Some(token) => request.bearer_auth(token.expose_secret()),
            None => request,
        }
    }

    /// Sends `request` and reads its JSON answer.
    async fn answer<T: DeserializeOwned>(
        &self,
        request: reqwest::RequestBuilder,
    ) -> Result<Option<T>, Failed> {
        let reply = request.send().await.map_err(|error| {
            Failed::from(SharingFailed::Unreachable {
                server: self.origin.clone(),
                problem: without_url(&error),
                // A refused connection sent nothing; any other failure may
                // follow a request the server received.
                next: if error.is_connect() {
                    NextRun::Clean
                } else {
                    NextRun::MaybeDone
                },
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
        if let Some((answer, next)) = server_answer(&problem.code) {
            return Err(SharingFailed::ServerError {
                server: self.origin.clone(),
                answer,
                title: problem.title,
                next,
            }
            .into());
        }
        match ApiCode::from_text(&problem.code) {
            Some(code) => Err(Refused::new(code, problem.detail, &self.origin)
                .naming(problem.share.as_deref())
                .with_errors(problem.errors)
                .with_helper(self.helper.as_deref())
                .into()),
            None => Err(self.bad(StatusCode::BAD_REQUEST, format!(
                "it answered {status} with the code {}, which this flare does not know; a newer flare may",
                problem.code
            ))),
        }
    }
}

/// `body` as JSON bytes.
fn json(body: &impl Serialize) -> Result<Vec<u8>, Failed> {
    serde_json::to_vec(body).map_err(|error| Failed::System {
        problem: format!("the request could not be written: {error}"),
    })
}

/// A form boundary that appears in no part: `flare-form-` and the first
/// number that gives one.
pub(crate) fn boundary(share: &[u8], images: &[LocalPart]) -> String {
    let appears = |boundary: &str| {
        let needle = boundary.as_bytes();
        std::iter::once(share)
            .chain(images.iter().map(|image| image.bytes.as_slice()))
            .any(|part| part.windows(needle.len()).any(|window| window == needle))
    };
    (0_u64..)
        .map(|number| format!("flare-form-{number:016x}"))
        .find(|boundary| !appears(boundary))
        .unwrap_or_default()
}

/// The form body of `share` and `images`, each as the part its kind names, with `boundary`. A filename
/// escapes a quote and a backslash with a backslash.
pub(crate) fn form(boundary: &str, share: &[u8], images: &[LocalPart]) -> Vec<u8> {
    let mut body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"share\"\r\nContent-Type: application/json\r\n\r\n"
    )
    .into_bytes();
    body.extend_from_slice(share);
    for image in images {
        let name = image.path.replace('\\', "\\\\").replace('"', "\\\"");
        body.extend_from_slice(
            format!(
                "\r\n--{boundary}\r\nContent-Disposition: form-data; name=\"{}\"; filename=\"{name}\"\r\nContent-Type: {}\r\n\r\n",
                image.part.name(),
                image.part.content_type(),
            )
            .as_bytes(),
        );
        body.extend_from_slice(&image.bytes);
    }
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

/// The server's own failure that `code` names, with what it means for
/// running the command again, or `None` for any other code. A busy server
/// answers before the handler runs, so nothing was done. A fault or a
/// passed deadline may follow a finished write.
fn server_answer(code: &str) -> Option<(&'static str, NextRun)> {
    [
        (SERVER_BUSY, NextRun::Clean),
        (SERVER_DEADLINE, NextRun::MaybeDone),
        (SERVER_FAULT, NextRun::MaybeDone),
    ]
    .into_iter()
    .find(|(text, _)| *text == code)
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

#[cfg(test)]
mod tests;
