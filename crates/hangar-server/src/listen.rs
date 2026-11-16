//! [`Listeners`]: the sockets, the accept loop, graceful shutdown and the
//! idle exit.

use crate::app::{App, Kind};
use crate::schemas::Schemas;
use crate::tasks::{self, Tasks};
use hangar_auth_oidc::Clock as OidcClock;
use hangar_core::{PageStore, RecordStore};
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper_util::rt::{TokioIo, TokioTimer};
use hyper_util::server::graceful::GracefulShutdown;
use std::convert::Infallible;
use std::future::Future;
use std::io;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, TcpListener as StdListener};
use std::pin::pin;
use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::Semaphore;

/// Why the listeners could not be bound.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum BindError {
    /// Another program holds the port.
    #[error("port {port} is in use")]
    PortInUse {
        /// The port.
        port: u16,
    },
    /// The operating system refused the socket for another reason.
    #[error("the socket could not be bound")]
    Io(#[source] io::Error),
}

/// Why serving stopped with an error.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ServeError {
    /// The state directory could not be prepared or the server file could
    /// not be written, so no client can find the server.
    #[error("the server file could not be written")]
    State(#[source] hangar_auth_loopback::StateError),
    /// A listener could not be handed to the runtime.
    #[error("a listener failed")]
    Listener(#[source] io::Error),
}

/// Why serving stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Stopped {
    /// The shutdown future resolved.
    Shutdown,
    /// No request arrived for [`Limits::idle_exit`](crate::Limits::idle_exit).
    Idle,
}

/// The bound sockets: the launch host's on `127.0.0.1`, and with OpenID
/// Connect the sign-in host's on `127.0.0.1` and `::1`.
#[derive(Debug)]
pub struct Listeners {
    launch: StdListener,
    oidc: Vec<StdListener>,
    oidc_port: Option<u16>,
}

impl Listeners {
    /// Binds `127.0.0.1:<port>`, where 0 picks a free port, and with
    /// `oidc` also `127.0.0.1:<oidc>` and `[::1]:<oidc>`. `hangar serve`
    /// passes 8740, the port of the registered redirect URI.
    ///
    /// # Errors
    ///
    /// Returns [`BindError::PortInUse`] when a port is held and
    /// [`BindError::Io`] for any other refusal.
    pub fn bind(port: u16, oidc: Option<u16>) -> Result<Self, BindError> {
        let launch = listen(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        let mut sockets = Vec::new();
        let mut oidc_port = None;
        if let Some(wanted) = oidc {
            let v4 = listen(SocketAddr::from((Ipv4Addr::LOCALHOST, wanted)))?;
            let bound = v4.local_addr().map_err(BindError::Io)?.port();
            sockets.push(v4);
            match listen(SocketAddr::from((Ipv6Addr::LOCALHOST, bound))) {
                Ok(v6) => sockets.push(v6),
                // A machine without IPv6 loopback serves the sign-in host on
                // 127.0.0.1 alone; a browser that tries ::1 first falls back.
                Err(BindError::Io(error)) if error.kind() == io::ErrorKind::AddrNotAvailable => {}
                Err(other) => return Err(other),
            }
            oidc_port = Some(bound);
        }
        Ok(Self {
            launch,
            oidc: sockets,
            oidc_port,
        })
    }

    /// The launch host's port, which `Loopback::new` needs.
    ///
    /// # Panics
    ///
    /// Never: a bound socket always has an address.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.launch.local_addr().map_or(0, |address| address.port())
    }

    /// The sign-in host's port, when OpenID Connect is on.
    #[must_use]
    pub fn oidc_port(&self) -> Option<u16> {
        self.oidc_port
    }

    /// Writes the server file, serves until `shutdown` resolves or the idle
    /// period passes, then stops accepting, lets requests in work finish
    /// for at most [`Limits::drain`](crate::Limits::drain), removes the
    /// server file and the openers, and returns.
    ///
    /// # Errors
    ///
    /// Returns [`ServeError::State`] when the server file cannot be written,
    /// before any request is served, and [`ServeError::Listener`] when a
    /// socket cannot join the runtime.
    pub async fn serve<P, R, S, K>(
        self,
        app: App<P, R, S, K>,
        shutdown: impl Future<Output = ()>,
    ) -> Result<Stopped, ServeError>
    where
        P: PageStore + 'static,
        R: RecordStore + 'static,
        S: Schemas + 'static,
        K: OidcClock,
    {
        let oidc_port = self.oidc_port;
        let launch = runtime_listener(self.launch)?;
        let mut oidc = Vec::new();
        for socket in self.oidc {
            oidc.push(runtime_listener(socket)?);
        }
        app.open_files().map_err(ServeError::State)?;
        if let Some(port) = oidc_port {
            app.serve_sign_in_on(port);
        }
        let limits = app.limits().clone();
        let connections = Arc::new(Semaphore::new(limits.connections));
        let graceful = GracefulShutdown::new();
        let mut owned = Tasks::default();
        let mut sweeper = Tasks::default();
        sweeper.spawn(app.clone().sweep_forever());
        let mut shutdown = pin!(shutdown);
        let reason = loop {
            let accepted = async {
                let permit = Arc::clone(&connections).acquire_owned().await;
                let accepted = accept(&launch, &oidc, oidc_port).await;
                (permit, accepted)
            };
            tokio::select! {
                () = &mut shutdown => break Stopped::Shutdown,
                () = tasks::until(app.idle_deadline()) => {
                    if app.idle_passed() {
                        break Stopped::Idle;
                    }
                }
                () = owned.reap() => {}
                (permit, accepted) = accepted => {
                    let (Ok(permit), Ok((stream, peer, kind))) = (permit, accepted) else {
                        continue;
                    };
                    let app = app.clone();
                    let io = TokioIo::new(stream);
                    let service = service_fn(move |request| {
                        let app = app.clone();
                        let kind = kind.clone();
                        async move { Ok::<_, Infallible>(app.respond(kind, peer, request).await) }
                    });
                    let mut builder = http1::Builder::new();
                    builder
                        .timer(TokioTimer::new())
                        .header_read_timeout(limits.read_time)
                        .max_buf_size(limits.header_bytes.saturating_mul(2).max(8192));
                    let connection = graceful.watch(builder.serve_connection(io, service));
                    owned.spawn(async move {
                        let _ = connection.await;
                        drop(permit);
                    });
                }
            }
        };
        drop(launch);
        drop(oidc);
        let drained = async {
            graceful.shutdown().await;
        };
        let _ = tasks::within(limits.drain, drained).await;
        owned.finish(std::time::Duration::ZERO).await;
        sweeper.abort().await;
        app.close_files();
        Ok(reason)
    }
}

fn listen(address: SocketAddr) -> Result<StdListener, BindError> {
    let socket = StdListener::bind(address).map_err(|error| {
        if error.kind() == io::ErrorKind::AddrInUse {
            BindError::PortInUse {
                port: address.port(),
            }
        } else {
            BindError::Io(error)
        }
    })?;
    socket.set_nonblocking(true).map_err(BindError::Io)?;
    Ok(socket)
}

fn runtime_listener(socket: StdListener) -> Result<TcpListener, ServeError> {
    TcpListener::from_std(socket).map_err(ServeError::Listener)
}

/// Accepts the next connection on any listener. Each `accept` is
/// cancel-safe, so a `select!` that drops this future loses no connection.
async fn accept(
    launch: &TcpListener,
    oidc: &[TcpListener],
    oidc_port: Option<u16>,
) -> io::Result<(TcpStream, std::net::IpAddr, Kind)> {
    let on_launch = async {
        let (stream, peer) = launch.accept().await?;
        Ok::<_, io::Error>((stream, peer.ip(), Kind::Launch))
    };
    let on_oidc = async {
        match (oidc, oidc_port) {
            ([first, rest @ ..], Some(port)) => {
                let second = async {
                    match rest.first() {
                        Some(listener) => listener.accept().await,
                        None => std::future::pending().await,
                    }
                };
                let (stream, peer) = tokio::select! {
                    accepted = first.accept() => accepted?,
                    accepted = second => accepted?,
                };
                Ok((stream, peer.ip(), Kind::Oidc { port }))
            }
            _ => std::future::pending().await,
        }
    };
    tokio::select! {
        accepted = on_launch => accepted,
        accepted = on_oidc => accepted,
    }
}
