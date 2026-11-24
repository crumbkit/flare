//! The loopback listener that takes the login code from the browser.
//!
//! `flare login` listens on a free port of 127.0.0.1 and sends the browser
//! to the server's `/cli/login` with that port and a random state. The
//! server sends the signed-in browser back to
//! `http://127.0.0.1:<port>/callback?code=<code>&state=<state>`. The
//! listener answers every other path with 404 and keeps waiting, because a
//! browser may ask for `/favicon.ico`. A callback whose state differs is
//! refused at once, because only the server that this login opened knows
//! the state.
//!
//! The listener reads only the request line of each request, at most
//! 8 KiB, and answers with a fixed plain-text page. It does less than an
//! HTTP server: it keeps no connection open and reads no header or body,
//! which a browser's GET of the callback does not need. A request that
//! needs more is answered 400.

use crate::codes::{Failed, SharingFailed};
use secrecy::SecretString;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::{TcpListener, TcpStream};
use tokio::time::{Instant, timeout_at};

/// The longest request line the listener reads.
const MAX_REQUEST: usize = 8 * 1024;

/// The length of a login code: 32 bytes in base64url.
const CODE_CHARS: usize = 43;

/// The page the browser shows after a callback.
const SIGNED_IN: &str = "flare is signed in. You can close this tab.\n";

fn failed(server: &str, problem: impl Into<String>) -> Failed {
    SharingFailed::LoginFailed {
        server: server.to_owned(),
        problem: problem.into(),
    }
    .into()
}

/// The listener, bound before the browser opens.
pub(crate) struct Callback {
    listener: TcpListener,
    port: u16,
}

/// What one request asked for.
enum Asked {
    /// The callback with its query.
    Callback(String),
    /// Another path, or a request the listener cannot read.
    Other,
}

impl Callback {
    /// Binds a free port of 127.0.0.1.
    ///
    /// # Errors
    ///
    /// Returns [`Failed::System`] when no port can be bound.
    pub(crate) async fn bind() -> Result<Self, Failed> {
        let listener =
            TcpListener::bind(("127.0.0.1", 0))
                .await
                .map_err(|error| Failed::System {
                    problem: format!("no loopback port for the login: {error}"),
                })?;
        let port = listener
            .local_addr()
            .map_err(|error| Failed::System {
                problem: error.to_string(),
            })?
            .port();
        Ok(Self { listener, port })
    }

    /// The port the server sends the browser back to.
    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// Waits up to `wait` for the callback with `state` from the sign-in to
    /// `server` and answers the login code.
    ///
    /// # Errors
    ///
    /// Returns [`SharingFailed::LoginFailed`] when the wait ends, the state
    /// differs, or the callback holds no code.
    pub(crate) async fn code(
        &self,
        server: &str,
        state: &str,
        wait: Duration,
    ) -> Result<SecretString, Failed> {
        let deadline = Instant::now() + wait;
        loop {
            let accepted = timeout_at(deadline, self.listener.accept())
                .await
                .map_err(|_| {
                    failed(
                        server,
                        format!("no browser came back within {} seconds", wait.as_secs()),
                    )
                })?;
            let Ok((mut stream, _)) = accepted else {
                continue;
            };
            let Ok(asked) = timeout_at(deadline, read(&mut stream)).await else {
                continue;
            };
            let Asked::Callback(query) = asked else {
                answer(&mut stream, 404, "Not found.\n").await;
                continue;
            };
            let pairs: Vec<(String, String)> = url_pairs(&query);
            let value = |name: &str| {
                pairs
                    .iter()
                    .find(|(key, _)| key == name)
                    .map(|(_, value)| value.as_str())
            };
            if value("state") != Some(state) {
                answer(
                    &mut stream,
                    400,
                    "This sign-in is not the one flare started.\n",
                )
                .await;
                return Err(failed(
                    server,
                    "the browser came back with another login's state",
                ));
            }
            let code = value("code").filter(|code| {
                code.len() == CODE_CHARS
                    && code
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-' || byte == b'_')
            });
            let Some(code) = code else {
                answer(&mut stream, 400, "The sign-in gave flare no login code.\n").await;
                return Err(failed(server, "the browser came back with no login code"));
            };
            let code = SecretString::from(code.to_owned());
            answer(&mut stream, 200, SIGNED_IN).await;
            return Ok(code);
        }
    }
}

/// The query's pairs. The callback's values are base64url and hex, so
/// they need no percent-decoding; a value that holds `%` stays as it is and
/// fails the checks that follow.
fn url_pairs(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(key, value)| (key.to_owned(), value.to_owned()))
        .collect()
}

/// Reads the request line.
async fn read(stream: &mut TcpStream) -> Asked {
    let mut buffer = Vec::with_capacity(1024);
    let mut chunk = [0_u8; 1024];
    while !buffer.windows(2).any(|window| window == b"\r\n") {
        match stream.read(&mut chunk).await {
            Ok(0) | Err(_) => return Asked::Other,
            Ok(read) => buffer.extend(chunk.iter().take(read)),
        }
        if buffer.len() > MAX_REQUEST {
            return Asked::Other;
        }
    }
    let line = String::from_utf8_lossy(&buffer);
    let mut words = line.split_whitespace();
    let (Some("GET"), Some(target)) = (words.next(), words.next()) else {
        return Asked::Other;
    };
    match target.split_once('?') {
        Some(("/callback", query)) => Asked::Callback(query.to_owned()),
        _ => Asked::Other,
    }
}

/// Answers with `status` and the plain-text `body`, and closes.
async fn answer(stream: &mut TcpStream, status: u16, body: &str) {
    let reason = match status {
        200 => "OK",
        404 => "Not Found",
        _ => "Bad Request",
    };
    let reply = format!(
        "HTTP/1.1 {status} {reason}\r\ncontent-type: text/plain; charset=utf-8\r\ncontent-length: {}\r\ncache-control: no-store\r\nconnection: close\r\n\r\n{body}",
        body.len()
    );
    let _ = stream.write_all(reply.as_bytes()).await;
    let _ = stream.shutdown().await;
}
