//! A hosted test server with the share API on memory stores: the fake
//! provider admits every account of `example.com`, and the creators are
//! alice, bob and carol. Each person gets a CLI token the way `flare login`
//! does: a browser session, `/cli/login`, then the code exchange.

use crate::faulty::{Faults, FaultyRecipients, FaultyResponses, FaultyShares, FaultyTokens};
use crate::harness::{Call, Reply, cookie_pair};
use crate::hosted::{Admits, Hosted, SESSION};
use flare_auth_oidc::{Admit, Reach};
use flare_core::memory::MemoryGrantStore;
use flare_core::{Creators, Email, Flares};
use flare_server::Sharing;
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
}

impl Api {
    /// A server whose creators are alice, bob and carol at example.com.
    pub async fn start() -> Self {
        let faults = Faults::default();
        let switches = faults.clone();
        let hosted = Hosted::start_attached(Admits::Domain, None, move |app| {
            let creators = Creators::only(
                ["alice", "bob", "carol"]
                    .iter()
                    .map(|name| Email::try_from(format!("{name}@example.com").as_str()).unwrap())
                    .collect(),
            );
            let flares = Flares::new(
                FaultyShares {
                    fail: switches.shares.clone(),
                    ..FaultyShares::default()
                },
                MemoryGrantStore::new(),
                FaultyResponses {
                    fail: switches.responses.clone(),
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
            app.with_sharing(Sharing::new(flares, tokens, recipients, admit))
        })
        .await;
        Self { hosted, faults }
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
