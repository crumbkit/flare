//! The `hangar` command: serve a board's pages and bindings on this machine,
//! open them in a browser, publish pages and save bindings.
//!
//! The library holds the whole command, so its tests can pass the parser
//! and the tool record to crumbkit-test. The binary calls [`run`].
//!
//! The command is thin. [`run`] parses with crumbkit, reads the environment
//! once into a snapshot, and each command builds hangar-board's store,
//! hangar-auth-loopback's state and hangar-auth-oidc's authenticator and
//! calls hangar-server. `serve` is the only long-running command; the
//! others run to their end on a runtime of their own.

mod bind;
mod board;
mod cli;
pub mod codes;
mod config;
mod documents;
mod duration;
mod env;
mod log;
mod open;
mod opener;
mod pages;
mod publish;
mod serve;

pub use cli::{Cli, TOOL};
pub use codes::CODES;

use cli::{CommandsVerb, SelfVerb, Verb};
use codes::Failed;
use env::Snapshot;
use std::process::ExitCode;

/// The exit code after Ctrl-C (CLI-8).
const INTERRUPTED: u8 = 130;

/// Parses the command line, runs the command and returns the exit code.
/// `interrupted` marks the usage log's line as a run that Ctrl-C ended.
#[must_use]
pub fn run(interrupted: impl Fn()) -> ExitCode {
    let (cli, mut run) = match crumbkit::parse::<Cli>(&TOOL) {
        Ok(parsed) => parsed,
        Err(stop) => return stop.exit_code(),
    };
    let env = Snapshot::capture();
    match cli.command {
        Verb::Serve(args) => match block_on(true, serve::run(&args, &env)) {
            Ok((served, serve::Stop::Interrupted)) => {
                interrupted();
                let _ = crumbkit::finish(run, documents::SERVE, Ok::<_, Failed>(served));
                ExitCode::from(INTERRUPTED)
            }
            result => crumbkit::finish(run, documents::SERVE, result.map(|(served, _)| served)),
        },
        Verb::Open(args) => {
            let result = block_on(false, open::run(&args, &env));
            crumbkit::finish(run, documents::OPEN, result)
        }
        Verb::Publish(args) => {
            let dry_run = run.is_dry_run();
            let result = block_on(false, publish::run(&args, dry_run, &env));
            crumbkit::finish(run, documents::PUBLISH, result)
        }
        Verb::Bind(args) => {
            let dry_run = run.is_dry_run();
            let result = block_on(false, bind::run(&args, dry_run, &env));
            crumbkit::finish(run, documents::BIND, result)
        }
        Verb::Commands {
            command: CommandsVerb::List(args),
        } => {
            let result = crumbkit::commands::list(&mut run, &TOOL, &args);
            crumbkit::finish(run, crumbkit::commands::COMMANDS_LIST, result)
        }
        Verb::SelfCommand {
            command: SelfVerb::Usage(args),
        } => {
            let result = Ok::<_, crumbkit::Error>(crumbkit_usage::findings(&mut run, &TOOL, &args));
            crumbkit::finish(run, crumbkit_usage::SELF_USAGE, result)
        }
    }
}

/// Runs `future` to its end on a new runtime: one with a thread per core
/// for `serve`, and one on this thread for the other commands.
fn block_on<T>(serve: bool, future: impl Future<Output = Result<T, Failed>>) -> Result<T, Failed> {
    let mut builder = if serve {
        tokio::runtime::Builder::new_multi_thread()
    } else {
        tokio::runtime::Builder::new_current_thread()
    };
    match builder.enable_all().build() {
        Ok(runtime) => runtime.block_on(future),
        Err(error) => Err(Failed::System {
            problem: format!("the async runtime could not start: {error}"),
        }),
    }
}
