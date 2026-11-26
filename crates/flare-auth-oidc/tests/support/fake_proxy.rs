//! A fake single sign-on proxy's issuer that runs inside the test process:
//! it publishes a key set over HTTP on 127.0.0.1, with a new RSA key and a
//! new P-256 key for each test, and signs the tokens a proxy would set as
//! its cookie. Each test sets what the issuer does wrong.

use aws_lc_rs::hmac;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::{
    ECDSA_P256_SHA256_FIXED_SIGNING, EcdsaKeyPair, KeyPair as _, RSA_PKCS1_SHA256,
};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flare_auth_oidc::{
    Admit, AllowEntry, AllowList, Audience, CookieName, DomainRule, Email, KeySetUrl,
    ProxyAlgorithm, ProxyAlgorithms, ProxyAuthenticator, ProxyConfig, ProxyIssuer, Reach,
    TestClock, WorkspaceDomain,
};
use flare_core::{PageName, PrincipalId, Resource};
use http_body_util::Full;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use serde_json::{Map, Value, json};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;

/// The issuer that the proxy's tokens name.
pub const ISSUER: &str = "https://sso.example.com";
/// The audience of flare's application behind the proxy.
pub const AUDIENCE: &str = "flare-behind-the-proxy";
/// The cookie that the proxy sets.
pub const COOKIE: &str = "proxy_token";
/// The address of the person the tests sign in.
pub const EMAIL: &str = "ada@example.com";
/// The principal that the allow-list gives that person.
pub const PRINCIPAL: &str = "ada";
/// The page that the admitted person may reach.
pub const HOME: &str = "home";
/// The domain of the tests' domain rule.
pub const DOMAIN: &str = "example.com";
/// The RSA key's id.
pub const RSA_KID: &str = "rsa-1";
/// The P-256 key's id.
pub const EC_KID: &str = "ec-1";

/// How the issuer signs a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sign {
    /// RS256 with the published RSA key.
    Rs256,
    /// ES256 with the published P-256 key.
    Es256,
    /// `alg: none` and no signature.
    AlgNone,
    /// HS256 keyed with the RSA key's public modulus, as an attacker who
    /// read the key set would sign.
    Hs256WithPublicKey,
    /// RS256 with a key the key set does not publish, under a new key id.
    UnpublishedKey,
    /// RS256 with the published key, and one byte of the signature changed.
    BadSignature,
    /// ES256 with the published P-256 key, under the RSA key's id.
    Es256NamingTheRsaKey,
    /// RS256 with the published RSA key, under the P-256 key's id.
    Rs256NamingTheEcKey,
}

/// What the key set endpoint replies.
#[derive(Debug, Clone, PartialEq)]
pub enum KeysReply {
    /// The keys the issuer publishes.
    Normal,
    /// A 500 reply.
    Down,
    /// This JSON document.
    Json(Value),
}

struct State {
    rsa: KeyPair,
    ec: EcdsaKeyPair,
    published: Vec<&'static str>,
    reply: KeysReply,
    fetches: usize,
}

/// A running fake issuer.
#[derive(Clone)]
pub struct FakeProxy {
    state: Arc<Mutex<State>>,
    keys_url: String,
}

impl std::fmt::Debug for FakeProxy {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("FakeProxy")
            .field("keys_url", &self.keys_url)
            .finish_non_exhaustive()
    }
}

fn b64(bytes: &[u8]) -> String {
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Seconds since the Unix epoch, now.
pub fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("a time after 1970")
        .as_secs()
}

fn reply(status: StatusCode, body: &Value) -> Response<Full<Bytes>> {
    let mut response = Response::new(Full::new(Bytes::from(body.to_string())));
    *response.status_mut() = status;
    response
}

impl FakeProxy {
    /// Starts a fake issuer on a new port of 127.0.0.1, on the current
    /// Tokio runtime.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let keys_url = format!("http://{}/jwks", listener.local_addr().expect("an address"));
        let state = Arc::new(Mutex::new(State {
            rsa: KeyPair::generate(KeySize::Rsa2048).expect("an RSA key"),
            ec: EcdsaKeyPair::generate(&ECDSA_P256_SHA256_FIXED_SIGNING).expect("a P-256 key"),
            published: vec![RSA_KID, EC_KID],
            reply: KeysReply::Normal,
            fetches: 0,
        }));
        let shared = Arc::clone(&state);
        #[expect(
            clippy::disallowed_methods,
            reason = "the fake issuer's tasks end with the test's runtime"
        )]
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let shared = Arc::clone(&shared);
                #[expect(
                    clippy::disallowed_methods,
                    reason = "each connection's task ends with the connection or the test's runtime"
                )]
                tokio::spawn(async move {
                    let service = service_fn(move |request| handle(Arc::clone(&shared), request));
                    let _ = hyper::server::conn::http1::Builder::new()
                        .serve_connection(TokioIo::new(stream), service)
                        .await;
                });
            }
        });
        Self { state, keys_url }
    }

    fn with<T>(&self, change: impl FnOnce(&mut State) -> T) -> T {
        change(&mut self.state.lock().expect("the fake's lock"))
    }

    /// The key set's URL.
    pub fn keys_url(&self) -> &str {
        &self.keys_url
    }

    /// Sets what the key set endpoint replies.
    pub fn keys_reply(&self, reply: KeysReply) {
        self.with(|state| state.reply = reply);
    }

    /// Stops publishing the key `kid`.
    pub fn unpublish(&self, kid: &str) {
        self.with(|state| state.published.retain(|published| *published != kid));
    }

    /// How many requests reached the key set endpoint.
    pub fn fetches(&self) -> usize {
        self.with(|state| state.fetches)
    }

    /// The claims of a token that every check accepts: the issuer, the
    /// audience, issued now, expiring in five minutes, for [`EMAIL`]
    /// marked verified.
    pub fn claims() -> Map<String, Value> {
        let now = now_seconds();
        let mut claims = Map::new();
        claims.insert("iss".into(), json!(ISSUER));
        claims.insert("aud".into(), json!(AUDIENCE));
        claims.insert("sub".into(), json!("user-1"));
        claims.insert("email".into(), json!(EMAIL));
        claims.insert("email_verified".into(), json!(true));
        claims.insert("iat".into(), json!(now));
        claims.insert("exp".into(), json!(now + 300));
        claims
    }

    /// A token with [`FakeProxy::claims`], changed by `edit`, signed as
    /// `sign` says.
    pub fn token_with(&self, sign: Sign, edit: impl FnOnce(&mut Map<String, Value>)) -> String {
        let mut claims = Self::claims();
        edit(&mut claims);
        self.sign(sign, &Value::Object(claims))
    }

    /// A token that every check accepts, signed as `sign` says.
    pub fn token(&self, sign: Sign) -> String {
        self.token_with(sign, |_| {})
    }

    /// `claims` signed as `sign` says.
    pub fn sign(&self, sign: Sign, claims: &Value) -> String {
        let (alg, kid) = match sign {
            Sign::Rs256 | Sign::BadSignature => ("RS256", RSA_KID),
            Sign::Es256 => ("ES256", EC_KID),
            Sign::AlgNone => ("none", RSA_KID),
            Sign::Hs256WithPublicKey => ("HS256", RSA_KID),
            Sign::UnpublishedKey => ("RS256", "unpublished-1"),
            Sign::Es256NamingTheRsaKey => ("ES256", RSA_KID),
            Sign::Rs256NamingTheEcKey => ("RS256", EC_KID),
        };
        let header = json!({ "alg": alg, "kid": kid, "typ": "JWT" });
        let signed = format!(
            "{}.{}",
            b64(header.to_string().as_bytes()),
            b64(claims.to_string().as_bytes())
        );
        let random = SystemRandom::new();
        let signature = self.with(|state| match sign {
            Sign::AlgNone => Vec::new(),
            Sign::Es256 | Sign::Es256NamingTheRsaKey => state
                .ec
                .sign(&random, signed.as_bytes())
                .expect("a signature")
                .as_ref()
                .to_vec(),
            Sign::Hs256WithPublicKey => {
                let public = state.rsa.public_key();
                let secret = public.modulus().big_endian_without_leading_zero();
                let key = hmac::Key::new(hmac::HMAC_SHA256, secret);
                hmac::sign(&key, signed.as_bytes()).as_ref().to_vec()
            }
            Sign::UnpublishedKey => {
                let other = KeyPair::generate(KeySize::Rsa2048).expect("an RSA key");
                rsa_sign(&other, &signed)
            }
            Sign::Rs256NamingTheEcKey => rsa_sign(&state.rsa, &signed),
            Sign::Rs256 | Sign::BadSignature => {
                let mut signature = rsa_sign(&state.rsa, &signed);
                if sign == Sign::BadSignature {
                    signature[10] ^= 0x01;
                }
                signature
            }
        });
        format!("{signed}.{}", b64(&signature))
    }

    /// A config for this issuer that admits `admit` and accepts RS256 and
    /// ES256.
    pub fn config(&self, admit: Admit) -> ProxyConfig {
        ProxyConfig::new(
            ProxyIssuer::try_from(ISSUER).expect("an issuer"),
            Audience::try_from(AUDIENCE).expect("an audience"),
            KeySetUrl::loopback_http(&self.keys_url).expect("a key set URL"),
            CookieName::try_from(COOKIE).expect("a cookie name"),
            ProxyAlgorithms::new(&[ProxyAlgorithm::Rs256, ProxyAlgorithm::Es256])
                .expect("algorithms"),
            admit,
        )
    }

    /// Connects an authenticator to this issuer under the allow-list, with
    /// a clock the test moves.
    pub async fn connect(&self) -> (ProxyAuthenticator<TestClock>, TestClock) {
        self.connect_config(self.config(Admit::Emails(allow_list())))
            .await
    }

    /// Connects an authenticator with `config` and a clock the test moves.
    pub async fn connect_config(
        &self,
        config: ProxyConfig,
    ) -> (ProxyAuthenticator<TestClock>, TestClock) {
        let clock = TestClock::new();
        let authenticator = ProxyAuthenticator::connect_with_clock(config, clock.clone())
            .await
            .expect("the authenticator connects");
        (authenticator, clock)
    }
}

fn rsa_sign(key: &KeyPair, signed: &str) -> Vec<u8> {
    let mut signature = vec![0; key.public_modulus_len()];
    key.sign(
        &RSA_PKCS1_SHA256,
        &SystemRandom::new(),
        signed.as_bytes(),
        &mut signature,
    )
    .expect("a signature");
    signature
}

fn jwks(state: &State) -> Value {
    let mut keys = Vec::new();
    if state.published.contains(&RSA_KID) {
        let public = state.rsa.public_key();
        keys.push(json!({
            "kty": "RSA",
            "use": "sig",
            "alg": "RS256",
            "kid": RSA_KID,
            "n": b64(public.modulus().big_endian_without_leading_zero()),
            "e": b64(public.exponent().big_endian_without_leading_zero()),
        }));
    }
    if state.published.contains(&EC_KID) {
        // The public key is the uncompressed point: 0x04, then x, then y.
        let point = state.ec.public_key().as_ref();
        keys.push(json!({
            "kty": "EC",
            "use": "sig",
            "alg": "ES256",
            "crv": "P-256",
            "kid": EC_KID,
            "x": b64(&point[1..33]),
            "y": b64(&point[33..65]),
        }));
    }
    json!({ "keys": keys })
}

async fn handle(
    shared: Arc<Mutex<State>>,
    request: Request<Incoming>,
) -> Result<Response<Full<Bytes>>, Infallible> {
    let mut state = shared.lock().expect("the fake's lock");
    if request.uri().path() != "/jwks" {
        return Ok(reply(StatusCode::NOT_FOUND, &json!({})));
    }
    state.fetches += 1;
    Ok(match &state.reply {
        KeysReply::Normal => reply(StatusCode::OK, &jwks(&state)),
        KeysReply::Down => reply(StatusCode::INTERNAL_SERVER_ERROR, &json!({})),
        KeysReply::Json(document) => reply(StatusCode::OK, document),
    })
}

/// The page that the admitted person may reach, as a resource.
pub fn home() -> Resource {
    Resource::Page(PageName::try_from(HOME).expect("a page name"))
}

/// The allow-list of the tests: [`EMAIL`] as [`PRINCIPAL`], reaching the
/// page [`HOME`] only.
pub fn allow_list() -> AllowList {
    AllowList::new(vec![AllowEntry::new(
        Email::try_from(EMAIL).expect("an email").into(),
        PrincipalId::try_from(PRINCIPAL).expect("a principal id"),
        Reach::Only(vec![home()]),
    )])
    .expect("an allow-list")
}

/// Every address of [`DOMAIN`], reaching the page [`HOME`] only.
pub fn domain_rule() -> Admit {
    Admit::Domain(DomainRule::new(
        WorkspaceDomain::try_from(DOMAIN).expect("a domain"),
        Reach::Only(vec![home()]),
    ))
}
