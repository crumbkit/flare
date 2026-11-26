//! A hosted test server behind a single sign-on proxy: the site signs
//! people in with the proxy's signed token, from the fake proxy issuer of
//! flare-auth-oidc's tests, and serves the share API on memory stores. The
//! admit rule is the fake's allow-list, which admits `ada@example.com`.

use crate::fake_proxy::{COOKIE, FakeProxy, HOME, allow_list};
use crate::faulty::{FaultyRecipients, FaultyShares, FaultyTokens};
use crate::harness::{
    Call, FixedSchemas, Pages, Records, cookie_pair, schemas, seed_page_in, stores,
};
use flare_auth_oidc::{Admit, ProxyAuthenticator, ProxyConfig, ProxyLink, TestClock};
use flare_core::memory::{
    MemoryCommentStore, MemoryCursorStore, MemoryGrantStore, MemoryResponseStore,
};
use flare_core::{CreatorList, Email, Flares};
use flare_server::{App, Limits, Listeners, PublicOrigin, ServeError, Sharing, Site, Stopped};
use serde_json::json;
use std::net::{Ipv4Addr, SocketAddr};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use url::Url;

/// The proxy's sign-in page that a reader with no cookie is linked to.
pub const SIGN_IN: &str = "https://sso.example.com/sign-in";
/// The proxy's sign-out address.
pub const SIGN_OUT: &str = "https://sso.example.com/sign-out";
/// The loopback port the tests name as the CLI's.
pub const CLI_PORT: u16 = 47_124;
/// The state the tests name as the CLI's.
pub const STATE: &str = "proxystate0123456789abcdef";

/// The app a proxied test server runs.
pub type ProxiedApp = App<Pages, Records, FixedSchemas, TestClock, ProxyAuthenticator<TestClock>>;

/// A running hosted server behind the fake proxy.
#[derive(Debug)]
pub struct Proxied {
    /// The Host that requests name: `localhost:<port>`.
    pub authority: String,
    /// The public origin: `http://localhost:<port>`.
    pub origin: String,
    /// The fake proxy issuer.
    pub fake: FakeProxy,
    port: u16,
    _shutdown: oneshot::Sender<()>,
    _task: JoinHandle<Result<Stopped, ServeError>>,
}

impl Proxied {
    /// A server whose proxy links to [`SIGN_IN`] and [`SIGN_OUT`].
    pub async fn start() -> Self {
        Self::start_with(|config| {
            config
                .with_sign_in_url(ProxyLink::try_from(SIGN_IN).unwrap())
                .with_sign_out_url(ProxyLink::try_from(SIGN_OUT).unwrap())
        })
        .await
    }

    /// A server whose proxy config `edit` finishes.
    pub async fn start_with(edit: impl FnOnce(ProxyConfig) -> ProxyConfig) -> Self {
        let fake = FakeProxy::start().await;
        let (proxy, _clock) = fake
            .connect_config(edit(fake.config(Admit::Emails(allow_list()))))
            .await;
        let listeners = Listeners::bind_hosted(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let port = listeners.port();
        let authority = format!("localhost:{port}");
        let origin = format!("http://{authority}");
        let public = PublicOrigin::loopback_http(&origin).unwrap();
        let (pages, records, _switches) = stores();
        seed_page_in(&pages, HOME, "<p>{{task.title}}</p>", &[]).await;
        let site = Site::Hosted {
            origin: public,
            sign_in: proxy,
        };
        let app: ProxiedApp = App::new(pages, records, schemas(), site, Limits::default());
        let app = app.with_sharing(share_api());
        let (sender, receiver) = oneshot::channel::<()>();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test server's task ends with the test's runtime"
        )]
        let task = tokio::spawn(listeners.serve(app, async {
            let _ = receiver.await;
        }));
        Self {
            authority,
            origin,
            fake,
            port,
            _shutdown: sender,
            _task: task,
        }
    }

    /// A request with the origin's Host.
    pub fn request(&self, method: &str, path: &str) -> Call {
        Call::new(self.port, method, path, &self.authority, &self.origin)
    }

    /// A GET with the origin's Host.
    pub fn get(&self, path: &str) -> Call {
        self.request("GET", path)
    }

    /// The Cookie header value that carries `token` as the proxy sets it.
    pub fn cookie(token: &str) -> String {
        format!("{COOKIE}={token}")
    }

    /// A flare token for the person whose proxy token is `token`, the way
    /// `flare login` gets one: `/cli/login` with the proxy's cookie, then
    /// the code exchange.
    pub async fn flare_token(&self, token: &str) -> String {
        let reply = self
            .get(&format!("/cli/login?port={CLI_PORT}&state={STATE}"))
            .cookie(&Self::cookie(token))
            .send()
            .await;
        assert_eq!(reply.status, 303, "{}", reply.body);
        let location = Url::parse(reply.header("location").unwrap()).unwrap();
        let code = location
            .query_pairs()
            .find(|(key, _)| key == "code")
            .map(|(_, value)| value.into_owned())
            .unwrap();
        let body = json!({ "code": code, "label": "test" }).to_string();
        let exchanged = self
            .request("POST", "/api/tokens/exchange")
            .body("application/json", body.into_bytes())
            .send()
            .await;
        assert_eq!(exchanged.status, 201, "{}", exchanged.body);
        let issued: serde_json::Value = serde_json::from_str(&exchanged.body).unwrap();
        issued["token"].as_str().unwrap().to_owned()
    }
}

impl Proxied {
    /// Shares a Markdown flare with the bearer `token`, which must succeed,
    /// and returns its id.
    pub async fn share(&self, token: &str) -> String {
        let body = json!({
            "title": "Notes",
            "format": "markdown",
            "content": "# Notes\n",
            "with": [],
            "re": [],
        })
        .to_string();
        let reply = self
            .request("POST", "/api/shares")
            .bearer(token)
            .body("application/json", body.into_bytes())
            .send()
            .await;
        assert_eq!(reply.status, 201, "{}", reply.body);
        let shared: serde_json::Value = serde_json::from_str(&reply.body).unwrap();
        shared["share"]["id"].as_str().unwrap().to_owned()
    }
}

/// The share API on memory stores, under the fake's allow-list, whose one
/// person may share.
fn share_api() -> Sharing<
    FaultyShares,
    MemoryGrantStore,
    MemoryResponseStore,
    MemoryCommentStore,
    MemoryCursorStore,
    FaultyTokens,
    FaultyRecipients,
    flare_core::NoTeams,
    CreatorList,
    flare_server::NoFiles,
> {
    let creators = CreatorList::only(vec![Email::try_from(crate::fake_proxy::EMAIL).unwrap()]);
    let flares = Flares::new(
        FaultyShares::default(),
        MemoryGrantStore::new(),
        MemoryResponseStore::new(),
        MemoryCommentStore::new(),
        MemoryCursorStore::new(),
        creators,
    );
    Sharing::new(
        flares,
        FaultyTokens::default(),
        FaultyRecipients::default(),
        Admit::Emails(allow_list()),
    )
}

/// The cookie pair `name` that `reply` sets, if any.
pub fn set_cookie(reply: &crate::harness::Reply, name: &str) -> Option<String> {
    cookie_pair(reply, name)
}
