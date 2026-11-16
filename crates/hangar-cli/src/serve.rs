//! `hangar serve`: checks the board program, the config and the Keychain
//! secret before it binds, then serves the board until Ctrl-C, SIGTERM or
//! the idle exit.
//!
//! It prints one line that names the board and says to run `hangar open`,
//! and never prints the launch host, a launch URL or the token.

use crate::board;
use crate::cli::ServeArgs;
use crate::codes::Failed;
use crate::config::{self, KEYCHAIN_SERVICE};
use crate::documents::Served;
use crate::env::Snapshot;
use crate::{log, open};
use hangar_auth_loopback::{Loopback, Served as Reach, local_principal};
use hangar_auth_oidc::{ConnectError, KeychainSecret, OidcAuthenticator, OidcConfig};
use hangar_server::{App, BindError, Gate, Limits, Listeners, ServeError, Stopped};
use std::sync::OnceLock;
use tokio::signal::unix::{SignalKind, signal};

/// The sign-in host's port, which the provider's redirect URI names.
const SIGN_IN_PORT: u16 = 8740;

/// Why the server stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Stop {
    /// SIGTERM.
    Shutdown,
    /// Ctrl-C, which exits 130 (CLI-8).
    Interrupted,
    /// The idle period passed.
    Idle,
}

fn system(error: impl std::fmt::Display) -> Failed {
    Failed::System {
        problem: error.to_string(),
    }
}

/// Connects the OpenID Connect authenticator, reading the client secret
/// from the Keychain.
async fn connect(config: OidcConfig) -> Result<OidcAuthenticator, Failed> {
    let secret =
        KeychainSecret::new(KEYCHAIN_SERVICE).map_err(|error| Failed::SecretUnavailable {
            problem: error.to_string(),
        })?;
    OidcAuthenticator::connect(config, &secret)
        .await
        .map_err(|error| match error {
            ConnectError::Secret(error) => Failed::SecretUnavailable {
                problem: error.to_string(),
            },
            other => Failed::ProviderUnavailable {
                problem: other.to_string(),
            },
        })
}

/// Waits for SIGTERM or Ctrl-C.
struct Signals {
    terminate: tokio::signal::unix::Signal,
    interrupt: tokio::signal::unix::Signal,
}

impl Signals {
    fn install() -> Result<Self, Failed> {
        Ok(Self {
            terminate: signal(SignalKind::terminate()).map_err(system)?,
            interrupt: signal(SignalKind::interrupt()).map_err(system)?,
        })
    }

    // Both `recv` futures are cancel-safe, so the one that loses the select
    // drops no signal that a later call needs.
    async fn wait(&mut self) -> Stop {
        tokio::select! {
            _ = self.terminate.recv() => Stop::Shutdown,
            _ = self.interrupt.recv() => Stop::Interrupted,
        }
    }
}

/// Runs `hangar serve` and returns what it served and why it stopped.
///
/// # Errors
///
/// Returns the failure that stopped it before or while it served.
pub(crate) async fn run(args: &ServeArgs, env: &Snapshot) -> Result<(Served, Stop), Failed> {
    let (root, store) = board::checked_store(&args.board, env).await?;
    let config = config::load(args.config.as_deref(), env)?;
    let oidc = match config.oidc {
        Some(config) => Some(connect(config).await?),
        None => None,
    };
    if open::answers(&root, env).await {
        return Err(Failed::AlreadyRunning);
    }
    let state = board::loopback_state(&root, env)?;
    let mut signals = Signals::install()?;
    let wanted = oidc.as_ref().map(|_| SIGN_IN_PORT);
    let listeners = Listeners::bind(args.port, wanted).map_err(|error| match error {
        BindError::PortInUse { port } => Failed::PortInUse { port },
        other => system(other),
    })?;
    let principal = local_principal(&env.user()).map_err(|error| Failed::StateUnusable {
        problem: format!("the account name is not a principal: {error}"),
    })?;
    let loopback = Loopback::new(listeners.port(), principal, Reach::Everything).map_err(system)?;
    let mut limits = Limits::default();
    limits.idle_exit = args.idle_exit;
    let gate = Gate::new(loopback, oidc);
    let app = App::new(store.clone(), store.clone(), store, gate, state, limits);
    log::start();
    tracing::info!(
        board = %root.display(),
        "hangar serves this board; run `hangar open` to open it in a browser"
    );
    let caught = OnceLock::new();
    let shutdown = async {
        let _ = caught.set(signals.wait().await);
    };
    let stopped = listeners
        .serve(app, shutdown)
        .await
        .map_err(|error| match error {
            ServeError::State(error) => Failed::StateUnusable {
                problem: error.to_string(),
            },
            other => system(other),
        })?;
    let stop = match stopped {
        Stopped::Idle => Stop::Idle,
        _ => caught.get().copied().unwrap_or(Stop::Shutdown),
    };
    let served = Served {
        board: root.display().to_string(),
        stopped: match stop {
            Stop::Shutdown => "shutdown",
            Stop::Interrupted => "interrupted",
            Stop::Idle => "idle",
        },
    };
    Ok((served, stop))
}
