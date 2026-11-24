//! `flare serve`: checks the board program, the config and the client
//! secret before it binds, then serves the board until Ctrl-C, SIGTERM or,
//! on the local site, the idle exit.
//!
//! Without a `hosted` section in the config it serves the local site and
//! prints one line that names the board and says to run `flare open`; it
//! never prints the launch host, a launch URL or the token. With one, it
//! serves the hosted site on the configured address behind a TLS proxy,
//! reads the client secret from the configured file, writes no server file
//! and has no idle exit. When the hosted section names its stores, it checks
//! the DynamoDB table and the S3 bucket before it binds and serves the share
//! API from them.

use crate::board;
use crate::cli::ServeArgs;
use crate::codes::{Failed, HeldPort};
use crate::config::{
    self, Hosted, KEYCHAIN_SERVICE, LocalOidc, Secret, Sharing as SharingConfig, Site as Configured,
};
use crate::documents::Served;
use crate::env::Snapshot;
use crate::{log, open};
use crumbkit::Call;
use flare_auth_loopback::{Loopback, Served as Reach, local_principal};
use flare_auth_oidc::{
    Admit, ClientSecretSource, ConnectError, FileSecret, KeychainSecret, OidcAuthenticator,
    OidcConfig,
};
use flare_aws::{
    DynamoCommentStore, DynamoCursorStore, DynamoGrantStore, DynamoResponseStore,
    DynamoSealingKeys, DynamoSessionStore, DynamoShareStore, DynamoTeamStore, DynamoTokenStore,
    Stores,
};
use flare_board::BoardStore;
use flare_core::{FlareTeams, Flares, NamedRecipients};
use flare_server::{
    App, BindError, Gate, Limits, Listeners, Metrics, ServeError, Sharing, Site, Stopped,
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
        Configured::Hosted(hosted) => self::hosted(&root, store, hosted, metrics).await,
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
    let app = App::new(store.clone(), store.clone(), store, site, limits).with_metrics(metrics);
    log::start();
    tracing::info!(
        board = %root.display(),
        "flare serves this board; run `flare open` to open it in a browser"
    );
    serve(root, listeners, app, &mut signals).await
}

/// Serves the hosted site: reads the secret file, draws the sealing key
/// and discovers the provider before it binds.
async fn hosted(
    root: &Path,
    store: BoardStore,
    hosted: Hosted,
    metrics: Metrics,
) -> Result<(Served, Stop), Failed> {
    let Hosted {
        origin,
        listen,
        oidc,
        secret_file,
        sharing,
    } = hosted;
    let admit = oidc.admit().clone();
    let stores = match &sharing {
        Some(config) => Some(Stores::connect(&config.stores).await),
        None => None,
    };
    let persisted = stores.as_ref().map(|stores| Persisted {
        sessions: stores.sessions.clone(),
        keys: stores.keys.clone(),
    });
    let oidc = connect(oidc, &Secret::File(secret_file), persisted).await?;
    if sharing.as_ref().is_some_and(|config| config.open_to_anyone) {
        tracing::warn!(
            "hosted.creators is \"anyone admitted\" under an anyone admit rule, so any account the issuer verifies can sign in and share"
        );
    }
    let sharing = match (sharing, stores) {
        (Some(config), Some(stores)) => Some(share_api(*config, stores, admit).await?),
        _ => None,
    };
    let mut signals = Signals::install()?;
    let listeners =
        Listeners::bind_hosted(listen).map_err(|error| bind_failed(error, |_| HeldPort::Hosted))?;
    log::start();
    tracing::info!(
        board = %root.display(),
        origin = origin.as_str(),
        "flare serves this board as a hosted site behind a TLS proxy"
    );
    let site = Site::Hosted { origin, oidc };
    let app = App::new(store.clone(), store.clone(), store, site, Limits::default())
        .with_metrics(metrics);
    let app = match sharing {
        Some(sharing) => app.with_sharing(sharing),
        None => app,
    };
    serve(root, listeners, app, &mut signals).await
}

/// The share API on the hosted site's table and bucket.
type HostedSharing = Sharing<
    DynamoShareStore,
    DynamoGrantStore,
    DynamoResponseStore,
    DynamoCommentStore,
    DynamoCursorStore,
    DynamoTokenStore,
    NamedRecipients,
    FlareTeams<DynamoTeamStore>,
>;

/// The share API on the configured table and bucket, which it checks
/// first, accepting a token only while `admit` admits its owner.
async fn share_api(
    config: SharingConfig,
    stores: Stores,
    admit: Admit,
) -> Result<HostedSharing, Failed> {
    stores
        .check()
        .await
        .map_err(|error| Failed::StoresUnavailable {
            problem: error.to_string(),
        })?;
    let flares = Flares::new(
        stores.shares,
        stores.grants,
        stores.responses,
        stores.comments,
        stores.cursors,
        config.creators,
    )
    .with_teams(FlareTeams::new(stores.teams));
    let api = Sharing::new(flares, stores.tokens, config.recipients, admit);
    Ok(match config.get_flare {
        Some(address) => api.with_get_flare(address),
        None => api,
    })
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
