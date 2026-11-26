//! `flare serve`: checks the board program, the config and the client
//! secret before it binds, then serves the board until Ctrl-C, SIGTERM or,
//! on the local site, the idle exit.
//!
//! Without a `hosted` section in the config it serves the local site and
//! prints one line that names the board and says to run `flare open`; it
//! never prints the launch host, a launch URL or the token. With one, it
//! serves the hosted site on the configured address behind a TLS proxy,
//! writes no server file and has no idle exit. A hosted site signs people
//! in through flare's OpenID Connect sign-in, whose client secret it reads
//! from the configured file, or through the signed token of a single
//! sign-on proxy, whose key set it fetches before it binds. When the hosted section names its stores, it checks
//! the DynamoDB table and the S3 bucket before it binds and serves the share
//! API from them, with the images of shares kept in the bucket.

mod hosted;

use crate::board;
use crate::cli::ServeArgs;
use crate::codes::{Failed, HeldPort};
use crate::config::{self, KEYCHAIN_SERVICE, LocalOidc, Secret, Site as Configured};
use crate::documents::Served;
use crate::env::Snapshot;
use crate::{log, open};
use crumbkit::Call;
use flare_auth_loopback::{Loopback, Served as Reach, local_principal};
use flare_auth_oidc::{
    ClientSecretSource, ConnectError, FileSecret, KeychainSecret, OidcAuthenticator, OidcConfig,
};
use flare_aws::{DynamoSealingKeys, DynamoSessionStore};
use flare_board::BoardStore;
use flare_server::{
    App, BindError, Gate, HostedSignIn, Limits, Listeners, Metrics, ServeError, Site, Stopped,
};
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

/// The stores that keep browser sessions and sealing keys in the hosted
/// site's table, so every process of the site shares them.
struct Persisted {
    sessions: DynamoSessionStore,
    keys: DynamoSealingKeys,
}

/// Connects the OpenID Connect authenticator, reading the client secret
/// from where `secret` says.
async fn connect(
    config: OidcConfig,
    secret: &Secret,
    stored: Option<Persisted>,
) -> Result<OidcAuthenticator, Failed> {
    match secret {
        Secret::Keychain => {
            let source = KeychainSecret::new(KEYCHAIN_SERVICE).map_err(|error| {
                Failed::SecretUnavailable {
                    problem: error.to_string(),
                }
            })?;
            connect_with(
                config,
                &source,
                |problem| Failed::SecretUnavailable { problem },
                stored,
            )
            .await
        }
        Secret::File(path) => {
            let shown = path.display().to_string();
            connect_with(
                config,
                &FileSecret::new(path),
                |problem| Failed::SecretFileUnavailable {
                    path: shown.clone(),
                    problem,
                },
                stored,
            )
            .await
        }
    }
}

async fn connect_with(
    config: OidcConfig,
    source: &impl ClientSecretSource,
    unreadable: impl Fn(String) -> Failed,
    stored: Option<Persisted>,
) -> Result<OidcAuthenticator, Failed> {
    let connected = match stored {
        Some(Persisted { sessions, keys }) => {
            OidcAuthenticator::connect_stored(config, source, sessions, keys).await
        }
        None => OidcAuthenticator::connect(config, source).await,
    };
    connected.map_err(|error| match error {
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
    serve_board(args, env).await.map_err(|failed| {
        failed
            .for_board(args.board.board.as_deref())
            .serving_again(again(args))
    })
}

/// `flare serve` with the arguments of `args` except `--port`, which a
/// held port's recovery leaves out.
fn again(args: &ServeArgs) -> Call {
    let path = |call: Call, name: &'static str, value: Option<&Path>| match value {
        Some(value) => call.value(name, value.display().to_string()),
        None => call,
    };
    let call = path(Call::new(&["serve"]), "board", args.board.board.as_deref());
    let call = path(call, "board_program", args.board.board_program.as_deref());
    let call = path(call, "config", args.config.as_deref());
    call.value("idle_exit", crate::duration::text(args.idle_exit))
}

/// Serves the board that `args` names.
async fn serve_board(args: &ServeArgs, env: &Snapshot) -> Result<(Served, Stop), Failed> {
    let (root, store) = board::checked_store(&args.board, env).await?;
    let config = config::load(args.config.as_deref(), env)?;
    let metrics = Metrics::new(config.metrics);
    match config.site {
        Configured::Local { oidc } => local(args, env, (&root, store), oidc, metrics).await,
        Configured::Hosted(site) => hosted::hosted(&root, store, site, metrics).await,
    }
}

/// The failure of a bind, where `held` says which port a held `port` is.
fn bind_failed(error: BindError, held: impl Fn(u16) -> HeldPort) -> Failed {
    match error {
        BindError::PortInUse { port } => Failed::PortInUse {
            port,
            held: held(port),
            again: None,
        },
        other => system(other),
    }
}

/// Serves the local site.
async fn local(
    args: &ServeArgs,
    env: &Snapshot,
    (root, store): (&Path, BoardStore),
    oidc: Option<LocalOidc>,
    metrics: Metrics,
) -> Result<(Served, Stop), Failed> {
    let oidc = match oidc {
        Some(LocalOidc { config, secret }) => Some(connect(config, &secret, None).await?),
        None => None,
    };
    if open::answers(root, env).await {
        return Err(Failed::AlreadyRunning { board: None });
    }
    let state = board::loopback_state(root, env)?;
    let mut signals = Signals::install()?;
    let wanted = oidc.as_ref().map(|_| SIGN_IN_PORT);
    let listeners = Listeners::bind(args.port, wanted).map_err(|error| {
        bind_failed(error, |port| {
            if wanted == Some(port) && args.port != port {
                HeldPort::SignIn
            } else {
                HeldPort::Launch
            }
        })
    })?;
    let principal = local_principal(&env.user()).map_err(|error| Failed::StateUnusable {
        path: None,
        problem: format!("the account name is not a principal: {error}"),
    })?;
    let loopback = Loopback::new(listeners.port(), principal, Reach::Everything).map_err(system)?;
    let mut limits = Limits::default();
    limits.idle_exit = args.idle_exit;
    let site = Site::Local {
        gate: Gate::new(loopback, oidc),
        state,
    };
    let app: App<_, _, _> =
        App::new(store.clone(), store.clone(), store, site, limits).with_metrics(metrics);
    log::start();
    tracing::info!(
        board = %root.display(),
        "flare serves this board; run `flare open` to open it in a browser"
    );
    serve(root, listeners, app, &mut signals).await
}

/// Serves `app` until a signal or, on the local site, the idle exit.
async fn serve<H: HostedSignIn>(
    root: &Path,
    listeners: Listeners,
    app: App<BoardStore, BoardStore, BoardStore, flare_auth_oidc::SystemClock, H>,
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
            ServeError::State(error) => board::state_unusable(&error),
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
