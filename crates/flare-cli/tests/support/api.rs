//! A fake share API, a fake Keychain program and a login opener, so the
//! sharing commands run against a server and a Keychain of the test's own.
//!
//! The fake API answers each request with the first route that matches its
//! method and path, and drops that route, so two routes for one path
//! answer two calls in order. It keeps every request. The Keychain program
//! keeps one item in a file and logs each command line it is given, so a
//! test can show that no token is on a command line. The login opener
//! follows `/cli/login` as a signed-in browser would: it calls the
//! command's loopback listener with a code and the state from the URL.

use crate::run::Home;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

/// The login code that the opener hands to the command.
pub const CODE: &str = "abcdefghijklmnopqrstuvwxyz0123456789ABCDEFG";

/// The token that the fake exchange issues.
pub const TOKEN: &str =
    "flare_0123456789abcdef0123456789abcdef_SECRETsecretSECRETsecretSECRETsecret1234567";

/// The id of [`TOKEN`].
pub const TOKEN_ID: &str = "0123456789abcdef0123456789abcdef";

/// A flare's id.
pub const FLARE: &str = "3f9c0a17e2b44d6f8a1c5b7e9d20f4a6";

/// One request that the fake API received.
#[derive(Debug, Clone)]
pub struct Request {
    /// The method.
    pub method: String,
    /// The path with its query.
    pub target: String,
    /// The headers, with lower-case names.
    pub headers: HashMap<String, String>,
    /// The body.
    pub body: String,
}

impl Request {
    /// The body as JSON.
    pub fn json(&self) -> Value {
        serde_json::from_str(&self.body).unwrap()
    }
}

/// One canned answer.
#[derive(Debug, Clone)]
struct Route {
    method: &'static str,
    path: String,
    status: u16,
    content_type: &'static str,
    body: String,
    /// The `content-length` the answer declares, which is longer than the
    /// body for an answer cut short.
    length: usize,
}

/// The fake share API.
#[derive(Debug, Clone)]
pub struct FakeApi {
    /// The origin, such as `http://127.0.0.1:4000`.
    pub origin: String,
    routes: Arc<Mutex<Vec<Route>>>,
    requests: Arc<Mutex<Vec<Request>>>,
}

impl FakeApi {
    /// Starts the fake API on a free port.
    pub fn start() -> Self {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let origin = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
        let api = Self {
            origin,
            routes: Arc::new(Mutex::new(Vec::new())),
            requests: Arc::new(Mutex::new(Vec::new())),
        };
        let served = api.clone();
        #[expect(
            clippy::disallowed_methods,
            reason = "the fake API's thread answers until the test process ends"
        )]
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                served.answer(stream);
            }
        });
        api
    }

    /// Answers the next `method` request to `path` with `status` and the
    /// JSON `body`.
    pub fn route(&self, method: &'static str, path: &str, status: u16, body: &Value) {
        let content_type = if status >= 400 {
            "application/problem+json"
        } else {
            "application/json"
        };
        self.routes.lock().unwrap().push(Route {
            method,
            path: path.to_owned(),
            status,
            content_type,
            length: body.to_string().len(),
            body: body.to_string(),
        });
    }

    /// Reads the next `method` request to `path` and closes the connection
    /// without an answer, as a server that dropped the request after it
    /// arrived would. A route with status 0 stands for this.
    pub fn route_silent(&self, method: &'static str, path: &str) {
        self.route_raw(method, path, 0, "", 0);
    }

    /// Answers the next `method` request to `path` with `status`, the text
    /// `body` and a `content-length` of `length`, so a `length` longer than
    /// the body cuts the answer short as a dropped connection would.
    pub fn route_raw(
        &self,
        method: &'static str,
        path: &str,
        status: u16,
        body: &str,
        length: usize,
    ) {
        self.routes.lock().unwrap().push(Route {
            method,
            path: path.to_owned(),
            status,
            content_type: "application/json",
            body: body.to_owned(),
            length,
        });
    }

    /// Answers the next exchange with [`TOKEN`] for `ana@example.org`.
    pub fn issue(&self) {
        self.route(
            "POST",
            "/api/tokens/exchange",
            201,
            &json!({
                "id": TOKEN_ID,
                "token": TOKEN,
                "owner": "ana@example.org",
                "label": "ignored",
                "expires": "2027-02-21T10:00:00Z",
            }),
        );
    }

    /// Every request so far.
    pub fn requests(&self) -> Vec<Request> {
        self.requests.lock().unwrap().clone()
    }

    /// The requests to `path`, whatever their query.
    pub fn requests_to(&self, path: &str) -> Vec<Request> {
        self.requests()
            .into_iter()
            .filter(|request| request.target.split('?').next() == Some(path))
            .collect()
    }

    fn answer(&self, stream: TcpStream) {
        let mut reader = BufReader::new(stream.try_clone().unwrap());
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let mut parts = line.split_whitespace();
        let method = parts.next().unwrap_or_default().to_owned();
        let target = parts.next().unwrap_or_default().to_owned();
        let mut headers = HashMap::new();
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).unwrap();
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some((name, value)) = header.split_once(':') {
                headers.insert(name.trim().to_ascii_lowercase(), value.trim().to_owned());
            }
        }
        let length: usize = headers
            .get("content-length")
            .and_then(|value| value.parse().ok())
            .unwrap_or(0);
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let path = target.split('?').next().unwrap_or_default().to_owned();
        self.requests.lock().unwrap().push(Request {
            method: method.clone(),
            target,
            headers,
            body: String::from_utf8(body).unwrap(),
        });
        let route = {
            let mut routes = self.routes.lock().unwrap();
            let found = routes
                .iter()
                .position(|route| route.method == method && route.path == path);
            found.map(|index| routes.remove(index))
        };
        if route.as_ref().is_some_and(|route| route.status == 0) {
            return;
        }
        let (status, content_type, body, length) = route.map_or_else(
            || (404, "text/plain", "no route".to_owned(), 8),
            |route| (route.status, route.content_type, route.body, route.length),
        );
        let mut stream = stream;
        let _ = write!(
            stream,
            "HTTP/1.1 {status} X\r\ncontent-type: {content_type}\r\ncontent-length: {length}\r\nconnection: close\r\n\r\n{body}"
        );
    }
}

/// A problem document with `code`, as the server sends it.
pub fn problem(code: &str, status: u16, server: &str) -> Value {
    json!({
        "type": format!("https://flare.dev/problems/{code}"),
        "title": "refused",
        "status": status,
        "code": code,
        "recovery": {"kind": "run", "call": format!("flare login {server}")},
    })
}

/// The fake Keychain program and the login opener in `home`.
pub trait Fakes {
    /// The fake `security` program.
    fn keychain(&self) -> PathBuf;
    /// The item the fake Keychain holds, as `<account> <secret>`.
    fn keychain_item(&self) -> Option<String>;
    /// Every command line the fake Keychain was given.
    fn keychain_calls(&self) -> String;
    /// An opener that follows `/cli/login` with `state`, or with the state
    /// in the URL when `state` is `None`.
    fn login_opener(&self, state: Option<&str>) -> PathBuf;
    /// The binary with `args`, the fake Keychain and the login opener.
    fn sharing(&self, args: &[&str]) -> crate::run::Ran;
}

impl Fakes for Home {
    fn keychain(&self) -> PathBuf {
        let item = self.path().join("keychain-item");
        let calls = self.path().join("keychain-calls");
        self.script(
            "security",
            &format!(
                r#"item='{item}'
printf '%s\n' "$*" >> '{calls}'
if [ "$1" = "-i" ]; then
  read -r line
  set -- $line
fi
command="$1"; shift
account=''; secret=''
while [ $# -gt 0 ]; do
  case "$1" in
    -a) account="$2"; shift 2 ;;
    -w) if [ $# -gt 1 ]; then secret="$2"; shift 2; else shift; fi ;;
    *) shift ;;
  esac
done
case "$command" in
  add-generic-password) printf '%s %s\n' "$account" "$secret" > "$item" ;;
  find-generic-password)
    [ -f "$item" ] || exit 44
    set -- $(cat "$item")
    [ "$1" = "$account" ] || exit 44
    printf '%s\n' "$2" ;;
  delete-generic-password) [ -f "$item" ] || exit 44; rm "$item" ;;
  *) exit 2 ;;
esac
"#,
                item = item.display(),
                calls = calls.display(),
            ),
        )
    }

    fn keychain_item(&self) -> Option<String> {
        std::fs::read_to_string(self.path().join("keychain-item"))
            .ok()
            .map(|text| text.trim_end().to_owned())
    }

    fn keychain_calls(&self) -> String {
        std::fs::read_to_string(self.path().join("keychain-calls")).unwrap_or_default()
    }

    fn login_opener(&self, state: Option<&str>) -> PathBuf {
        let log = self.path().join("opened.txt");
        let state = state.map_or_else(
            || {
                r#"$(printf '%s' "$1" | sed -n 's/.*[?&]state=\([A-Za-z0-9_-]*\).*/\1/p')"#
                    .to_owned()
            },
            ToOwned::to_owned,
        );
        self.script(
            "login-opener",
            &format!(
                r#"printf '%s\n' "$1" >> '{log}'
port=$(printf '%s' "$1" | sed -n 's/.*[?&]port=\([0-9]*\).*/\1/p')
state="{state}"
curl -s --max-time 5 "http://127.0.0.1:$port/callback?code={CODE}&state=$state" > /dev/null
"#,
                log = log.display(),
            ),
        )
    }

    fn sharing(&self, args: &[&str]) -> crate::run::Ran {
        let keychain = self.keychain();
        let existing = self.path().join("login-opener");
        let opener = if existing.exists() {
            existing
        } else {
            self.login_opener(None)
        };
        #[expect(clippy::disallowed_methods, reason = "the tests run the built binary")]
        let output = self
            .command(args)
            .env("FLARE_KEYCHAIN", keychain)
            .env("FLARE_OPENER", opener)
            .output()
            .unwrap();
        crate::run::Ran::from(output)
    }
}
