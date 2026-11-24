//! A fake OpenID provider that runs inside the test process, and the helpers
//! that sign a person in against it.
//!
//! The fake serves discovery, authorisation, token and key set endpoints over
//! HTTP on 127.0.0.1, with a new RSA key for each test. Its token endpoint
//! checks the code, the PKCE verifier, the client ID, the client secret and
//! the redirect URI, accepts each code once and signs an ID token. Each test
//! sets what the fake does wrong.

use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::rsa::{KeyPair, KeySize};
use aws_lc_rs::signature::{KeyPair as _, RSA_PKCS1_SHA256};
use aws_lc_rs::{digest, hmac};
use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use flare_auth_oidc::{
    Admit, AllowEntry, AllowList, ClientId, DomainRule, Email, FixedSecret, IssuerUrl, KeptState,
    OidcAuthenticator, OidcConfig, Reach, RedirectUri, ReturnTo, SignInRefusal, SignedIn,
    TestClock, WorkspaceDomain,
};
use flare_core::{Credential, PageName, PrincipalId, Resource};
use http_body_util::{BodyExt as _, Full};
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use secrecy::SecretString;
use serde_json::{Map, Value, json};
use std::collections::{HashMap, HashSet};
use std::convert::Infallible;
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::net::TcpListener;
use tokio::sync::Notify;
use url::Url;

/// The client ID that the fake knows.
pub const CLIENT_ID: &str = "flare-test-client";
/// The client secret that the fake knows.
pub const CLIENT_SECRET: &str = "fake-client-secret-7f3a";
/// The redirect URI that the fake's client is registered with.
pub const REDIRECT: &str = "http://localhost:8740/auth/oidc/callback";
/// The email of the person on the allow-list.
pub const EMAIL: &str = "ada@example.com";
/// The principal that the allow-list gives that person.
pub const PRINCIPAL: &str = "ada";
/// The page that the allow-listed person may reach.
pub const HOME: &str = "home";
/// The Google Workspace domain of the tests' domain rule.
pub const DOMAIN: &str = "example.com";

/// What the token endpoint replies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenReply {
    /// A good reply with an ID token.
    Normal,
    /// A reply with this status and an empty JSON object.
    Status(u16),
    /// No reply at all, so the client's deadline passes.
    Stall,
    /// No reply until a test sets another token reply; each held request
    /// then answers as that reply says.
    Hold,
    /// A 200 reply whose JSON has an access token and no ID token.
    NoIdToken,
}

/// What the key set endpoint replies.
#[derive(Debug, Clone, PartialEq)]
pub enum KeysReply {
    /// Every key the fake has signed with or published.
    Normal,
    /// A key set with no keys.
    Empty,
    /// This JSON document.
    Json(Value),
    /// A 500 reply.
    Down,
    /// No reply until a test sets another key set reply; each held request
    /// then answers as that reply says.
    Hold,
}

/// How the fake signs the next ID tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forge {
    /// RS256 with the current published key.
    None,
    /// `alg: none` and no signature.
    AlgNone,
    /// HS256 keyed with the client secret.
    Hs256,
    /// RS256 with a key the key set does not publish.
    UnpublishedKey,
    /// RS256 with the current key, and one byte of the signature changed.
    BadSignature,
}

type ClaimsEdit = Box<dyn Fn(&mut Map<String, Value>) + Send>;

struct Grant {
    nonce: String,
    challenge: String,
    redirect_uri: String,
    client_id: String,
}

/// A request that the fake received.
#[derive(Debug, Clone)]
pub struct Seen {
    /// The method, such as `POST`.
    pub method: String,
    /// The path.
    pub path: String,
    /// The query, or an empty string.
    pub query: String,
    /// Every header, as `name: value` lines.
    pub headers: String,
    /// The body as text.
    pub body: String,
}

struct State {
    issuer: String,
    metadata: Value,
    keys: Vec<(String, KeyPair)>,
    signing: usize,
    unpublished: Option<(String, KeyPair)>,
    keys_reply: KeysReply,
    token_reply: TokenReply,
    forge: Forge,
    email: String,
    hd: Option<Value>,
    claims_edit: Option<ClaimsEdit>,
    grants: HashMap<String, Grant>,
    redeemed: HashSet<String>,
    next_code: u64,
    seen: Vec<Seen>,
    released: Arc<Notify>,
}

/// A running fake provider.
#[derive(Clone)]
pub struct Fake {
    state: Arc<Mutex<State>>,
    issuer: String,
}

impl std::fmt::Debug for Fake {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Fake")
            .field("issuer", &self.issuer)
            .finish_non_exhaustive()
    }
}

fn new_key() -> KeyPair {
    KeyPair::generate(KeySize::Rsa2048).expect("an RSA key")
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

impl Fake {
    /// Starts a fake on a new port of 127.0.0.1, on the current Tokio
    /// runtime.
    pub async fn start() -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.expect("a port");
        let issuer = format!("http://{}", listener.local_addr().expect("an address"));
        let metadata = json!({
            "issuer": issuer,
            "authorization_endpoint": format!("{issuer}/authorize"),
            "token_endpoint": format!("{issuer}/token"),
            "jwks_uri": format!("{issuer}/keys"),
            "response_types_supported": ["code"],
            "subject_types_supported": ["public"],
            "id_token_signing_alg_values_supported": ["RS256"],
            "code_challenge_methods_supported": ["plain", "S256"],
        });
        let state = Arc::new(Mutex::new(State {
            issuer: issuer.clone(),
            metadata,
            keys: vec![("key-1".to_owned(), new_key())],
            signing: 0,
            unpublished: None,
            keys_reply: KeysReply::Normal,
            token_reply: TokenReply::Normal,
            forge: Forge::None,
            email: EMAIL.to_owned(),
            hd: None,
            claims_edit: None,
            grants: HashMap::new(),
            redeemed: HashSet::new(),
            next_code: 0,
            seen: Vec::new(),
            released: Arc::new(Notify::new()),
        }));
        let shared = Arc::clone(&state);
        #[expect(
            clippy::disallowed_methods,
            reason = "the fake provider's tasks end with the test's runtime"
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
        Self { state, issuer }
    }

    fn with<T>(&self, change: impl FnOnce(&mut State) -> T) -> T {
        change(&mut self.state.lock().expect("the fake's lock"))
    }

    /// The fake's issuer URL.
    pub fn issuer(&self) -> &str {
        &self.issuer
    }

    /// Changes the discovery document.
    pub fn edit_metadata(&self, edit: impl FnOnce(&mut Map<String, Value>)) {
        self.with(|state| {
            if let Value::Object(map) = &mut state.metadata {
                edit(map);
            }
        });
    }

    /// Changes every later ID token's claims.
    pub fn edit_claims(&self, edit: impl Fn(&mut Map<String, Value>) + Send + 'static) {
        self.with(|state| state.claims_edit = Some(Box::new(edit)));
    }

    /// Sets the email of every later ID token.
    pub fn set_email(&self, email: &str) {
        self.with(|state| state.email = email.to_owned());
    }

    /// Sets the `hd` claim of every later ID token: `None` leaves the claim
    /// out, and `Some` sets it to any JSON value, a string or not.
    pub fn set_hd(&self, hd: Option<Value>) {
        self.with(|state| state.hd = hd);
    }

    /// Sets how the fake signs later ID tokens.
    pub fn forge(&self, forge: Forge) {
        self.with(|state| state.forge = forge);
    }

    /// Sets what the token endpoint replies. Requests held by
    /// [`TokenReply::Hold`] then answer with `reply`.
    pub fn token_reply(&self, reply: TokenReply) {
        let released = self.with(|state| {
            state.token_reply = reply;
            Arc::clone(&state.released)
        });
        released.notify_waiters();
    }

    /// Sets what the key set endpoint replies. Requests held by
    /// [`KeysReply::Hold`] then answer with `reply`.
    pub fn keys_reply(&self, reply: KeysReply) {
        let released = self.with(|state| {
            state.keys_reply = reply;
            Arc::clone(&state.released)
        });
        released.notify_waiters();
    }

    /// Publishes a new key and signs later tokens with it.
    pub fn rotate_key(&self) {
        self.with(|state| {
            let kid = format!("key-{}", state.keys.len() + 1);
            state.keys.push((kid, new_key()));
            state.signing = state.keys.len() - 1;
        });
    }

    /// Makes the unpublished key that [`Forge::UnpublishedKey`] signs with a
    /// new one, under a new key id.
    pub fn new_unpublished_key(&self) {
        self.with(|state| {
            let kid = format!("unpublished-{}", state.next_code);
            state.next_code += 1;
            state.unpublished = Some((kid, new_key()));
        });
    }

    /// How many requests reached the key set endpoint.
    pub fn key_set_requests(&self) -> usize {
        self.count("/keys")
    }

    /// How many requests reached the token endpoint.
    pub fn token_requests(&self) -> usize {
        self.count("/token")
    }

    fn count(&self, path: &str) -> usize {
        self.with(|state| state.seen.iter().filter(|seen| seen.path == path).count())
    }

    /// Every request the fake received.
    pub fn seen(&self) -> Vec<Seen> {
        self.with(|state| state.seen.clone())
    }

    /// Plays the person approving the sign-in at the provider: reads the
    /// authorisation URL, grants a code and returns the callback's query.
    pub fn approve(&self, authorization: &Url) -> String {
        self.with(|state| approve(state, authorization))
    }

    /// Plays the person refusing the sign-in at the provider.
    pub fn deny(&self, authorization: &Url) -> String {
        let state = param(authorization, "state").expect("a state parameter");
        form(&[("error", "access_denied"), ("state", &state)])
    }
}

fn param(url: &Url, name: &str) -> Option<String> {
    url.query_pairs()
        .find(|(key, _)| key == name)
        .map(|(_, value)| value.into_owned())
}

/// Encodes pairs as a form query.
pub fn form(pairs: &[(&str, &str)]) -> String {
    url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(pairs)
        .finish()
}

fn approve(state: &mut State, authorization: &Url) -> String {
    let get = |name| param(authorization, name).unwrap_or_default();
    let code = format!("code-{}", state.next_code);
    state.next_code += 1;
    state.grants.insert(
        code.clone(),
        Grant {
            nonce: get("nonce"),
            challenge: get("code_challenge"),
            redirect_uri: get("redirect_uri"),
            client_id: get("client_id"),
        },
    );
    form(&[("code", &code), ("state", &get("state"))])
}

fn reply(status: StatusCode, body: Value) -> Response<Full<Bytes>> {
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
        .map(|b| b.to_bytes())
        .unwrap_or_default();
    let seen = Seen {
        method: parts.method.to_string(),
        path: parts.uri.path().to_owned(),
        query: parts.uri.query().unwrap_or_default().to_owned(),
        headers: parts
            .headers
            .iter()
            .map(|(name, value)| format!("{name}: {}\n", value.to_str().unwrap_or("?")))
            .collect(),
        body: String::from_utf8_lossy(&body).into_owned(),
    };
    let to_token = seen.path == "/token";
    let to_keys = seen.path == "/keys";
    let (stall, released) = {
        let mut state = shared.lock().expect("the fake's lock");
        state.seen.push(seen.clone());
        (
            to_token && state.token_reply == TokenReply::Stall,
            Arc::clone(&state.released),
        )
    };
    if stall {
        std::future::pending::<()>().await;
    }
    if to_token || to_keys {
        loop {
            // The waiter is made before the reply is read, so a release
            // between the two still wakes it.
            let release = released.notified();
            let held = {
                let state = shared.lock().expect("the fake's lock");
                (to_token && state.token_reply == TokenReply::Hold)
                    || (to_keys && state.keys_reply == KeysReply::Hold)
            };
            if !held {
                break;
            }
            release.await;
        }
    }
    let mut state = shared.lock().expect("the fake's lock");
    Ok(match seen.path.as_str() {
        "/.well-known/openid-configuration" => reply(StatusCode::OK, state.metadata.clone()),
        "/keys" => keys(&state),
        "/token" => token(&mut state, &seen.body),
        "/authorize" => {
            let url =
                Url::parse(&format!("{}/authorize?{}", state.issuer, seen.query)).expect("a URL");
            let query = approve(&mut state, &url);
            let target = param(&url, "redirect_uri").unwrap_or_default();
            let mut response = reply(StatusCode::FOUND, json!({}));
            let location = format!("{target}?{query}").parse().expect("a header value");
            response.headers_mut().insert("location", location);
            response
        }
        _ => reply(StatusCode::NOT_FOUND, json!({})),
    })
}

fn jwk(kid: &str, key: &KeyPair) -> Value {
    let public = key.public_key();
    json!({
        "kty": "RSA",
        "use": "sig",
        "alg": "RS256",
        "kid": kid,
        "n": b64(public.modulus().big_endian_without_leading_zero()),
        "e": b64(public.exponent().big_endian_without_leading_zero()),
    })
}

fn keys(state: &State) -> Response<Full<Bytes>> {
    match &state.keys_reply {
        KeysReply::Normal | KeysReply::Hold => {
            let keys: Vec<Value> = state.keys.iter().map(|(kid, key)| jwk(kid, key)).collect();
            reply(StatusCode::OK, json!({ "keys": keys }))
        }
        KeysReply::Empty => reply(StatusCode::OK, json!({ "keys": [] })),
        KeysReply::Json(document) => reply(StatusCode::OK, document.clone()),
        KeysReply::Down => reply(StatusCode::INTERNAL_SERVER_ERROR, json!({})),
    }
}

/// The key set of one RSA key, for tests that edit it.
pub fn one_key_set(edit: impl FnOnce(&mut Map<String, Value>)) -> Value {
    let mut key = jwk("key-x", &new_key());
    if let Value::Object(map) = &mut key {
        edit(map);
    }
    json!({ "keys": [key] })
}

fn token(state: &mut State, body: &str) -> Response<Full<Bytes>> {
    match state.token_reply {
        TokenReply::Status(status) => {
            let status = StatusCode::from_u16(status).expect("a status");
            return reply(status, json!({}));
        }
        TokenReply::NoIdToken => {
            return reply(
                StatusCode::OK,
                json!({ "access_token": "at", "token_type": "Bearer" }),
            );
        }
        TokenReply::Normal | TokenReply::Stall | TokenReply::Hold => {}
    }
    let fields: HashMap<String, String> = url::form_urlencoded::parse(body.as_bytes())
        .into_owned()
        .collect();
    let field = |name: &str| fields.get(name).map(String::as_str).unwrap_or_default();
    let refused = reply(StatusCode::BAD_REQUEST, json!({ "error": "invalid_grant" }));
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
        && grant.client_id == CLIENT_ID
        && field("redirect_uri") == grant.redirect_uri
        && challenge == grant.challenge;
    if !good {
        return reply(
            StatusCode::UNAUTHORIZED,
            json!({ "error": "invalid_client" }),
        );
    }
    let nonce = grant.nonce.clone();
    state.redeemed.insert(code);
    let id_token = mint(state, &nonce);
    reply(
        StatusCode::OK,
        json!({
            "access_token": "fake-access-token",
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": id_token,
        }),
    )
}

fn mint(state: &State, nonce: &str) -> String {
    let now = now_seconds();
    let mut claims = Map::new();
    claims.insert("iss".into(), json!(state.issuer));
    claims.insert("aud".into(), json!(CLIENT_ID));
    claims.insert("azp".into(), json!(CLIENT_ID));
    claims.insert("sub".into(), json!("1234567890"));
    claims.insert("email".into(), json!(state.email));
    claims.insert("email_verified".into(), json!(true));
    claims.insert("nonce".into(), json!(nonce));
    if let Some(hd) = &state.hd {
        claims.insert("hd".into(), hd.clone());
    }
    claims.insert("iat".into(), json!(now));
    // Two days ahead, so a test that moves its clock by hours still holds a
    // token that has not expired.
    claims.insert("exp".into(), json!(now + 2 * 24 * 60 * 60));
    if let Some(edit) = &state.claims_edit {
        edit(&mut claims);
    }
    let (kid, key) = match (&state.forge, &state.unpublished) {
        (Forge::UnpublishedKey, Some((kid, key))) => (kid.as_str(), key),
        _ => {
            let (kid, key) = &state.keys[state.signing];
            (kid.as_str(), key)
        }
    };
    let alg = match state.forge {
        Forge::AlgNone => "none",
        Forge::Hs256 => "HS256",
        _ => "RS256",
    };
    let header = json!({ "alg": alg, "kid": kid, "typ": "JWT" });
    let signed = format!(
        "{}.{}",
        b64(header.to_string().as_bytes()),
        b64(Value::Object(claims).to_string().as_bytes())
    );
    let signature = match state.forge {
        Forge::AlgNone => Vec::new(),
        Forge::Hs256 => {
            let hmac_key = hmac::Key::new(hmac::HMAC_SHA256, CLIENT_SECRET.as_bytes());
            hmac::sign(&hmac_key, signed.as_bytes()).as_ref().to_vec()
        }
        _ => {
            let mut signature = vec![0; key.public_modulus_len()];
            key.sign(
                &RSA_PKCS1_SHA256,
                &SystemRandom::new(),
                signed.as_bytes(),
                &mut signature,
            )
            .expect("a signature");
            if state.forge == Forge::BadSignature {
                signature[10] ^= 0x01;
            }
            signature
        }
    };
    format!("{signed}.{}", b64(&signature))
}

/// The allow-list of the tests: [`EMAIL`] as [`PRINCIPAL`], reaching the
/// page [`HOME`] only.
pub fn allow_list() -> AllowList {
    let home = Resource::Page(PageName::try_from(HOME).expect("a page name"));
    AllowList::new(vec![AllowEntry::new(
        Email::try_from(EMAIL).expect("an email"),
        PrincipalId::try_from(PRINCIPAL).expect("a principal id"),
        Reach::Only(vec![home]),
    )])
    .expect("an allow-list")
}

/// The domain rule of the tests: every verified account of [`DOMAIN`],
/// reaching the page [`HOME`] only.
pub fn domain_rule() -> Admit {
    let home = Resource::Page(PageName::try_from(HOME).expect("a page name"));
    let domain = WorkspaceDomain::try_from(DOMAIN).expect("a domain");
    Admit::Domain(DomainRule::new(domain, Reach::Only(vec![home])))
}

/// A config for the fake that admits the allow-list, with a two second
/// request deadline.
pub fn config(fake: &Fake) -> OidcConfig {
    config_with(fake, Admit::Emails(allow_list()))
}

/// A config for the fake that admits the domain rule.
pub fn domain_config(fake: &Fake) -> OidcConfig {
    config_with(fake, domain_rule())
}

/// A config for the fake with another rule of who may sign in.
pub fn config_with(fake: &Fake, admit: Admit) -> OidcConfig {
    OidcConfig::new(
        IssuerUrl::loopback_http(fake.issuer()).expect("a loopback issuer"),
        ClientId::try_from(CLIENT_ID).expect("a client id"),
        RedirectUri::try_from(REDIRECT).expect("a redirect URI"),
        admit,
    )
    .with_request_deadline(Duration::from_secs(2))
}

/// The client secret that the fake knows.
pub fn secret() -> FixedSecret {
    FixedSecret::new(SecretString::from(CLIENT_SECRET))
}

/// Connects an authenticator with a test clock to the fake.
pub async fn connect(fake: &Fake) -> (OidcAuthenticator<TestClock>, TestClock) {
    let clock = TestClock::new();
    let authenticator =
        OidcAuthenticator::connect_with_clock(config(fake), &secret(), clock.clone())
            .await
            .expect("connect succeeds against the fake");
    (authenticator, clock)
}

/// Connects an authenticator with a test clock to the fake, with
/// `deadline` for each request to it in place of two seconds, for a test
/// that holds token requests longer than that.
pub async fn connect_with_deadline(
    fake: &Fake,
    deadline: Duration,
) -> (OidcAuthenticator<TestClock>, TestClock) {
    let clock = TestClock::new();
    let config = config(fake).with_request_deadline(deadline);
    let authenticator = OidcAuthenticator::connect_with_clock(config, &secret(), clock.clone())
        .await
        .expect("connect succeeds against the fake");
    (authenticator, clock)
}

/// Connects an authenticator with a test clock and the domain rule to the
/// fake.
pub async fn connect_domain(fake: &Fake) -> (OidcAuthenticator<TestClock>, TestClock) {
    let clock = TestClock::new();
    let authenticator =
        OidcAuthenticator::connect_with_clock(domain_config(fake), &secret(), clock.clone())
            .await
            .expect("connect succeeds against the fake");
    (authenticator, clock)
}

/// A return-to value.
pub fn return_to() -> ReturnTo {
    ReturnTo::try_from("/pages/home").expect("a return-to value")
}

/// Starts a sign-in, approves it at the fake and completes it.
pub async fn sign_in(
    authenticator: &OidcAuthenticator<TestClock>,
    fake: &Fake,
) -> Result<SignedIn, SignInRefusal> {
    sign_in_replacing(authenticator, fake, None).await
}

/// Starts a sign-in, approves it at the fake and completes it in a browser
/// that holds the session `old`.
pub async fn sign_in_replacing(
    authenticator: &OidcAuthenticator<TestClock>,
    fake: &Fake,
    old: Option<&Credential>,
) -> Result<SignedIn, SignInRefusal> {
    let start = authenticator.start(return_to()).await.expect("a start");
    let query = fake.approve(start.redirect());
    let kept = KeptState::from_cookie(start.kept().expose());
    authenticator.complete(&query, Some(&kept), old).await
}

/// Asserts that a sign-in was refused with `expected`.
pub fn assert_refused(result: Result<SignedIn, SignInRefusal>, expected: &SignInRefusal) {
    match result {
        Ok(_) => panic!("the sign-in succeeded, and {expected:?} was expected"),
        Err(refusal) => assert_eq!(&refusal, expected),
    }
}
