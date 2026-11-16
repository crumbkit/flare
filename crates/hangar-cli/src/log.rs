//! The server's structured log on standard error (SVC-9). crumbkit's
//! output owns every other write to the standard streams (CLI-1); this
//! module owns the log that `hangar serve` writes while it runs, which
//! hangar-server's request lines go to.

/// Sends `tracing` events to standard error, one line each, without
/// colour, for the rest of the process.
pub(crate) fn start() {
    #[expect(
        clippy::disallowed_methods,
        reason = "the server's log is the one writer to standard error outside crumbkit's output (CLI-1)"
    )]
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(false)
        .with_writer(std::io::stderr)
        .finish();
    // A second call keeps the first subscriber, which writes to the same
    // stream.
    let _ = tracing::subscriber::set_global_default(subscriber);
}
