//! A fake OpenID provider that serves HTTPS on 127.0.0.1, with a CA and a
//! leaf certificate for `localhost` that each run generates.
//!
//! The shipped `flare serve` accepts only an https issuer, so the provider
//! speaks TLS. It serves discovery, authorisation, token and key set
//! endpoints with a new RSA key, and signs RS256 ID tokens. The token
//! endpoint checks the code, the PKCE verifier, the client id, the client
//! secret and the redirect URI, and accepts each code once. It is a
//! trimmed copy of the HTTP half of
//! `flare-auth-oidc/tests/support/fake_provider.rs`, which cannot be
//! included whole because it needs that crate's `testing` feature.

use aws_lc_rs::digest;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::rsa::{KeyPair as RsaKeyPair, KeySize};
use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_SHA256};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use http_body_util::{BodyExt as _, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use rcgen::{BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::aws_lc_rs::default_provider;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use url::Url;

/// The client id that the provider knows.
pub const CLIENT_ID: &str = "flare-e2e-client";
/// The client secret that the provider knows, a made-up test value.
pub const CLIENT_SECRET: &str = "made-up-e2e-client-secret";
/// The email of the person the provider signs in.
pub const EMAIL: &str = "ada@example.com";

struct Grant {
    nonce: String,
    challenge: String,
    redirect_uri: String,
}

struct State {
    issuer: String,
    key: RsaKeyPair,
    grants: HashMap<String, Grant>,
    redeemed: HashSet<String>,
    next_code: u64,
}

/// A running provider.
pub struct Provider {
    /// The issuer, `https://localhost:<port>`.
    pub issuer: String,
    /// The port, on 127.0.0.1.
    pub port: u16,
    /// The CA certificate as PEM, which a process must trust to reach the
    /// provider.
    pub ca_pem: String,
}

impl std::fmt::Debug for Provider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Provider")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a time after 1970")
        .as_secs()
}

/// A CA, and a TLS configuration whose leaf certificate it signed for
/// `localhost`. Returns the CA as PEM.
fn certificates() -> (String, ServerConfig) {
    let ca_key = KeyPair::generate().expect("a CA key");
    let mut ca_params = CertificateParams::new(Vec::new()).expect("CA parameters");
    ca_params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    ca_params
        .distinguished_name
        .push(DnType::CommonName, "flare end-to-end test CA");
    ca_params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    let ca = ca_params.self_signed(&ca_key).expect("a CA certificate");
    let issuer = Issuer::from_params(&ca_params, &ca_key);
    let leaf_key = KeyPair::generate().expect("a leaf key");
    let leaf = CertificateParams::new(vec!["localhost".to_owned()])
        .expect("leaf parameters")
        .signed_by(&leaf_key, &issuer)
        .expect("a leaf certificate");
    let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf_key.serialize_der()));
    let chain: Vec<CertificateDer<'static>> = vec![leaf.der().clone()];
    let config = ServerConfig::builder_with_provider(Arc::new(default_provider()))
        .with_safe_default_protocol_versions()
        .expect("TLS versions")
        .with_no_client_auth()
        .with_single_cert(chain, key)
        .expect("a certificate chain");
    (ca.pem(), config)
}

impl Provider {
    /// Starts the provider on a new port of 127.0.0.1, on the current
    /// Tokio runtime.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let port = listener.local_addr().expect("an address").port();
        let issuer = format!("https://localhost:{port}");
        let (ca_pem, config) = certificates();
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let state = Arc::new(Mutex::new(State {
            issuer: issuer.clone(),
            key: RsaKeyPair::generate(KeySize::Rsa2048).expect("an RSA key"),
            grants: HashMap::new(),
            redeemed: HashSet::new(),
            next_code: 0,
        }));
        #[expect(
            clippy::disallowed_methods,
            reason = "the provider's tasks end with the test's runtime"
        )]
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let acceptor = acceptor.clone();
                let state = Arc::clone(&state);
                #[expect(
                    clippy::disallowed_methods,
                    reason = "each connection's task ends with the connection or the test's runtime"
                )]
                tokio::spawn(async move {
                    let Ok(stream) = acceptor.accept(stream).await else {
                        return;
                    };
                    let service = service_fn(move |request| handle(Arc::clone(&state), request));
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Self {
            issuer,
            port,
            ca_pem,
        }
    }
}

fn param(url: &Url, name: &str) -> String {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
        .unwrap_or_default()
}

fn reply(status: StatusCode, body: &Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body.to_string())));
    *response.status_mut() = status;
    response
}

async fn handle(
    shared: Arc<Mutex<State>>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let (parts, body) = request.into_parts();
    let body = body
        .collect()
        .await
        .map(|collected| collected.to_bytes())
        .unwrap_or_default();
    let mut state = shared.lock().expect("the provider's lock");
    Ok(match parts.uri.path() {
        "/.well-known/openid-configuration" => reply(StatusCode::OK, &metadata(&state.issuer)),
        "/keys" => reply(StatusCode::OK, &json!({ "keys": [jwk(&state.key)] })),
        "/token" => token(&mut state, &String::from_utf8_lossy(&body)),
        "/authorize" => authorize(&mut state, parts.uri.query().unwrap_or_default()),
        _ => reply(StatusCode::NOT_FOUND, &json!({})),
    })
}

fn metadata(issuer: &str) -> Value {
    json!({
        "issuer": issuer,
        "authorization_endpoint": format!("{issuer}/authorize"),
        "token_endpoint": format!("{issuer}/token"),
        "jwks_uri": format!("{issuer}/keys"),
        "response_types_supported": ["code"],
        "subject_types_supported": ["public"],
        "id_token_signing_alg_values_supported": ["RS256"],
        "code_challenge_methods_supported": ["S256"],
    })
}

fn jwk(key: &RsaKeyPair) -> Value {
    let public = key.public_key();
    json!({
        "kty": "RSA",
        "use": "sig",
        "alg": "RS256",
        "kid": "key-1",
        "n": b64(public.modulus().big_endian_without_leading_zero()),
        "e": b64(public.exponent().big_endian_without_leading_zero()),
    })
}

/// Plays the person approving the sign-in: grants a code and redirects to
/// the redirect URI with it and the state.
fn authorize(state: &mut State, query: &str) -> Response<Full<Bytes>> {
    let url = Url::parse(&format!("{}/authorize?{query}", state.issuer)).expect("a URL");
    let code = format!("code-{}", state.next_code);
    state.next_code += 1;
    state.grants.insert(
        code.clone(),
        Grant {
            nonce: param(&url, "nonce"),
            challenge: param(&url, "code_challenge"),
            redirect_uri: param(&url, "redirect_uri"),
        },
    );
    let back = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs([("code", code.as_str()), ("state", &param(&url, "state"))])
        .finish();
    let mut response = reply(StatusCode::FOUND, &json!({}));
    let location = format!("{}?{back}", param(&url, "redirect_uri"));
    response
        .headers_mut()
        .insert("location", location.parse().expect("a header value"));
    response
}

fn token(state: &mut State, body: &str) -> Response<Full<Bytes>> {
    let fields: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let field = |name: &str| fields.get(name).map(String::as_str).unwrap_or_default();
    let refused = reply(
        StatusCode::BAD_REQUEST,
        &json!({ "error": "invalid_grant" }),
    );
    let code = field("code").to_owned();
    if state.redeemed.contains(&code) {
        return refused;
    }
    let Some(grant) = state.grants.get(&code) else {
        return refused;
    };
    let challenge =
        b64(digest::digest(&digest::SHA256, field("code_verifier").as_bytes()).as_ref());
    let good = field("grant_type") == "authorization_code"
        && field("client_id") == CLIENT_ID
        && field("client_secret") == CLIENT_SECRET
        && field("redirect_uri") == grant.redirect_uri
        && challenge == grant.challenge;
    if !good {
        return reply(
            StatusCode::UNAUTHORIZED,
            &json!({ "error": "invalid_client" }),
        );
    }
    let nonce = grant.nonce.clone();
    state.redeemed.insert(code);
    reply(
        StatusCode::OK,
        &json!({
            "access_token": "made-up-access-token",
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": mint(state, &nonce),
        }),
    )
}

/// An RS256 ID token for [`EMAIL`] with `nonce`.
fn mint(state: &State, nonce: &str) -> String {
    let now = now_seconds();
    let claims = json!({
        "iss": state.issuer,
        "aud": CLIENT_ID,
        "azp": CLIENT_ID,
        "sub": "1234567890",
        "email": EMAIL,
        "email_verified": true,
        "nonce": nonce,
        "iat": now,
        "exp": now + 3600,
    });
    let header = json!({ "alg": "RS256", "kid": "key-1", "typ": "JWT" });
    let signed = format!(
        "{}.{}",
        b64(header.to_string().as_bytes()),
        b64(claims.to_string().as_bytes())
    );
    let mut signature = vec![0; state.key.public_modulus_len()];
    state
        .key
        .sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            signed.as_bytes(),
            &mut signature,
        )
        .expect("a signature");
    format!("{signed}.{}", b64(&signature))
}
