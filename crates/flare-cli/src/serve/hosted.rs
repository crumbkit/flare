//! The hosted site of `flare serve`: it checks the share API's stores and
//! the identifier kind their table records, reads the teams document that
//! `hosted.teams` names, connects the sign-in, and only then binds the
//! listen address.

use super::teams::{HostedTeams, hosted_teams};
use super::{Persisted, Signals, Stop, bind_failed, connect, serve};
use crate::codes::{Failed, HeldPort};
use crate::config::{Hosted, HostedCreators, Secret, Sharing as SharingConfig, SignIn};
use crate::documents::Served;
use crate::log;
use flare_auth_oidc::{Admit, OidcAuthenticator, ProxyAuthenticator, ProxyConfig};
use flare_aws::{
    DynamoCommentStore, DynamoCursorStore, DynamoGrantStore, DynamoResponseStore, DynamoShareStore,
    DynamoTokenStore, IdentityError, S3FileStore, Stores,
};
use flare_board::BoardStore;
use flare_core::{Flares, IdentityKind, NamedRecipients};
use flare_server::{App, HostedSignIn, Limits, Listeners, Metrics, PublicOrigin, Sharing, Site};
use std::net::SocketAddr;
use std::path::Path;

/// Serves the hosted site: connects its sign-in and checks its stores
/// before it binds.
pub(super) async fn hosted(
    root: &Path,
    store: BoardStore,
    hosted: Hosted,
    metrics: Metrics,
) -> Result<(Served, Stop), Failed> {
    let Hosted {
        origin,
        listen,
        sign_in,
        sharing,
    } = hosted;
    let admit = sign_in.admit().clone();
    let kind = sign_in.identity().kind();
    let stores = match &sharing {
        Some(config) => Some(checked(Stores::connect(&config.stores).await, kind).await?),
        None => None,
    };
    let teams = match (&sharing, &stores) {
        (Some(config), Some(stores)) => Some(hosted_teams(&config.teams, stores, kind).await?),
        _ => None,
    };
    let persisted = stores.as_ref().map(|stores| Persisted {
        sessions: stores.sessions.clone(),
        keys: stores.keys.clone(),
    });
    let sign_in = match sign_in {
        SignIn::Oidc {
            config,
            secret_file,
        } => Connected::Oidc(connect(config, &Secret::File(secret_file), persisted).await?),
        SignIn::Proxy(config) => Connected::Proxy(connect_proxy(config).await?),
    };
    if sharing.as_ref().is_some_and(|config| config.open_to_anyone) {
        tracing::warn!(
            "hosted.creators is \"anyone admitted\" under an anyone admit rule, so any account the issuer verifies can sign in and share"
        );
    }
    let sharing = match (sharing, stores, teams) {
        (Some(config), Some(stores), Some(teams)) => {
            Some(share_api(*config, stores, teams, admit, kind))
        }
        _ => None,
    };
    let start = Start {
        root,
        store,
        origin,
        listen,
        metrics,
        sharing,
    };
    match sign_in {
        Connected::Oidc(oidc) => start.serve(oidc).await,
        Connected::Proxy(proxy) => start.serve(proxy).await,
    }
}

/// The stores, once their table and bucket answer and the table records
/// `kind`, which a first start writes. A table that records another kind
/// is refused, with nothing written.
async fn checked(stores: Stores, kind: IdentityKind) -> Result<Stores, Failed> {
    let unavailable = |problem: String| Failed::StoresUnavailable { problem };
    stores
        .check()
        .await
        .map_err(|error| unavailable(error.to_string()))?;
    match stores.identity(kind).await {
        Ok(()) => Ok(stores),
        Err(IdentityError::Changed { recorded }) => Err(Failed::IdentityChanged {
            recorded: recorded.as_str().to_owned(),
            configured: kind.as_str().to_owned(),
        }),
        Err(error) => Err(unavailable(error.to_string())),
    }
}

/// A hosted site's connected sign-in.
#[expect(
    clippy::large_enum_variant,
    reason = "one value lives on the stack while the hosted site starts, and boxing it saves nothing"
)]
enum Connected {
    Oidc(OidcAuthenticator),
    Proxy(ProxyAuthenticator),
}

/// Fetches the proxy's key set, naming `proxy.jwks_url` when it cannot.
async fn connect_proxy(config: ProxyConfig) -> Result<ProxyAuthenticator, Failed> {
    let url = config.jwks_url().as_str().to_owned();
    ProxyAuthenticator::connect(config)
        .await
        .map_err(|error| Failed::ProxyKeysUnavailable {
            url,
            problem: error.to_string(),
        })
}

/// What the hosted site serves with, once its sign-in is connected.
struct Start<'a> {
    root: &'a Path,
    store: BoardStore,
    origin: PublicOrigin,
    listen: SocketAddr,
    metrics: Metrics,
    sharing: Option<HostedSharing>,
}

impl Start<'_> {
    /// Binds the listen address and serves the hosted site with `sign_in`.
    async fn serve<H: HostedSignIn>(self, sign_in: H) -> Result<(Served, Stop), Failed> {
        let mut signals = Signals::install()?;
        let listeners = Listeners::bind_hosted(self.listen)
            .map_err(|error| bind_failed(error, |_| HeldPort::Hosted))?;
        log::start();
        tracing::info!(
            board = %self.root.display(),
            origin = self.origin.as_str(),
            "flare serves this board as a hosted site behind a TLS proxy"
        );
        let site = Site::Hosted {
            origin: self.origin,
            sign_in,
        };
        let store = self.store;
        let app: App<_, _, _, _, H> =
            App::new(store.clone(), store.clone(), store, site, Limits::default())
                .with_metrics(self.metrics);
        let app = match self.sharing {
            Some(sharing) => app.with_sharing(sharing),
            None => app,
        };
        serve(self.root, listeners, app, &mut signals).await
    }
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
    HostedTeams,
    HostedCreators,
    S3FileStore,
>;

/// The share API on the checked table and bucket, naming people of
/// `kind`, reading its teams from `teams`, keeping images in the bucket and
/// accepting a token only while `admit` admits its owner.
fn share_api(
    config: SharingConfig,
    stores: Stores,
    teams: HostedTeams,
    admit: Admit,
    kind: IdentityKind,
) -> HostedSharing {
    let flares = Flares::new(
        stores.shares,
        stores.grants,
        stores.responses,
        stores.comments,
        stores.cursors,
        config.creators,
    )
    .with_teams(teams)
    .with_identity(kind);
    let api = Sharing::new(flares, stores.tokens, config.recipients, admit)
        .with_everyone_signed_in(config.everyone_signed_in)
        .with_files(stores.files);
    match config.get_flare {
        Some(address) => api.with_get_flare(address),
        None => api,
    }
}
