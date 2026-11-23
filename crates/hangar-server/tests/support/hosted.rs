//! A hosted test server: one plain-HTTP listener, as a TLS proxy would
//! reach it, with an origin from `PublicOrigin::loopback_http`, the fake
//! provider and memory stores. Requests name `localhost:<port>` as their
//! Host, which is the origin's authority.

use crate::fake_provider::{DOMAIN, Fake, HOME, connect, connect_domain, connect_with_deadline};
use crate::harness::{
    BINDING, Call, Pages, RECORD, Records, Reply, Switches, cookie_pair, schemas, seed_page_in,
    stores,
};
use hangar_auth_oidc::{OidcAuthenticator, TestClock};
use hangar_core::PageStore as _;
use hangar_server::{App, Limits, Listeners, PublicOrigin, ServeError, Site, Stopped};
use serde_json::json;
use std::net::{Ipv4Addr, SocketAddr};
use std::time::Duration;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use url::Url;

/// The session cookie's name on a hosted server.
pub const SESSION: &str = "__Host-flare_session";
/// The kept-state cookie's name on a hosted server.
pub const KEPT: &str = "__Host-flare_oidc_state";

/// Who the fake admits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Admits {
    /// The allow-list: `ada@example.com` as `ada`, reaching the page `home`.
    AllowList,
    /// Every verified account of `example.com`, reaching the page `home`.
    Domain,
}

/// A running hosted server.
#[derive(Debug)]
pub struct Hosted {
    /// The listener's port.
    pub port: u16,
    /// The Host that requests name: `localhost:<port>`.
    pub authority: String,
    /// The public origin: `http://localhost:<port>`.
    pub origin: String,
    /// The switches of the stores.
    pub switches: Switches,
    /// The page store.
    pub pages: Pages,
    /// The record store.
    pub records: Records,
    /// The fake provider.
    pub fake: Fake,
    /// The authenticator's clock.
    pub clock: TestClock,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<Stopped, ServeError>>>,
}

impl Hosted {
    /// A hosted server with the default limits that admits the allow-list,
    /// with the page `home` published.
    pub async fn start() -> Self {
        Self::start_with(Admits::AllowList, None).await
    }

    /// A hosted server that admits the allow-list, with `limits`, the page
    /// `home` published, and `deadline` for each request to the provider.
    pub async fn start_with_deadline(deadline: Duration, limits: Limits) -> Self {
        let fake = Fake::start().await;
        let (oidc, clock) = connect_with_deadline(&fake, deadline).await;
        Self::serve(oidc, fake, clock, Some(limits)).await
    }

    /// A hosted server that admits `admits`, with `limits` or the default
    /// ones, and the page `home` published.
    pub async fn start_with(admits: Admits, limits: Option<Limits>) -> Self {
        let fake = Fake::start().await;
        let (oidc, clock) = match admits {
            Admits::AllowList => connect(&fake).await,
            Admits::Domain => {
                fake.set_hd(Some(json!(DOMAIN)));
                connect_domain(&fake).await
            }
        };
        Self::serve(oidc, fake, clock, limits).await
    }

    async fn serve(
        oidc: OidcAuthenticator<TestClock>,
        fake: Fake,
        clock: TestClock,
        limits: Option<Limits>,
    ) -> Self {
        let listeners = Listeners::bind_hosted(SocketAddr::from((Ipv4Addr::LOCALHOST, 0))).unwrap();
        let port = listeners.port();
        let authority = format!("localhost:{port}");
        let origin = format!("http://{authority}");
        let public = PublicOrigin::loopback_http(&origin).unwrap();
        let (pages, records, switches) = stores();
        let app = App::new(
            pages.clone(),
            records.clone(),
            schemas(),
            Site::Hosted {
                origin: public,
                oidc,
            },
            limits.unwrap_or_default(),
        );
        let (sender, receiver) = oneshot::channel::<()>();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test server's task is joined by Hosted::stop or ends with the test's runtime"
        )]
        let task = tokio::spawn(listeners.serve(app, async {
            let _ = receiver.await;
        }));
        let hosted = Self {
            port,
            authority,
            origin,
            switches,
            pages,
            records,
            fake,
            clock,
            shutdown: Some(sender),
            task: Some(task),
        };
        seed_page_in(&hosted.pages, HOME, "<p>{{task.title}}</p>", &[]).await;
        hosted
    }

    /// Publishes the page `name`.
    pub async fn seed_page(&self, name: &str) {
        seed_page_in(&self.pages, name, "<p>{{task.title}}</p>", &[]).await;
    }

    /// Publishes `PAGE`, seeds `RECORD` and saves `BINDING`.
    pub async fn seed_binding(&self) {
        let version = seed_page_in(&self.pages, crate::harness::PAGE, "<p>x</p>", &["title"]).await;
        self.records
            .store
            .seed(
                hangar_core::RecordPath::try_from(RECORD).unwrap(),
                hangar_core::RecordType::try_from(crate::harness::TASK).unwrap(),
                hangar_core::RecordBody::try_from(crate::harness::BODY).unwrap(),
            )
            .unwrap();
        let binding = crate::harness::binding(BINDING, crate::harness::PAGE, &version, RECORD);
        self.pages
            .store
            .save_binding(binding, &*self.records.store)
            .await
            .unwrap();
    }

    /// A request with the origin's Host.
    pub fn request(&self, method: &str, path: &str) -> Call {
        Call::new(self.port, method, path, &self.authority, &self.origin)
    }

    /// A GET with the origin's Host.
    pub fn get(&self, path: &str) -> Call {
        self.request("GET", path)
    }

    /// Starts a sign-in that returns to `/pages/home`: the reply, the
    /// provider URL and the kept-state cookie pair.
    pub async fn start_sign_in(&self) -> (Reply, Url, String) {
        let reply = self
            .get(&format!("/auth/oidc/start?return_to=/pages/{HOME}"))
            .send()
            .await;
        assert_eq!(reply.status, 303, "{}", reply.body);
        let location = Url::parse(reply.header("location").unwrap()).unwrap();
        let kept = cookie_pair(&reply, KEPT).expect("a kept-state cookie");
        (reply, location, kept)
    }

    /// The callback request with the provider's answer `query`.
    pub fn callback(&self, query: &str) -> Call {
        self.get(&format!("/auth/oidc/callback?{query}"))
    }

    /// Signs a browser in and returns the callback's reply. The browser
    /// sends `cookies` beside the kept state.
    pub async fn sign_in_reply(&self, cookies: &[&str]) -> Reply {
        let (_, location, kept) = self.start_sign_in().await;
        let mut sent = vec![kept.as_str()];
        sent.extend_from_slice(cookies);
        self.callback(&self.fake.approve(&location))
            .cookie(&sent.join("; "))
            .send()
            .await
    }

    /// Signs a browser in and returns its session cookie pair.
    pub async fn sign_in(&self) -> String {
        self.sign_in_beside(&[]).await
    }

    /// Signs a browser in that holds `cookies` and returns its new session
    /// cookie pair.
    pub async fn sign_in_beside(&self, cookies: &[&str]) -> String {
        let reply = self.sign_in_reply(cookies).await;
        assert_eq!(reply.status, 303, "{}", reply.body);
        cookie_pair(&reply, SESSION).expect("a session cookie")
    }

    /// Signs the domain account `<name>@example.com` in and returns its
    /// session cookie pair.
    pub async fn sign_in_as(&self, name: &str) -> String {
        self.fake.set_email(&format!("{name}@{DOMAIN}"));
        self.sign_in().await
    }

    /// The status of a read of `/pages/home` with `cookie`.
    pub async fn read_home(&self, cookie: &str) -> u16 {
        self.get(&format!("/pages/{HOME}"))
            .cookie(cookie)
            .send()
            .await
            .status
    }

    /// A read of `/pages/home` with `cookie`.
    pub async fn read_home_reply(&self, cookie: &str) -> Reply {
        self.get(&format!("/pages/{HOME}"))
            .cookie(cookie)
            .send()
            .await
    }

    /// Resolves the shutdown future and waits for `serve` to return.
    pub async fn stop(&mut self) -> Stopped {
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
        let task = self.task.take().expect("the server is running");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("serve returns")
            .unwrap()
            .unwrap()
    }

    /// Waits at most `limit` for `serve` to return by itself, and says
    /// whether it did.
    pub async fn returns_within(&mut self, limit: Duration) -> bool {
        let task = self.task.as_mut().expect("the server is running");
        tokio::time::timeout(limit, task).await.is_ok()
    }
}
