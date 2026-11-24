//! The pieces of the `hosted_stores` test: the built `flare serve` on a
//! hosted config, the person's browser, and the commands that need the
//! stand-ins and the generated CA in their environment.
//!
//! Only the fake provider speaks TLS. `flare serve` listens on plain HTTP
//! at 127.0.0.1 and its config names an `https://localhost:<port>` origin,
//! so the browser here sends `Host: localhost:<port>`, which is all the
//! server's Host check compares, and replays the `__Host-` cookies by hand
//! because the server sets them `Secure`.

use crate::api::Fakes as _;
use crate::run::{Home, Ran, write};
use crate::stand_in::{
    ACCESS_KEY_ID, SECRET_ACCESS_KEY, create_bucket, create_table, random_name, stand_in_url,
};
use crate::tls_provider::{CLIENT_ID, CLIENT_SECRET, Provider};
use reqwest::header::{COOKIE, HOST, LOCATION, SET_COOKIE};
use reqwest::{Client, Response, Url, redirect};
use std::net::{Ipv4Addr, SocketAddr, TcpListener};
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tokio::runtime::Runtime;

/// How long a step waits for the server or for the CLI.
const WAIT: Duration = Duration::from_secs(60);

/// A port that was free a moment ago.
pub fn free_port() -> u16 {
    TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// A child process that is killed when it drops.
#[derive(Debug)]
pub struct Guard(Option<Child>);

impl Guard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn child(&mut self) -> &mut Child {
        self.0
            .as_mut()
            .expect("a child that was not yet waited for")
    }

    /// Waits for the child to end and returns what it printed.
    fn finish(mut self) -> std::process::Output {
        self.0
            .take()
            .expect("a child that was not yet waited for")
            .wait_with_output()
            .unwrap()
    }
}

impl Drop for Guard {
    fn drop(&mut self) {
        if let Some(child) = &mut self.0 {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// Waits until `ready` holds, panicking with `what` after [`WAIT`].
pub fn wait_for(what: &str, mut ready: impl FnMut() -> bool) {
    let start = Instant::now();
    while !ready() {
        assert!(start.elapsed() < WAIT, "timed out waiting for {what}");
        #[expect(
            clippy::disallowed_methods,
            reason = "the test polls a child process and a file it cannot wait on"
        )]
        std::thread::sleep(Duration::from_millis(50));
    }
}

/// Which of the config's table and bucket exist on the stand-ins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Made {
    /// Both.
    Both,
    /// The table only.
    TableOnly,
    /// Neither.
    Neither,
}

/// Everything one run needs: the provider, the CA file, a stub board, a
/// secret file and a config that names a table and a bucket.
#[derive(Debug)]
pub struct Rig {
    /// The runtime that runs the provider and the browser's requests.
    pub runtime: Runtime,
    /// The scratch home of the commands.
    pub home: Home,
    /// The fake provider.
    pub provider: Provider,
    /// The port that `flare serve` listens on.
    pub port: u16,
    /// The table the config names.
    pub table: String,
    /// The bucket the config names.
    pub bucket: String,
    ca: PathBuf,
    board: PathBuf,
    program: PathBuf,
}

impl Rig {
    /// Starts the provider and writes the CA, the stub board, the secret
    /// file and the config. The table and bucket are created as `made`
    /// says.
    pub fn new(made: Made) -> Self {
        let runtime = Runtime::new().unwrap();
        let provider = runtime.block_on(Provider::start());
        let home = Home::new();
        let ca = home.path().join("ca.pem");
        write(&ca, &provider.ca_pem);
        let board = home.path().join("board-root");
        std::fs::create_dir_all(board.join(".board")).unwrap();
        write(&board.join(".board/board.json"), "{}");
        // `flare serve` only asks the board program whether it has
        // `log --at`, and a failure report that is not a usage failure says
        // so; the share API reads no board.
        let program = home.script(
            "board-stub",
            r#"printf '%s\n' '{"schema":"board/failure/v1","code":"board::not_found","message":"no such path","fields":{}}'
exit 1
"#,
        );
        let secret = home.path().join("client_secret");
        write(&secret, CLIENT_SECRET);
        std::fs::set_permissions(&secret, std::fs::Permissions::from_mode(0o600)).unwrap();
        let port = free_port();
        let table = random_name("flare-e2e");
        let bucket = random_name("flare-e2e");
        runtime.block_on(async {
            if made != Made::Neither {
                create_table(&table, None).await;
            }
            if made == Made::Both {
                create_bucket(&bucket).await;
            }
        });
        home.write_config(&format!(
            r#"{{"hosted":{{"origin":"https://localhost:{port}","listen":"127.0.0.1:{port}","stores":{{"region":"us-east-1","table":"{table}","bucket":"{bucket}"}}}},"oidc":{{"issuer":"{issuer}","client_id":"{CLIENT_ID}","client_secret_file":"{secret}","admit":{{"emails":[{{"email":"ada@example.com","principal":"ada","reach":"everything"}}]}}}}}}"#,
            issuer = provider.issuer,
            secret = secret.display(),
        ));
        Self {
            runtime,
            home,
            provider,
            port,
            table,
            bucket,
            ca,
            board,
            program,
        }
    }

    /// The origin that `flare login` and the commands use.
    pub fn origin(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// `args` with the generated CA as the only roots the process trusts
    /// and the stand-ins as the AWS endpoints, with made-up keys. The
    /// shipped `Stores::connect` reads these variables from the process
    /// environment, so no config key and no feature names an endpoint.
    fn command(&self, args: &[&str]) -> Command {
        let mut command = self.home.command(args);
        command
            .env("SSL_CERT_FILE", &self.ca)
            .env(
                "AWS_ENDPOINT_URL_DYNAMODB",
                stand_in_url("FLARE_TEST_DYNAMODB"),
            )
            .env("AWS_ENDPOINT_URL_S3", stand_in_url("FLARE_TEST_S3"))
            .env("AWS_ACCESS_KEY_ID", ACCESS_KEY_ID)
            .env("AWS_SECRET_ACCESS_KEY", SECRET_ACCESS_KEY);
        command
    }

    fn serve_args(&self) -> Vec<String> {
        vec![
            "serve".to_owned(),
            "--board".to_owned(),
            self.board.display().to_string(),
            "--board-program".to_owned(),
            self.program.display().to_string(),
        ]
    }

    /// Runs `flare serve` to its end, which it reaches only when it
    /// refuses to start.
    pub fn serve_to_end(&self) -> Ran {
        let args = self.serve_args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let output = self.command(&args).output().unwrap();
        Ran::from(output)
    }

    /// Starts `flare serve` and waits until it answers `/up`.
    pub fn serve(&self) -> Guard {
        let args = self.serve_args();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        #[expect(
            clippy::disallowed_methods,
            reason = "the test writes the server's log to a scratch file"
        )]
        let log = std::fs::File::create(self.home.path().join("serve.log")).unwrap();
        let mut command = self.command(&args);
        command.stdout(Stdio::null()).stderr(Stdio::from(log));
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let mut guard = Guard::new(command.spawn().unwrap());
        let client = Client::new();
        wait_for("flare serve to answer /up", || {
            if let Some(status) = guard.child().try_wait().unwrap() {
                let log = std::fs::read_to_string(self.home.path().join("serve.log"));
                panic!(
                    "flare serve ended with {status}: {}",
                    log.unwrap_or_default()
                );
            }
            self.runtime
                .block_on(self.get(&client, "/up", None))
                .is_ok_and(|response| response.status().is_success())
        });
        guard
    }

    /// A GET to the server as the proxy would send it: to 127.0.0.1 with
    /// the public origin's Host.
    ///
    /// # Errors
    ///
    /// Returns the error of the request.
    pub async fn get(
        &self,
        client: &Client,
        target: &str,
        extra: Option<(&str, &str)>,
    ) -> Result<Response, reqwest::Error> {
        let mut request = client
            .get(format!("http://127.0.0.1:{}{target}", self.port))
            .header(HOST, format!("localhost:{}", self.port));
        if let Some((name, value)) = extra {
            request = request.header(name, value);
        }
        request.send().await
    }

    /// Plays the person's browser through the sign-in that the URL `login`
    /// opened asks for, and delivers the login code to the CLI.
    pub async fn browser(&self, login: &str) {
        let plain = Client::builder()
            .redirect(redirect::Policy::none())
            .build()
            .unwrap();
        let tls = Client::builder()
            .redirect(redirect::Policy::none())
            .add_root_certificate(
                reqwest::Certificate::from_pem(self.provider.ca_pem.as_bytes()).unwrap(),
            )
            .resolve(
                "localhost",
                SocketAddr::from((Ipv4Addr::LOCALHOST, self.provider.port)),
            )
            .build()
            .unwrap();
        let wanted = Url::parse(login).unwrap();
        let back = format!("{}?{}", wanted.path(), wanted.query().unwrap_or_default());
        let start = format!(
            "/auth/oidc/start?{}",
            url::form_urlencoded::Serializer::new(String::new())
                .append_pair("return_to", &back)
                .finish()
        );
        let started = self.get(&plain, &start, None).await.unwrap();
        assert_eq!(started.status(), 303, "start: {started:?}");
        let kept = cookies(&started, "__Host-flare_oidc_state");
        let authorize = location(&started);
        let approved = tls.get(authorize).send().await.unwrap();
        assert_eq!(approved.status(), 302, "authorize: {approved:?}");
        let callback = Url::parse(&location(&approved)).unwrap();
        let target = format!("{}?{}", callback.path(), callback.query().unwrap());
        let signed = self
            .get(&plain, &target, Some((COOKIE.as_str(), &kept)))
            .await
            .unwrap();
        assert_eq!(signed.status(), 303, "callback: {signed:?}");
        let session = cookies(&signed, "__Host-flare_session");
        let handed = self
            .get(&plain, &back, Some((COOKIE.as_str(), &session)))
            .await
            .unwrap();
        assert_eq!(handed.status(), 303, "cli login: {handed:?}");
        let delivered = plain.get(location(&handed)).send().await.unwrap();
        assert!(
            delivered.status().is_success(),
            "the CLI's listener: {delivered:?}"
        );
    }

    /// Runs `flare login` against the server, plays the browser when the
    /// opener records the URL, and returns what the command printed.
    pub fn login(&self) -> Ran {
        let origin = self.origin();
        let mut command = self.command(&["login", &origin, "--json"]);
        command
            .env("FLARE_KEYCHAIN", self.home.keychain())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let child = Guard::new(command.spawn().unwrap());
        wait_for("flare login to open the browser", || {
            !self.home.opened().is_empty()
        });
        let url = self.home.opened().remove(0);
        self.runtime
            .block_on(self.browser(&url.display().to_string()));
        Ran::from(child.finish())
    }
}

/// The `Location` header.
fn location(response: &Response) -> String {
    response
        .headers()
        .get(LOCATION)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
        .unwrap_or_default()
}

/// The `name=value` pair of the cookie `name` that `response` sets.
fn cookies(response: &Response, name: &str) -> String {
    response
        .headers()
        .get_all(SET_COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .find(|value| value.starts_with(&format!("{name}=")) && !value.contains("Max-Age=0"))
        .and_then(|value| value.split(';').next())
        .map(str::to_owned)
        .unwrap_or_else(|| panic!("the response sets no {name} cookie: {response:?}"))
}
