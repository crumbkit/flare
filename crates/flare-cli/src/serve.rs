//! `flare serve`: checks the board program, the config and the client
//! secret before it binds, then serves the board until Ctrl-C, SIGTERM or,
//! on the local site, the idle exit.
//!
//! Without a `hosted` section in the config it serves the local site and
//! prints one line that names the board and says to run `flare open`; it
//! never prints the launch host, a launch URL or the token. With one, it
//! serves the hosted site on the configured address behind a TLS proxy,
//! reads the client secret from the configured file, writes no server file
//! and has no idle exit.

use crate::board;
use crate::cli::ServeArgs;
use crate::codes::Failed;
use crate::config::{self, Hosted, KEYCHAIN_SERVICE, LocalOidc, Secret, Site as Configured};
use crate::documents::Served;
use crate::env::Snapshot;
use crate::{log, open};
use flare_auth_loopback::{Loopback, Served as Reach, local_principal};
use flare_auth_oidc::{
    ClientSecretSource, ConnectError, FileSecret, KeychainSecret, OidcAuthenticator, OidcConfig,
};
use flare_board::BoardStore;
use flare_server::{App, BindError, Gate, Limits, Listeners, ServeError, Site, Stopped};
use std::path::Path;
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
/// from where `secret` says.
async fn connect(config: OidcConfig, secret: &Secret) -> Result<OidcAuthenticator, Failed> {
    match secret {
        Secret::Keychain => {
            let source = KeychainSecret::new(KEYCHAIN_SERVICE).map_err(|error| {
                Failed::SecretUnavailable {
                    problem: error.to_string(),
                }
            })?;
            connect_with(config, &source, |problem| Failed::SecretUnavailable {
                problem,
            })
            .await
        }
        Secret::File(path) => {
            let shown = path.display().to_string();
            connect_with(config, &FileSecret::new(path), |problem| {
                Failed::SecretFileUnavailable {
                    path: shown.clone(),
                    problem,
                }
            })
            .await
        }
    }
}

async fn connect_with(
    config: OidcConfig,
    source: &impl ClientSecretSource,
    unreadable: impl Fn(String) -> Failed,
) -> Result<OidcAuthenticator, Failed> {
    OidcAuthenticator::connect(config, source)
        .await
        .map_err(|error| match error {
            ConnectError::Secret(error) => unreadable(error.to_string()),
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

/// Runs `flare serve` and returns what it served and why it stopped.
///
/// # Errors
///
/// Returns the failure that stopped it before or while it served.
pub(crate) async fn run(args: &ServeArgs, env: &Snapshot) -> Result<(Served, Stop), Failed> {
    let (root, store) = board::checked_store(&args.board, env).await?;
    let config = config::load(args.config.as_deref(), env)?;
    match config.site {
        Configured::Local { oidc } => local(args, env, (&root, store), oidc).await,
        Configured::Hosted(hosted) => self::hosted(&root, store, hosted).await,
    }
}

fn bind_failed(error: BindError) -> Failed {
    match error {
        BindError::PortInUse { port } => Failed::PortInUse { port },
        other => system(other),
    }
}

/// Serves the local site.
async fn local(
    args: &ServeArgs,
    env: &Snapshot,
    (root, store): (&Path, BoardStore),
    oidc: Option<LocalOidc>,
) -> Result<(Served, Stop), Failed> {
    let oidc = match oidc {
        Some(LocalOidc { config, secret }) => Some(connect(config, &secret).await?),
        None => None,
    };
    if open::answers(root, env).await {
        return Err(Failed::AlreadyRunning);
    }
    let state = board::loopback_state(root, env)?;
    let mut signals = Signals::install()?;
    let wanted = oidc.as_ref().map(|_| SIGN_IN_PORT);
    let listeners = Listeners::bind(args.port, wanted).map_err(bind_failed)?;
    let principal = local_principal(&env.user()).map_err(|error| Failed::StateUnusable {
        problem: format!("the account name is not a principal: {error}"),
    })?;
    let loopback = Loopback::new(listeners.port(), principal, Reach::Everything).map_err(system)?;
    let mut limits = Limits::default();
    limits.idle_exit = args.idle_exit;
    let site = Site::Local {
        gate: Gate::new(loopback, oidc),
        state,
    };
    let app = App::new(store.clone(), store.clone(), store, site, limits);
    log::start();
    tracing::info!(
        board = %root.display(),
        "flare serves this board; run `flare open` to open it in a browser"
    );
    serve(root, listeners, app, &mut signals).await
}

/// Serves the hosted site: reads the secret file, draws the sealing key
/// and discovers the provider before it binds.
async fn hosted(root: &Path, store: BoardStore, hosted: Hosted) -> Result<(Served, Stop), Failed> {
    let Hosted {
        origin,
        listen,
        oidc,
        secret_file,
    } = hosted;
    let oidc = connect(oidc, &Secret::File(secret_file)).await?;
    let mut signals = Signals::install()?;
    let listeners = Listeners::bind_hosted(listen).map_err(bind_failed)?;
    log::start();
    tracing::info!(
        board = %root.display(),
        origin = origin.as_str(),
        "flare serves this board as a hosted site behind a TLS proxy"
    );
    let site = Site::Hosted { origin, oidc };
    let app = App::new(store.clone(), store.clone(), store, site, Limits::default());
    serve(root, listeners, app, &mut signals).await
}

/// Serves `app` until a signal or, on the local site, the idle exit.
async fn serve(
    root: &Path,
    listeners: Listeners,
    app: App<BoardStore, BoardStore, BoardStore>,
    signals: &mut Signals,
) -> Result<(Served, Stop), Failed> {
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
