//! The `flare` binary, which runs the command that the flare-cli library
//! holds.

use std::process::ExitCode;
use std::sync::OnceLock;

/// The usage log's writer for a run that Ctrl-C ends, which
/// `crumbkit_usage::start_under` hands over.
static INTERRUPTED: OnceLock<fn()> = OnceLock::new();

fn main() -> ExitCode {
    // The usage log starts first (CLI-7). The guard writes this run's line
    // when `main` returns, after the output is flushed; `flare serve`
    // writes it early, marked as interrupted, after Ctrl-C.
    let _log = crumbkit_usage::start_under(|write| {
        let _ = INTERRUPTED.set(write);
    });
    flare_cli::run(|| {
        if let Some(write) = INTERRUPTED.get() {
            write();
        }
    })
}
