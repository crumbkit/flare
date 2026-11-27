//! A hosted test server with the share API on memory stores: the fake
//! provider admits every account of `example.com`, and the creators are
//! alice, bob and carol. Each person gets a CLI token the way `flare login`
//! does: a browser session, `/cli/login`, then the code exchange.

use crate::faulty::{
    Faults, FaultyCreators, FaultyRecipients, FaultyResponses, FaultyShares, FaultyTokens,
};
use crate::harness::{Call, Reply, cookie_pair};
use crate::hosted::{Admits, Hosted, HostedApp, SESSION};
use flare_auth_oidc::{Admit, DomainRule, Reach, WorkspaceDomain};
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryFileStore, MemoryGrantStore, MemoryResponseStore,
    MemorySealingKeys, MemorySequence, MemoryTeamStore,
};
use flare_core::{
    CreatorList, DocumentText, Email, FlareTeams, Flares, Principal, ShareContent, ShareId, Shares,
    Teams, Timestamp,
};
use flare_server::{ContentHost, GetFlare, PublicOrigin, Sharing};
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
    /// A server whose creators are alice, bob and carol at example.com,
    /// under an admit rule that admits anyone.
    pub async fn start() -> Self {
        Self::start_full(Admit::Anyone(Reach::Everything), |app| app).await
    }

    /// As [`Api::start`], with the share API's admit rule `admit`.
    pub async fn start_admitting(admit: Admit) -> Self {
        Self::start_full(admit, |app| app).await
    }

    /// The server of [`Api::start`], whose app `extra` finishes after the
    /// share API is added, such as by turning metrics on.
    pub async fn start_with(extra: impl FnOnce(HostedApp) -> HostedApp) -> Self {
        Self::launch(Admit::Anyone(Reach::Everything), None, false, |app, _| {
            extra(app)
        })
        .await
    }

    /// The server of [`Api::start`] with a content host named
    /// `content.localhost` on its port, and an HTML-capable share API.
    pub async fn start_with_content_host() -> Self {
        Self::launch(
            Admit::Anyone(Reach::Everything),
            None,
            false,
            |app, port| {
                let origin =
                    PublicOrigin::loopback_http(&format!("http://content.localhost:{port}"))
                        .unwrap();
                app.with_content_host(ContentHost::new(origin, MemorySealingKeys::new()))
            },
        )
        .await
    }

    /// The server of [`Api::start_with_domain`], whose teams come from
    /// `teams` in place of flare's own store.
    pub async fn start_with_teams(teams: impl Teams + 'static) -> Self {
        let domain = WorkspaceDomain::try_from("example.com").unwrap();
        let admit = Admit::Domain(DomainRule::new(domain, Reach::Everything));
        Self::launch_with(admit, None, false, teams, |app, _| app).await
    }

    /// The server of [`Api::start`] with `hosted.everyone_signed_in` on, so
    /// an everyone grant opens to every signed-in person.
    pub async fn start_everyone_signed_in() -> Self {
        Self::launch(Admit::Anyone(Reach::Everything), None, true, |app, _| app).await
    }

    /// The same server under an admit rule of the domain example.com.
    pub async fn start_with_domain() -> Self {
        let domain = WorkspaceDomain::try_from("example.com").unwrap();
        Self::start_admitting(Admit::Domain(DomainRule::new(domain, Reach::Everything))).await
    }

    /// The server of [`Api::start`] with the share API's admit rule `admit`,
    /// whose app `extra` finishes after the share API is added.
    pub async fn start_full(admit: Admit, extra: impl FnOnce(HostedApp) -> HostedApp) -> Self {
        Self::launch(admit, None, false, |app, _| extra(app)).await
    }

    /// The server of [`Api::start`], whose reading pages link readers who
    /// have never shared to `get_flare`.
    pub async fn with_get_flare(get_flare: &str) -> Self {
        let address = GetFlare::try_from(get_flare).unwrap();
        Self::launch(
            Admit::Anyone(Reach::Everything),
            Some(address),
            false,
            |app, _| app,
        )
        .await
    }

    async fn launch(
        admit: Admit,
        get_flare: Option<GetFlare>,
        everyone_signed_in: bool,
        extra: impl FnOnce(HostedApp, u16) -> HostedApp,
    ) -> Self {
        let teams = FlareTeams::new(MemoryTeamStore::new());
        Self::launch_with(admit, get_flare, everyone_signed_in, teams, extra).await
    }

    async fn launch_with(
        admit: Admit,
        get_flare: Option<GetFlare>,
        everyone_signed_in: bool,
        teams: impl Teams + 'static,
        extra: impl FnOnce(HostedApp, u16) -> HostedApp,
    ) -> Self {
        let faults = Faults::default();
        let switches = faults.clone();
        let shares = FaultyShares {
            fail: faults.shares.clone(),
            ..FaultyShares::default()
        };
        let store = shares.clone();
        let hosted = Hosted::start_attached_at(Admits::Domain, None, move |app, port| {
            let admit = admit.clone();
            let creators = FaultyCreators {
                fail: switches.creators.clone(),
                inner: CreatorList::only(
                    ["alice", "bob", "carol"]
                        .iter()
                        .map(|name| {
                            Email::try_from(format!("{name}@example.com").as_str())
                                .unwrap()
                                .into()
                        })
                        .collect(),
                ),
            };
            let sequence = MemorySequence::new();
            let flares = Flares::new(
                store,
                MemoryGrantStore::new(),
                FaultyResponses {
                    fail: switches.responses.clone(),
                    inner: MemoryResponseStore::with_sequence(sequence.clone()),
                    fail_index: switches.index.clone(),
                },
                MemoryCommentStore::with_sequence(sequence),
                MemoryCursorStore::new(),
                creators,
            )
            .with_teams(teams);
            let tokens = FaultyTokens {
                fail: switches.tokens.clone(),
                ..FaultyTokens::default()
            };
            let recipients = FaultyRecipients {
                fail: switches.recipients.clone(),
            };
            let sharing = Sharing::new(flares, tokens, recipients, admit)
                .with_everyone_signed_in(everyone_signed_in)
                .with_files(MemoryFileStore::new());
            let sharing = match get_flare {
                Some(address) => sharing.with_get_flare(address),
                None => sharing,
            };
            extra(app.with_sharing(sharing), port)
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

    /// A request as a signed-in browser's script sends it: the session
    /// `cookie`, the same-origin headers and, for a body, JSON.
    pub fn browser(&self, method: &str, path: &str, cookie: &str) -> Call {
        self.hosted
            .request(method, path)
            .cookie(cookie)
            .same_origin()
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
