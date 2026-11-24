//! A hosted test server with the share API on memory stores: the fake
//! provider admits every account of `example.com`, and the creators are
//! alice, bob and carol. Each person gets a CLI token the way `flare login`
//! does: a browser session, `/cli/login`, then the code exchange.

use crate::faulty::{Faults, FaultyRecipients, FaultyResponses, FaultyShares, FaultyTokens};
use crate::harness::{Call, Reply, cookie_pair};
use crate::hosted::{Admits, Hosted, HostedApp, SESSION};
use flare_auth_oidc::{Admit, Reach};
use flare_core::memory::MemoryGrantStore;
use flare_core::{
    Creators, DocumentText, Email, Flares, Principal, ShareContent, ShareId, Shares, Timestamp,
};
use flare_server::{GetFlare, Sharing};
use serde_json::{Value, json};
use url::Url;

/// The loopback port the tests name as the CLI's.
pub const CLI_PORT: u16 = 47_123;

/// The state the tests name as the CLI's.
pub const STATE: &str = "teststate0123456789abcdef";

/// A hosted server with the share API.
#[derive(Debug)]
pub struct Api {
    /// The hosted server.
    pub hosted: Hosted,
    /// The stores' fault switches.
    pub faults: Faults,
    /// The share store the API keeps its shares in.
    pub shares: FaultyShares,
}

impl Api {
    /// A server whose creators are alice, bob and carol at example.com.
    pub async fn start() -> Self {
        Self::start_with(|app| app).await
    }

    /// The server of [`Api::start`], whose app `extra` finishes after the
    /// share API is added, such as by turning metrics on.
    pub async fn start_with(extra: impl FnOnce(HostedApp) -> HostedApp) -> Self {
        Self::launch(None, extra).await
    }

    /// The server of [`Api::start`], whose reading pages link readers who
    /// have never shared to `get_flare`.
    pub async fn with_get_flare(get_flare: &str) -> Self {
        let address = GetFlare::try_from(get_flare).unwrap();
        Self::launch(Some(address), |app| app).await
    }

    async fn launch(
        get_flare: Option<GetFlare>,
        extra: impl FnOnce(HostedApp) -> HostedApp,
    ) -> Self {
        let faults = Faults::default();
        let switches = faults.clone();
        let shares = FaultyShares {
            fail: faults.shares.clone(),
            ..FaultyShares::default()
        };
        let store = shares.clone();
        let hosted = Hosted::start_attached(Admits::Domain, None, move |app| {
            let creators = Creators::only(
                ["alice", "bob", "carol"]
                    .iter()
                    .map(|name| Email::try_from(format!("{name}@example.com").as_str()).unwrap())
                    .collect(),
            );
            let flares = Flares::new(
                store,
                MemoryGrantStore::new(),
                FaultyResponses {
                    fail: switches.responses.clone(),
                    fail_index: switches.index.clone(),
                    ..FaultyResponses::default()
                },
                creators,
            );
            let tokens = FaultyTokens {
                fail: switches.tokens.clone(),
                ..FaultyTokens::default()
            };
            let recipients = FaultyRecipients {
                fail: switches.recipients.clone(),
            };
            let admit = Admit::Anyone(Reach::Everything);
            let sharing = Sharing::new(flares, tokens, recipients, admit);
            let sharing = match get_flare {
                Some(address) => sharing.with_get_flare(address),
                None => sharing,
            };
            extra(app.with_sharing(sharing))
        })
        .await;
        Self {
            hosted,
            faults,
            shares,
        }
    }

    /// Publishes `markdown` as the next version of share `id`, as its owner
    /// `<owner>@example.com`.
    pub async fn publish(&self, owner: &str, id: &str, markdown: &str) {
        let email = Email::try_from(format!("{owner}@example.com").as_str()).unwrap();
        let who = Principal::with_email(email.principal_id(), email);
        Shares::new(self.shares.clone(), MemoryGrantStore::new())
            .publish(
                &who,
                &ShareId::try_from(id).unwrap(),
                ShareContent::Document(DocumentText::try_from(markdown).unwrap()),
                Timestamp::from_secs(1_795_219_200),
            )
            .await
            .unwrap();
    }

    /// Shares `markdown` titled `title` with `with`, which must succeed, and
    /// returns its id.
    pub async fn share_markdown(
        &self,
        token: &str,
        title: &str,
        markdown: &str,
        with: &[&str],
    ) -> String {
        let body = json!({
            "title": title,
            "format": "markdown",
            "content": markdown,
            "with": with,
            "re": [],
        });
        let (status, shared) = self.json("POST", "/api/shares", Some(token), body).await;
        assert_eq!(status, 201, "{shared}");
        shared["share"]["id"].as_str().unwrap().to_owned()
    }

    /// The origin, as `flare login` names it.
    pub fn origin(&self) -> &str {
        &self.hosted.origin
    }

    /// A browser session of `<name>@example.com`.
    pub async fn session(&self, name: &str) -> String {
        let cookie = self.hosted.sign_in_as(name).await;
        assert!(cookie.starts_with(SESSION));
        cookie
    }

    /// The reply to `/cli/login` with `cookie`, as the browser that `flare
    /// login` opened would get it.
    pub async fn cli_login(&self, cookie: Option<&str>) -> Reply {
        let call = self
            .hosted
            .get(&format!("/cli/login?port={CLI_PORT}&state={STATE}"));
        match cookie {
            Some(cookie) => call.cookie(cookie).send().await,
            None => call.send().await,
        }
    }

    /// A login code for `<name>@example.com`, from the redirect to the CLI.
    pub async fn code_for(&self, name: &str) -> String {
        let cookie = self.session(name).await;
        let reply = self.cli_login(Some(&cookie)).await;
        assert_eq!(reply.status, 303, "{}", reply.body);
        let location = Url::parse(reply.header("location").unwrap()).unwrap();
        assert_eq!(location.host_str(), Some("127.0.0.1"));
        assert_eq!(location.port(), Some(CLI_PORT));
        assert_eq!(location.path(), "/callback");
        let pairs: Vec<(String, String)> = location.query_pairs().into_owned().collect();
        assert!(pairs.contains(&("state".to_owned(), STATE.to_owned())));
        pairs
            .into_iter()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value)
            .unwrap()
    }

    /// Exchanges `code` for a token with `label`.
    pub async fn exchange(&self, code: &str, label: &str) -> (u16, Value) {
        self.json(
            "POST",
            "/api/tokens/exchange",
            None,
            json!({"code": code, "label": label}),
        )
        .await
    }

    /// The issued-token document for `<name>@example.com` with `label`.
    pub async fn issued(&self, name: &str, label: &str) -> Value {
        let code = self.code_for(name).await;
        let (status, issued) = self.exchange(&code, label).await;
        assert_eq!(status, 201, "{issued}");
        issued
    }

    /// A CLI token for `<name>@example.com`.
    pub async fn token_for(&self, name: &str) -> String {
        self.issued(name, "test").await["token"]
            .as_str()
            .unwrap()
            .to_owned()
    }

    /// A request, with the bearer `token` when given.
    pub fn call(&self, method: &str, path: &str, token: Option<&str>) -> Call {
        let call = self.hosted.request(method, path);
        match token {
            Some(token) => call.bearer(token),
            None => call,
        }
    }

    /// Sends a request with a JSON body and reads the JSON reply.
    pub async fn json(
        &self,
        method: &str,
        path: &str,
        token: Option<&str>,
        body: Value,
    ) -> (u16, Value) {
        let reply = self
            .call(method, path, token)
            .body("application/json", body.to_string().into_bytes())
            .send()
            .await;
        (reply.status, parse(&reply))
    }

    /// Sends a request with no body and reads the JSON reply.
    pub async fn read(&self, method: &str, path: &str, token: Option<&str>) -> (u16, Value) {
        let reply = self.call(method, path, token).send().await;
        (reply.status, parse(&reply))
    }

    /// Shares a Markdown flare titled `title`, which must succeed, and
    /// returns its id.
    pub async fn share(&self, token: &str, title: &str, with: &[&str], re: &[&str]) -> String {
        let (status, shared) = self
            .json(
                "POST",
                "/api/shares",
                Some(token),
                share_body(title, with, re),
            )
            .await;
        assert_eq!(status, 201, "{shared}");
        shared["share"]["id"].as_str().unwrap().to_owned()
    }
}

/// The body of a Markdown share.
pub fn share_body(title: &str, with: &[&str], re: &[&str]) -> Value {
    json!({
        "title": title,
        "format": "markdown",
        "content": format!("# {title}\n"),
        "with": with,
        "re": re,
    })
}

/// The reply's body as JSON, or `null`.
pub fn parse(reply: &Reply) -> Value {
    serde_json::from_str(&reply.body).unwrap_or(Value::Null)
}

/// The session cookie the reply sets.
pub fn session_of(reply: &Reply) -> Option<String> {
    cookie_pair(reply, SESSION)
}
