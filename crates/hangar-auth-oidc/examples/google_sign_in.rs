//! Signs one person in with a Google account, for the manual check in the
//! crate's README.
//!
//! Run it as `google_sign_in <client id> <allowed email>`, or as
//! `google_sign_in <client id> --domain <domain>` to admit every verified
//! account of one Google Workspace domain. It reads the client secret from the Keychain item of the service `hangar-oidc-google`,
//! listens on 127.0.0.1:8740 and [::1]:8740, and serves two routes:
//! `/auth/oidc/start` redirects to Google, and `/auth/oidc/callback`
//! answers with one line, the principal that signed in or the refusal's
//! name. It prints nothing. When it cannot start, `main` returns the error.

use hangar_auth_oidc::{
    Admit, AllowEntry, AllowList, ClientId, DomainRule, Email, IssuerUrl, KeptState,
    KeychainSecret, OidcAuthenticator, OidcConfig, Reach, RedirectUri, ReturnTo, WorkspaceDomain,
};
use hangar_core::PrincipalId;
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::header::{COOKIE, LOCATION, SET_COOKIE};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::TcpListener;

const COOKIE_NAME: &str = "hangar_oidc_state";

/// Why the example could not start. `main` returns it, and the Rust runtime
/// prints its `Debug` output, which says what failed in a sentence.
enum StartError {
    /// The arguments are not a client ID and one email or `--domain` and a
    /// domain.
    Usage,
    /// A value in the configuration was refused.
    Config(hangar_auth_oidc::ConfigError),
    /// The client secret could not be read.
    Secret(hangar_auth_oidc::SecretError),
    /// The provider could not be reached or is misconfigured.
    Connect(hangar_auth_oidc::ConnectError),
    /// A listening socket could not be opened.
    Listen(std::io::ErrorKind),
}

impl std::fmt::Debug for StartError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage => formatter
                .write_str("usage: google_sign_in <client id> <allowed email | --domain <domain>>"),
            Self::Config(error) => write!(formatter, "the configuration was refused: {error}"),
            Self::Secret(error) => write!(formatter, "the client secret was refused: {error}"),
            Self::Connect(error) => write!(formatter, "the provider could not be used: {error}"),
            Self::Listen(kind) => write!(formatter, "port 8740 could not be opened: {kind}"),
        }
    }
}

/// The rule of who may sign in: one email, which signs in as the principal
/// `owner`, or every account of one domain, each of which signs in as its
/// email.
fn admit(rule: &[String]) -> Result<Admit, StartError> {
    match rule {
        [flag, domain] if flag == "--domain" => {
            let domain = WorkspaceDomain::try_from(domain.as_str()).map_err(StartError::Config)?;
            Ok(Admit::Domain(DomainRule::new(domain, Reach::Everything)))
        }
        [email] => {
            let principal = PrincipalId::try_from("owner").map_err(|_| StartError::Usage)?;
            let allow = AllowList::new(vec![AllowEntry::new(
                Email::try_from(email.as_str()).map_err(StartError::Config)?,
                principal,
                Reach::Everything,
            )])
            .map_err(StartError::Config)?;
            Ok(Admit::Emails(allow))
        }
        _ => Err(StartError::Usage),
    }
}

fn config(arguments: &[String]) -> Result<OidcConfig, StartError> {
    let [_, client_id, rule @ ..] = arguments else {
        return Err(StartError::Usage);
    };
    let admit = admit(rule)?;
    Ok(OidcConfig::new(
        IssuerUrl::try_from("https://accounts.google.com").map_err(StartError::Config)?,
        ClientId::try_from(client_id.as_str()).map_err(StartError::Config)?,
        RedirectUri::try_from("http://localhost:8740/auth/oidc/callback")
            .map_err(StartError::Config)?,
        admit,
    ))
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), StartError> {
    let arguments: Vec<String> = std::env::args().collect();
    let config = config(&arguments)?;
    let secret = KeychainSecret::new("hangar-oidc-google").map_err(StartError::Secret)?;
    let authenticator = Arc::new(
        OidcAuthenticator::connect(config, &secret)
            .await
            .map_err(StartError::Connect)?,
    );
    let mut listeners = Vec::new();
    for address in ["127.0.0.1:8740", "[::1]:8740"] {
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| StartError::Listen(error.kind()))?;
        listeners.push(listener);
    }
    let mut tasks = tokio::task::JoinSet::new();
    for listener in listeners {
        tasks.spawn(serve(listener, Arc::clone(&authenticator)));
    }
    while tasks.join_next().await.is_some() {}
    Ok(())
}

async fn serve(listener: TcpListener, authenticator: Arc<OidcAuthenticator>) {
    while let Ok((stream, _)) = listener.accept().await {
        let authenticator = Arc::clone(&authenticator);
        #[expect(
            clippy::disallowed_methods,
            reason = "the example serves each connection on a task that ends with the connection, and the process exits after one sign-in"
        )]
        tokio::spawn(async move {
            let service = service_fn(move |request| route(Arc::clone(&authenticator), request));
            let _ = hyper::server::conn::http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .await;
        });
    }
}

fn text(status: StatusCode, line: &str) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(format!("{line}\n"))));
    *response.status_mut() = status;
    response
}

fn kept_state(request: &Request<Incoming>) -> Option<KeptState> {
    let cookies = request.headers().get(COOKIE)?.to_str().ok()?;
    cookies
        .split(';')
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(name, _)| *name == COOKIE_NAME)
        .map(|(_, value)| KeptState::from_cookie(value))
}

async fn route(
    authenticator: Arc<OidcAuthenticator>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let response = match request.uri().path() {
        "/auth/oidc/start" => start(&authenticator),
        "/auth/oidc/callback" => {
            let query = request.uri().query().unwrap_or_default().to_owned();
            let kept = kept_state(&request);
            match authenticator.complete(&query, kept.as_ref()).await {
                Ok(signed_in) => text(StatusCode::OK, signed_in.principal().id().as_str()),
                Err(refusal) => text(StatusCode::FORBIDDEN, &format!("{refusal:?}")),
            }
        }
        _ => text(StatusCode::NOT_FOUND, "NotFound"),
    };
    Ok(response)
}

fn start(authenticator: &OidcAuthenticator) -> Response<Full<Bytes>> {
    let Ok(return_to) = ReturnTo::try_from("/") else {
        return text(StatusCode::INTERNAL_SERVER_ERROR, "ReturnTo");
    };
    let started = match authenticator.start(return_to) {
        Ok(started) => started,
        Err(refusal) => return text(StatusCode::SERVICE_UNAVAILABLE, &format!("{refusal:?}")),
    };
    let cookie = format!(
        "{COOKIE_NAME}={}; HttpOnly; SameSite=Lax; Path=/auth/oidc; Max-Age=600",
        started.kept().expose()
    );
    let mut response = text(StatusCode::FOUND, "Redirecting");
    let headers = response.headers_mut();
    if let (Ok(location), Ok(cookie)) = (started.redirect().as_str().parse(), cookie.parse()) {
        headers.insert(LOCATION, location);
        headers.insert(SET_COOKIE, cookie);
    }
    response
}
