//! `flare open`: finds the running server through `server.json`, checks
//! that it holds the token, asks it for a launch code and opens the opener
//! file it writes.
//!
//! The client sends the token only after the server's probe answer proves
//! that the server holds it, so a stale `server.json` whose port another
//! program now holds never receives the token. It connects only to
//! 127.0.0.1, with no proxy, and follows no redirect. This is one of the
//! modules that reads the token (BASE-24), to put it in the bearer header.

use crate::board::{self, Need};
use crate::cli::OpenArgs;
use crate::codes::Failed;
use crate::documents::Opened;
use crate::env::Snapshot;
use crate::opener;
use flare_auth_loopback::{Nonce, ServerRecord, StateError};
use flare_core::PageName;
use reqwest::{Client, StatusCode};
use secrecy::ExposeSecret as _;
use serde::Deserialize;
use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::time::Duration;

/// How long each request to the server may take.
const DEADLINE: Duration = Duration::from_secs(10);

/// The answer of `POST /launch/codes`.
#[derive(Deserialize)]
struct Codes {
    opener: PathBuf,
}

/// A client of one running server.
struct Server {
    client: Client,
    origin: String,
    record: ServerRecord,
}

fn not_running(reason: impl Into<String>) -> Failed {
    Failed::NotRunning {
        reason: reason.into(),
    }
}

impl Server {
    /// A client of the server that `record` names.
    fn new(record: ServerRecord) -> Result<Self, Failed> {
        let host = record.launch_host();
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, host.port()));
        let client = Client::builder()
            .no_proxy()
            .resolve(host.name(), address)
            .redirect(reqwest::redirect::Policy::none())
            .timeout(DEADLINE)
            .build()
            .map_err(|_| not_running("the HTTP client could not be built"))?;
        Ok(Self {
            client,
            origin: host.origin(),
            record,
        })
    }

    /// Sends the probe and checks the proof. The probe carries no token.
    async fn prove(&self) -> Result<(), Failed> {
        let nonce =
            Nonce::new().map_err(|_| not_running("the operating system gave no random bytes"))?;
        let url = format!("{}/launch/probe?nonce={}", self.origin, nonce.as_str());
        let reply = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| not_running("nothing answers on the port that server.json names"))?;
        let status = reply.status();
        let proof = reply
            .text()
            .await
            .map_err(|_| not_running("the probe's answer could not be read"))?;
        if status != StatusCode::OK || !self.record.proof_matches(&nonce, proof.trim()) {
            return Err(not_running(
                "the program on the port that server.json names did not prove that it holds the token",
            ));
        }
        Ok(())
    }

    /// The status of `GET path` with the bearer token.
    async fn status(&self, path: &str) -> Result<StatusCode, Failed> {
        self.client
            .get(format!("{}{path}", self.origin))
            .bearer_auth(self.record.token().expose_secret())
            .header("accept", "application/json")
            .send()
            .await
            .map(|reply| reply.status())
            .map_err(|_| not_running("the server stopped answering"))
    }

    /// The path that `name` opens: its binding, else its page.
    async fn target(&self, name: Option<&str>) -> Result<String, Failed> {
        let Some(name) = name else {
            return Ok("/flare/".to_owned());
        };
        let not_found = || Failed::PageNotFound {
            name: name.to_owned(),
        };
        let name = PageName::try_from(name).map_err(|_| not_found())?;
        for path in [format!("/b/{name}"), format!("/pages/{name}")] {
            match self.status(&path).await? {
                StatusCode::OK => return Ok(path),
                StatusCode::NOT_FOUND => {}
                other => {
                    return Err(Failed::BoardUnavailable {
                        problem: format!("the server answered {other} for {path}"),
                    });
                }
            }
        }
        Err(not_found())
    }

    /// Asks for a launch code for `target` and returns the opener file.
    async fn opener(&self, target: &str) -> Result<PathBuf, Failed> {
        let reply = self
            .client
            // `target` is `/flare/` or a path built from a checked page
            // name, so it needs no escaping in the query.
            .post(format!("{}/launch/codes?return_to={target}", self.origin))
            .bearer_auth(self.record.token().expose_secret())
            .send()
            .await
            .map_err(|_| not_running("the server stopped answering"))?;
        if reply.status() != StatusCode::OK {
            return Err(Failed::OpenerFailed {
                problem: format!("the server answered {} for a launch code", reply.status()),
            });
        }
        let body = reply
            .bytes()
            .await
            .map_err(|_| not_running("the server stopped answering"))?;
        let codes: Codes = serde_json::from_slice(&body).map_err(|_| Failed::OpenerFailed {
            problem: "the server's answer names no opener file".to_owned(),
        })?;
        Ok(codes.opener)
    }
}

/// The server record of the board at `root`.
fn record(root: &std::path::Path, env: &Snapshot) -> Result<ServerRecord, Failed> {
    let state = board::loopback_state(root, env)?;
    match state.read_server() {
        Ok(Some(record)) => Ok(record),
        Ok(None) => Err(not_running("there is no server.json for this board")),
        Err(StateError::Malformed { .. }) => {
            Err(not_running("server.json is not a flare server file"))
        }
        Err(error) => Err(Failed::StateUnusable {
            problem: error.to_string(),
        }),
    }
}

/// Runs `flare open`.
///
/// # Errors
///
/// Returns the failure that stopped it.
pub(crate) async fn run(args: &OpenArgs, env: &Snapshot) -> Result<Opened, Failed> {
    let root = board::root(args.board.as_deref(), Need::Directory, env)?;
    let server = Server::new(record(&root, env)?)?;
    server.prove().await?;
    let target = server.target(args.name.as_deref()).await?;
    let opener = server.opener(&target).await?;
    opener::open(&opener, env)?;
    Ok(Opened { target })
}

/// Whether a server that answers the probe already serves the board at
/// `root`, for `flare serve`.
pub(crate) async fn answers(root: &std::path::Path, env: &Snapshot) -> bool {
    let Ok(record) = record(root, env) else {
        return false;
    };
    match Server::new(record) {
        Ok(server) => server.prove().await.is_ok(),
        Err(_) => false,
    }
}
