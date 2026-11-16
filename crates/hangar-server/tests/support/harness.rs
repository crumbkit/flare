//! The test server: memory stores behind wrappers that count writes and
//! can be told to fail, a real listener on a free port, and a raw HTTP/1.1
//! client that sends exactly the headers a test names.

use super::scratch::Scratch;
use hangar_auth_loopback::{Loopback, ReturnPath, Served, StateDir, local_principal};
use hangar_auth_oidc::{OidcAuthenticator, TestClock};
use hangar_core::memory::{MemoryPageStore, MemoryRecordStore};
use hangar_core::{
    Binding, BindingName, Blob, ContentFormat, FieldName, Fills, MadeFrom, MoveError, Page,
    PageContent, PageName, PagePin, PageStore, PageStoreError, PageVersion, Principal, PrincipalId,
    Provenance, PublishedPage, ReadError, Record, RecordBody, RecordPath, RecordStore, RecordType,
    Rejection, Sample, Slot, SlotName, Slots, WriteError, WriteProvenance,
};
use hangar_server::{App, Clock, Gate, Limits, Listeners, Schemas, ServeError, Stopped};
use secrecy::ExposeSecret as _;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
use tokio::net::TcpStream;
use tokio::sync::{Notify, Semaphore, oneshot};
use tokio::task::JoinHandle;

/// The record type of the fixtures.
pub const TASK: &str = "tasks/task/v2";
/// The schema of `TASK`.
pub const TASK_SCHEMA: &str = r#"{
  "type": "object",
  "required": ["title", "status"],
  "properties": {
    "$type": {"const": "tasks/task/v2", "type": "string"},
    "title": {"type": "string", "title": "Title"},
    "status": {"type": "string", "title": "Status", "enum": ["open", "done"]}
  }
}"#;
/// The page every fixture binding pins.
pub const PAGE: &str = "task-card";
/// The binding every fixture makes.
pub const BINDING: &str = "kitchen";
/// The record the binding fills its slot with.
pub const RECORD: &str = "/tasks/tap";
/// The body of `RECORD` when seeded.
pub const BODY: &str = r#"{"title": "Replace the tap washer", "status": "open"}"#;
/// The template of `PAGE`.
pub const TEMPLATE: &str = "<h1 class=\"card\">{{task.title}}</h1>";
/// The sample of `PAGE`'s slot.
pub const SAMPLE: &str = r#"{"title": "A sample task", "status": "open"}"#;

/// Switches the tests flip to make the stores fail or wait.
#[derive(Debug, Clone, Default)]
pub struct Switches {
    /// Page and binding writes: publish, save and move.
    pub page_writes: Arc<AtomicUsize>,
    /// Record writes.
    pub record_writes: Arc<AtomicUsize>,
    /// The next page read panics.
    pub panic_on_read: Arc<AtomicBool>,
    /// Every page read answers `Unavailable`.
    pub pages_down: Arc<AtomicBool>,
    /// How many of the next record writes answer `Conflict` without writing.
    pub conflicts: Arc<AtomicUsize>,
    /// The next record write is rejected with this text.
    pub reject: Arc<Mutex<Option<String>>>,
    /// While set, `latest` waits for a permit of this semaphore.
    pub hold: Arc<Mutex<Option<Arc<Semaphore>>>>,
    /// Notified when `latest` starts waiting on `hold`.
    pub held: Arc<Notify>,
    /// While set, a record write waits for a permit of this semaphore
    /// before it writes.
    pub write_hold: Arc<Mutex<Option<Arc<Semaphore>>>>,
    /// Notified when a record write starts waiting on `write_hold`.
    pub write_held: Arc<Notify>,
    /// Record writes whose future has started and has not yet ended or
    /// been dropped.
    pub writes_in_work: Arc<AtomicUsize>,
    /// A binding that reads of its name answer in place of the stored one,
    /// as when someone edits a binding record by hand.
    pub binding_override: Arc<Mutex<Option<Binding>>>,
}

/// The page store the server gets.
#[derive(Debug, Clone)]
pub struct Pages {
    /// The store behind it.
    pub store: Arc<MemoryPageStore>,
    switches: Switches,
}

impl Pages {
    fn read(&self) -> Result<(), PageStoreError> {
        if self.switches.panic_on_read.swap(false, Ordering::SeqCst) {
            panic!("a page read panicked, as the test asked");
        }
        if self.switches.pages_down.load(Ordering::SeqCst) {
            return Err(PageStoreError::Unavailable);
        }
        Ok(())
    }
}

impl PageStore for Pages {
    async fn publish(
        &self,
        page: Page,
        provenance: Provenance,
    ) -> Result<PageVersion, PageStoreError> {
        self.switches.page_writes.fetch_add(1, Ordering::SeqCst);
        self.store.publish(page, provenance).await
    }

    async fn latest(&self, name: &PageName) -> Result<PublishedPage, PageStoreError> {
        self.read()?;
        let hold = self.switches.hold.lock().unwrap().clone();
        if let Some(hold) = hold {
            self.switches.held.notify_one();
            let _permit = hold.acquire().await;
        }
        self.store.latest(name).await
    }

    async fn at(&self, pin: &PagePin) -> Result<PublishedPage, PageStoreError> {
        self.read()?;
        self.store.at(pin).await
    }

    async fn versions(&self, name: &PageName) -> Result<Vec<PageVersion>, PageStoreError> {
        self.read()?;
        self.store.versions(name).await
    }

    async fn pages(&self) -> Result<Vec<PageName>, PageStoreError> {
        self.read()?;
        self.store.pages().await
    }

    async fn save_binding(
        &self,
        binding: Binding,
        records: &impl RecordStore,
    ) -> Result<(), PageStoreError> {
        self.switches.page_writes.fetch_add(1, Ordering::SeqCst);
        self.store.save_binding(binding, records).await
    }

    async fn binding(&self, name: &BindingName) -> Result<Binding, PageStoreError> {
        self.read()?;
        let swapped = self.switches.binding_override.lock().unwrap().clone();
        if let Some(binding) = swapped.filter(|binding| binding.name() == name) {
            return Ok(binding);
        }
        self.store.binding(name).await
    }

    async fn bindings_of(&self, name: &PageName) -> Result<Vec<Binding>, PageStoreError> {
        self.read()?;
        self.store.bindings_of(name).await
    }

    async fn check_fit(
        &self,
        name: &BindingName,
        records: &impl RecordStore,
    ) -> Result<(), PageStoreError> {
        self.read()?;
        self.store.check_fit(name, records).await
    }

    async fn move_binding(
        &self,
        name: &BindingName,
        expected: &PageVersion,
        version: &PageVersion,
        records: &impl RecordStore,
        mover: &WriteProvenance,
    ) -> Result<(), MoveError> {
        self.switches.page_writes.fetch_add(1, Ordering::SeqCst);
        self.store
            .move_binding(name, expected, version, records, mover)
            .await
    }
}

/// Counts one record write in work until it drops, whether the write ends
/// or its future is dropped.
struct InWork(Arc<AtomicUsize>);

impl InWork {
    fn enter(count: &Arc<AtomicUsize>) -> Self {
        count.fetch_add(1, Ordering::SeqCst);
        Self(Arc::clone(count))
    }
}

impl Drop for InWork {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The record store the server gets.
#[derive(Debug, Clone)]
pub struct Records {
    /// The store behind it.
    pub store: Arc<MemoryRecordStore>,
    switches: Switches,
}

impl RecordStore for Records {
    async fn read(&self, path: &RecordPath) -> Result<Record, ReadError> {
        self.store.read(path).await
    }

    async fn read_all(&self, paths: &[RecordPath]) -> Vec<Result<Record, ReadError>> {
        self.store.read_all(paths).await
    }

    async fn write(
        &self,
        path: &RecordPath,
        body: &RecordBody,
        provenance: &WriteProvenance,
    ) -> Result<Blob, WriteError> {
        let _in_work = InWork::enter(&self.switches.writes_in_work);
        let hold = self.switches.write_hold.lock().unwrap().clone();
        if let Some(hold) = hold {
            self.switches.write_held.notify_one();
            let _permit = hold.acquire().await;
        }
        let conflict = self
            .switches
            .conflicts
            .try_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                left.checked_sub(1)
            })
            .is_ok();
        if conflict {
            return Err(WriteError::Conflict);
        }
        if let Some(text) = self.switches.reject.lock().unwrap().take() {
            return Err(WriteError::Rejected(Rejection::new(text)));
        }
        let written = self.store.write(path, body, provenance).await;
        if written.is_ok() {
            self.switches.record_writes.fetch_add(1, Ordering::SeqCst);
        }
        written
    }
}

/// Schemas from a fixed map.
#[derive(Debug, Clone)]
pub struct FixedSchemas(pub HashMap<String, String>);

impl Schemas for FixedSchemas {
    async fn schema(&self, record_type: &RecordType) -> Result<String, ReadError> {
        self.0
            .get(record_type.as_str())
            .cloned()
            .ok_or(ReadError::NotFound)
    }
}

/// A clock that tests move forward.
#[derive(Debug, Clone, Default)]
pub struct TestTime(Arc<Mutex<Duration>>);

impl TestTime {
    /// Moves the clock forward by `by`.
    pub fn advance(&self, by: Duration) {
        *self.0.lock().unwrap() += by;
    }

    fn offset(&self) -> Duration {
        *self.0.lock().unwrap()
    }
}

impl Clock for TestTime {
    fn now(&self) -> Instant {
        Instant::now() + self.offset()
    }

    fn wall(&self) -> SystemTime {
        SystemTime::now() + self.offset()
    }
}

/// The default limits with `edit` applied.
pub fn limits(edit: impl FnOnce(&mut Limits)) -> Limits {
    let mut limits = Limits::default();
    edit(&mut limits);
    limits
}

/// What a test server starts with.
#[derive(Debug, Default)]
pub struct Options {
    /// The limits; the defaults when `None`.
    pub limits: Option<Limits>,
    /// The OpenID Connect authenticator, which turns the sign-in host on.
    pub oidc: Option<OidcAuthenticator<TestClock>>,
    /// The stores and state directory of an earlier server, which this one
    /// shares, as a new `hangar serve` process on the same board does.
    pub after: Option<Earlier>,
}

/// What a later server on the same board shares with an earlier one.
#[derive(Debug, Clone)]
pub struct Earlier {
    pages: Pages,
    records: Records,
    switches: Switches,
    state: StateDir,
}

/// A running server.
#[derive(Debug)]
pub struct Server {
    /// The loopback authenticator the server uses.
    pub loopback: Loopback,
    /// The launch host's authority, `h-<hex>.localhost:<port>`.
    pub authority: String,
    /// The launch host's port.
    pub port: u16,
    /// The sign-in host's port, when OpenID Connect is on.
    pub oidc_port: Option<u16>,
    /// The switches of the stores.
    pub switches: Switches,
    /// The page store.
    pub pages: Pages,
    /// The record store.
    pub records: Records,
    /// The clock.
    pub time: TestTime,
    /// The state directory.
    pub state: StateDir,
    /// The `Debug` output of the app, taken before it started serving.
    pub debug: String,
    app: App<Pages, Records, FixedSchemas, TestClock>,
    scratch: Scratch,
    shutdown: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<Result<Stopped, ServeError>>>,
}

impl Server {
    /// A server with the default limits and no OpenID Connect.
    pub async fn start() -> Self {
        Self::start_with(Options::default()).await
    }

    /// A server with `options`.
    pub async fn start_with(options: Options) -> Self {
        let oidc_wanted = options.oidc.as_ref().map(|_| 0);
        let listeners = Listeners::bind(0, oidc_wanted).unwrap();
        let port = listeners.port();
        let oidc_port = listeners.oidc_port();
        let loopback =
            Loopback::new(port, local_principal("tester").unwrap(), Served::Everything).unwrap();
        let scratch = Scratch::new();
        let (pages, records, switches, state) = match options.after {
            Some(earlier) => (
                earlier.pages,
                earlier.records,
                earlier.switches,
                earlier.state,
            ),
            None => {
                let switches = Switches::default();
                let pages = Pages {
                    store: Arc::new(MemoryPageStore::new()),
                    switches: switches.clone(),
                };
                let records = Records {
                    store: Arc::new(MemoryRecordStore::new()),
                    switches: switches.clone(),
                };
                let state = StateDir::for_board(
                    &scratch.path().join("state"),
                    &scratch.path().join("board"),
                );
                (pages, records, switches, state)
            }
        };
        let schemas = FixedSchemas(HashMap::from([(TASK.to_owned(), TASK_SCHEMA.to_owned())]));
        let time = TestTime::default();
        let app = App::new(
            pages.clone(),
            records.clone(),
            schemas,
            Gate::new(loopback.clone(), options.oidc),
            state.clone(),
            options.limits.unwrap_or_default(),
        )
        .with_clock(time.clone());
        let debug = format!("{app:?} {listeners:?}");
        let (sender, receiver) = oneshot::channel::<()>();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test server's task is joined by Server::join or ends with the test's runtime"
        )]
        let task = tokio::spawn(listeners.serve(app.clone(), async {
            let _ = receiver.await;
        }));
        let authority = loopback.launch_host().authority();
        let server = Self {
            loopback,
            authority,
            port,
            oidc_port,
            switches,
            pages,
            records,
            time,
            state,
            debug,
            app,
            scratch,
            shutdown: Some(sender),
            task: Some(task),
        };
        server.wait_for_server_file().await;
        server
    }

    async fn wait_for_server_file(&self) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !self.named_by_server_file() {
            assert!(
                Instant::now() < deadline,
                "the server never wrote server.json"
            );
            tokio::task::yield_now().await;
        }
    }

    /// The `Debug` output of the app now, with every session, token and
    /// code it holds.
    pub fn debug_now(&self) -> String {
        format!("{:?}", self.app)
    }

    /// Whether `server.json` names this server.
    pub fn named_by_server_file(&self) -> bool {
        self.state
            .read_server()
            .ok()
            .flatten()
            .is_some_and(|record| record.token().expose_secret() == self.token())
    }

    /// What a later server on the same board shares with this one.
    pub fn earlier(&self) -> Earlier {
        Earlier {
            pages: self.pages.clone(),
            records: self.records.clone(),
            switches: self.switches.clone(),
            state: self.state.clone(),
        }
    }

    /// The scratch directory.
    pub fn scratch(&self) -> &std::path::Path {
        self.scratch.path()
    }

    /// Resolves the shutdown future.
    pub fn signal_shutdown(&mut self) {
        if let Some(sender) = self.shutdown.take() {
            let _ = sender.send(());
        }
    }

    /// Resolves the shutdown future and waits for `serve` to return.
    pub async fn stop(&mut self) -> Stopped {
        self.signal_shutdown();
        self.join().await
    }

    /// Waits for `serve` to return by itself.
    pub async fn join(&mut self) -> Stopped {
        let task = self.task.take().expect("the server is running");
        tokio::time::timeout(Duration::from_secs(30), task)
            .await
            .expect("serve returns")
            .unwrap()
            .unwrap()
    }

    /// The launch token, as `hangar open` reads it from `server.json`.
    pub fn token(&self) -> String {
        self.loopback
            .server_record()
            .token()
            .expose_secret()
            .to_owned()
    }

    /// A request on the launch host.
    pub fn request(&self, method: &str, path: &str) -> Call {
        Call {
            port: self.port,
            method: method.to_owned(),
            path: path.to_owned(),
            headers: vec![
                ("Host".to_owned(), self.authority.clone()),
                ("Connection".to_owned(), "close".to_owned()),
            ],
            body: Vec::new(),
            origin: format!("http://{}", self.authority),
        }
    }

    /// A GET on the launch host.
    pub fn get(&self, path: &str) -> Call {
        self.request("GET", path)
    }

    /// A request on the sign-in host, `localhost:<port>`.
    pub fn oidc_request(&self, method: &str, path: &str) -> Call {
        let port = self.oidc_port.expect("OpenID Connect is on");
        Call {
            port,
            method: method.to_owned(),
            path: path.to_owned(),
            headers: vec![
                ("Host".to_owned(), format!("localhost:{port}")),
                ("Connection".to_owned(), "close".to_owned()),
            ],
            body: Vec::new(),
            origin: format!("http://localhost:{port}"),
        }
    }

    /// Signs a browser in through a launch code and returns its
    /// `hangar_session=<secret>` cookie pair.
    pub async fn sign_in(&self) -> String {
        let code = self
            .loopback
            .issue_code(ReturnPath::try_from("/hangar/").unwrap(), self.time.now())
            .unwrap();
        let reply = self
            .get(&format!("/launch?code={}", code.secret().expose_secret()))
            .send()
            .await;
        assert_eq!(reply.status, 200, "{}", reply.body);
        cookie_pair(&reply, "hangar_session").expect("a session cookie")
    }

    /// Publishes `PAGE` with one `task` slot whose `title` is editable.
    pub async fn seed_page(&self) -> PageVersion {
        self.seed_page_with(PAGE, TEMPLATE, &["title"]).await
    }

    /// Publishes a page named `name` with `template` and one `task` slot
    /// that edits `editable`.
    pub async fn seed_page_with(
        &self,
        name: &str,
        template: &str,
        editable: &[&str],
    ) -> PageVersion {
        let slot = Slot::new(
            SlotName::try_from("task").unwrap(),
            RecordType::try_from(TASK).unwrap(),
            editable
                .iter()
                .map(|field| FieldName::try_from(*field).unwrap())
                .collect(),
            Some(Sample::new(
                RecordType::try_from(TASK).unwrap(),
                RecordBody::try_from(SAMPLE).unwrap(),
            )),
        )
        .unwrap();
        let page = Page::new(
            PageName::try_from(name).unwrap(),
            PageContent::new(ContentFormat::Html, template).unwrap(),
            Slots::new(vec![slot]).unwrap(),
        );
        self.pages
            .store
            .publish(page, provenance(MadeFrom::Original))
            .await
            .unwrap()
    }

    /// Seeds `path` with `body` and returns its blob.
    pub fn seed_record(&self, path: &str, body: &str) -> Blob {
        self.records
            .store
            .seed(
                RecordPath::try_from(path).unwrap(),
                RecordType::try_from(TASK).unwrap(),
                RecordBody::try_from(body).unwrap(),
            )
            .unwrap()
    }

    /// The body of `path` now.
    pub async fn body_of(&self, path: &str) -> String {
        self.records
            .store
            .read(&RecordPath::try_from(path).unwrap())
            .await
            .unwrap()
            .body()
            .as_str()
            .to_owned()
    }

    /// Publishes `PAGE`, seeds `RECORD` and saves `BINDING`, which fills the
    /// `task` slot with it.
    pub async fn seed_binding(&self) -> Blob {
        let version = self.seed_page().await;
        let blob = self.seed_record(RECORD, BODY);
        self.save_binding(BINDING, PAGE, &version, RECORD).await;
        blob
    }

    /// From now on, reads of the binding `name` of `PAGE` at `version`
    /// answer it filling `task` with `record`.
    pub fn swap_fill(&self, name: &str, version: &PageVersion, record: &str) {
        *self.switches.binding_override.lock().unwrap() =
            Some(binding(name, PAGE, version, record));
    }

    /// Saves the binding `name` of `page` at `version`, filling `task` with
    /// `record`.
    pub async fn save_binding(&self, name: &str, page: &str, version: &PageVersion, record: &str) {
        let binding = binding(name, page, version, record);
        self.pages
            .store
            .save_binding(binding, &*self.records.store)
            .await
            .unwrap();
    }
}

/// The binding `name` of `page` at `version`, filling `task` with `record`.
pub fn binding(name: &str, page: &str, version: &PageVersion, record: &str) -> Binding {
    let pin = PagePin::new(PageName::try_from(page).unwrap(), version.clone());
    let fills = Fills::new([(
        SlotName::try_from("task").unwrap(),
        RecordPath::try_from(record).unwrap(),
    )])
    .unwrap();
    Binding::new(
        BindingName::try_from(name).unwrap(),
        pin.clone(),
        fills,
        provenance(MadeFrom::Page(pin)),
    )
}

/// A provenance made by the tests.
pub fn provenance(made_from: MadeFrom) -> Provenance {
    Provenance::new(
        Principal::new(PrincipalId::try_from("local:tester").unwrap()),
        made_from,
        Vec::new(),
    )
    .unwrap()
}

/// One request, built header by header.
#[derive(Debug, Clone)]
pub struct Call {
    port: u16,
    method: String,
    path: String,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    origin: String,
}

impl Call {
    /// Adds a header.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_owned(), value.to_owned()));
        self
    }

    /// Removes every header named `name`.
    pub fn without(mut self, name: &str) -> Self {
        self.headers
            .retain(|(key, _)| !key.eq_ignore_ascii_case(name));
        self
    }

    /// Replaces the Host header.
    pub fn host(self, host: &str) -> Self {
        self.without("Host").header("Host", host)
    }

    /// Sends the session cookie pair `cookie`.
    pub fn cookie(self, cookie: &str) -> Self {
        self.header("Cookie", cookie)
    }

    /// Sends `token` as a bearer token.
    pub fn bearer(self, token: &str) -> Self {
        self.header("Authorization", &format!("Bearer {token}"))
    }

    /// Adds `Origin` and `Sec-Fetch-Site: same-origin`, as a browser does for
    /// a form post on the page's own origin.
    pub fn same_origin(self) -> Self {
        let origin = self.origin.clone();
        self.header("Origin", &origin)
            .header("Sec-Fetch-Site", "same-origin")
    }

    /// Sends `pairs` as a form post body.
    pub fn form(mut self, pairs: &[(String, String)]) -> Self {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(pairs)
            .finish();
        self.body = body.into_bytes();
        self.header("Content-Type", "application/x-www-form-urlencoded")
    }

    /// Sends `body` with `content_type`.
    pub fn body(mut self, content_type: &str, body: Vec<u8>) -> Self {
        self.body = body;
        self.header("Content-Type", content_type)
    }

    /// The request's bytes.
    pub fn bytes(&self) -> Vec<u8> {
        let mut out = format!("{} {} HTTP/1.1\r\n", self.method, self.path);
        for (name, value) in &self.headers {
            out.push_str(&format!("{name}: {value}\r\n"));
        }
        if !self.body.is_empty() || self.method == "POST" {
            out.push_str(&format!("Content-Length: {}\r\n", self.body.len()));
        }
        out.push_str("\r\n");
        let mut bytes = out.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }

    /// Sends the request and reads the whole reply.
    pub async fn send(self) -> Reply {
        exchange(self.port, &self.bytes()).await
    }
}

/// A reply as the client read it.
#[derive(Debug, Clone)]
pub struct Reply {
    /// The status code.
    pub status: u16,
    /// The headers, names in lower case, in order.
    pub headers: Vec<(String, String)>,
    /// The body as text.
    pub body: String,
}

impl Reply {
    /// The value of the one header `name`.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }

    /// Every value of the header `name`.
    pub fn all(&self, name: &str) -> Vec<&str> {
        self.headers
            .iter()
            .filter(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
            .collect()
    }

    /// The problem code in the body, from JSON or from the HTML document's
    /// `data-code` attribute.
    pub fn code(&self) -> Option<String> {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&self.body) {
            return value["code"].as_str().map(str::to_owned);
        }
        let start = self.body.find("data-code=\"")? + "data-code=\"".len();
        let rest = &self.body[start..];
        Some(rest[..rest.find('"')?].to_owned())
    }
}

/// The `name=value` pair of the cookie `name` that the reply sets.
pub fn cookie_pair(reply: &Reply, name: &str) -> Option<String> {
    reply
        .all("set-cookie")
        .into_iter()
        .find(|cookie| cookie.starts_with(&format!("{name}=")))
        .and_then(|cookie| cookie.split(';').next())
        .map(str::to_owned)
}

/// Writes `bytes` to a new connection to `port` and reads the reply until
/// the server closes it.
pub async fn exchange(port: u16, bytes: &[u8]) -> Reply {
    let mut stream = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    stream.write_all(bytes).await.unwrap();
    let mut raw = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), stream.read_to_end(&mut raw))
        .await
        .expect("the server answers within 20 seconds")
        .unwrap();
    parse(&raw)
}

/// Parses one HTTP/1.1 reply.
pub fn parse(raw: &[u8]) -> Reply {
    let split = raw
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap_or_else(|| panic!("no header end in {:?}", String::from_utf8_lossy(raw)));
    let head = String::from_utf8_lossy(&raw[..split]).into_owned();
    let rest = &raw[split + 4..];
    let mut lines = head.split("\r\n");
    let status_line = lines.next().unwrap();
    let status = status_line.split(' ').nth(1).unwrap().parse().unwrap();
    let headers: Vec<(String, String)> = lines
        .map(|line| {
            let (name, value) = line.split_once(':').unwrap();
            (name.trim().to_ascii_lowercase(), value.trim().to_owned())
        })
        .collect();
    let chunked = headers
        .iter()
        .any(|(name, value)| name == "transfer-encoding" && value.contains("chunked"));
    let body = if chunked {
        unchunk(rest)
    } else {
        rest.to_vec()
    };
    Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&body).into_owned(),
    }
}

fn unchunk(mut rest: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    loop {
        let Some(end) = rest.windows(2).position(|window| window == b"\r\n") else {
            return out;
        };
        let size =
            usize::from_str_radix(std::str::from_utf8(&rest[..end]).unwrap().trim(), 16).unwrap();
        if size == 0 {
            return out;
        }
        out.extend_from_slice(&rest[end + 2..end + 2 + size]);
        rest = &rest[end + 4 + size..];
    }
}

/// The hidden inputs and field values of the first form in `html`, as a
/// browser would post them, HTML entities decoded.
pub fn form_pairs(html: &str) -> Vec<(String, String)> {
    let start = html.find("<form").expect("a form");
    let end = html[start..]
        .find("</form>")
        .map_or(html.len(), |end| start + end);
    let form = &html[start..end];
    let mut pairs = Vec::new();
    let mut rest = form;
    while let Some(at) = rest.find("<input") {
        let tag_end = rest[at..].find('>').unwrap() + at;
        let tag = &rest[at..tag_end];
        if let (Some(name), Some(value)) = (attribute(tag, "name"), attribute(tag, "value")) {
            pairs.push((name, value));
        }
        rest = &rest[tag_end..];
    }
    pairs
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let key = format!(" {name}=\"");
    let start = tag.find(&key)? + key.len();
    let end = tag[start..].find('"')? + start;
    Some(unescape(&tag[start..end]))
}

/// Decodes the five entities hangar-render writes.
pub fn unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// Sets the value of `name` in `pairs`.
pub fn set(pairs: &mut [(String, String)], name: &str, value: &str) {
    let pair = pairs
        .iter_mut()
        .find(|(key, _)| key == name)
        .unwrap_or_else(|| panic!("no {name} in the form"));
    pair.1 = value.to_owned();
}

/// The value of `name` in `pairs`.
pub fn value<'a>(pairs: &'a [(String, String)], name: &str) -> &'a str {
    &pairs
        .iter()
        .find(|(key, _)| key == name)
        .unwrap_or_else(|| panic!("no {name} in the form"))
        .1
}
