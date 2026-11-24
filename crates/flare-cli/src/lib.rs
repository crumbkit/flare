//! The `flare` command: serve a board's pages and bindings on this machine,
//! open them in a browser, publish pages and save bindings, and share files
//! through a flare server's share API.
//!
//! The library holds the whole command, so its tests can pass the parser
//! and the tool record to crumbkit-test. The binary calls [`run`].
//!
//! The command is thin. [`run`] parses with crumbkit, reads the environment
//! once into a snapshot, and each command builds flare-board's store,
//! flare-auth-loopback's state and flare-auth-oidc's authenticator and
//! calls flare-server. `login`, `logout`, `share`, `show`, `grant`,
//! `shares` and `activity` call a flare server's share API with the token
//! that `login` keeps in the Keychain; `share --dry-run` calls no server.
//! `metrics` totals the metric lines of a server's log. `serve` is the only long-running command; the
//! others run to their end on a runtime of their own.

mod bind;
mod board;
mod cli;
mod client;
pub mod codes;
mod config;
mod documents;
mod duration;
mod env;
mod keychain;
mod log;
mod login;
mod metrics;
mod open;
mod opener;
mod pages;
mod publish;
mod serve;
mod sharing;
mod teams;

pub use cli::{Cli, TOOL};
pub use codes::CODES;

use cli::{CommandsVerb, SKILL, SchemasCommand, SelfVerb, TeamsVerb, Verb};
use codes::Failed;
use env::Snapshot;
use std::process::ExitCode;

/// The exit code after Ctrl-C (CLI-8).
const INTERRUPTED: u8 = 130;

/// Parses the command line, runs the command and returns the exit code.
/// `interrupted` marks the usage log's line as a run that Ctrl-C ended.
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
        Verb::Login(args) => {
            let result = block_on(false, login::login(&args, &env));
            crumbkit::finish(run, documents::LOGIN, result)
        }
        Verb::Logout(args) => {
            let question = "Revoke this computer's flare token and delete it from the Keychain?";
            if let Err(refused) = run.confirm(args.yes, question) {
                return crumbkit::finish(run, documents::LOGOUT, Err(refused));
            }
            let result = block_on(false, login::logout(&env));
            crumbkit::finish(run, documents::LOGOUT, result)
        }
        Verb::Share(args) => {
            let dry_run = run.is_dry_run();
            let result = block_on(false, sharing::share(&args, dry_run, &env));
            crumbkit::finish(run, documents::SHARE, result)
        }
        Verb::Show(args) => {
            let result = block_on(false, sharing::show(&args, &env));
            crumbkit::finish(run, documents::SHOW, result)
        }
        Verb::Grant(args) => {
            let result = block_on(false, sharing::grant(&args, &env));
            crumbkit::finish(run, documents::GRANT, result)
        }
        Verb::Revoke(args) => {
            let question = format!("End these grants on the share {}?", args.id);
            if let Err(refused) = run.confirm(args.yes, &question) {
                return crumbkit::finish(run, documents::REVOKE, Err(refused));
            }
            let result = block_on(false, sharing::revoke(&args, &env));
            crumbkit::finish(run, documents::REVOKE, result)
        }
        Verb::Grants(args) => {
            let result = block_on(false, sharing::grants(&args, &env));
            crumbkit::finish(run, documents::GRANTS, result)
        }
        Verb::Teams { command } => run_teams(run, command, &env),
        Verb::Shares(args) => {
            let result = block_on(false, sharing::shares(&args, &env));
            crumbkit::finish(run, documents::SHARES, result)
        }
        Verb::Activity(args) => {
            let result = block_on(false, sharing::activity(&args, &env));
            crumbkit::finish(run, documents::ACTIVITY, result)
        }
        Verb::Metrics(args) => {
            let result = metrics::run(&args);
            crumbkit::finish(run, documents::METRICS, result)
        }
        Verb::Commands {
            command: CommandsVerb::List(args),
        } => {
            let result = crumbkit::commands::list(&mut run, &TOOL, &args);
            crumbkit::finish(run, crumbkit::commands::COMMANDS_LIST, result)
        }
        Verb::Schemas {
            command: SchemasCommand::List(args),
        } => {
            let result = crumbkit::schemas::list(&mut run, &TOOL, &args);
            crumbkit::finish(run, crumbkit::schemas::SCHEMAS_LIST, result)
        }
        Verb::Schemas {
            command: SchemasCommand::Show(args),
        } => {
            let result = crumbkit::schemas::show(&mut run, &TOOL, &args);
            crumbkit::finish(run, crumbkit::schemas::SCHEMAS_SHOW, result)
        }
        Verb::SelfCommand {
            command: SelfVerb::Usage(args),
        } => {
            let result = Ok::<_, crumbkit::Error>(crumbkit_usage::findings(&mut run, &TOOL, &args));
            crumbkit::finish(run, crumbkit_usage::SELF_USAGE, result)
        }
        Verb::Skills { command } => crumbkit_skills::run(run, &[SKILL], command),
    }
}

/// Runs one `flare teams` command.
fn run_teams(mut run: crumbkit::Run, command: TeamsVerb, env: &Snapshot) -> ExitCode {
    use documents::teams::{
        TEAMS_ADD, TEAMS_CREATE, TEAMS_DELETE, TEAMS_LIST, TEAMS_REMOVE, TEAMS_SHOW, TEAMS_TRANSFER,
    };
    match command {
        TeamsVerb::Create(args) => {
            let result = block_on(false, teams::create(&args.name, env));
            crumbkit::finish(run, TEAMS_CREATE, result)
        }
        TeamsVerb::Add(args) => {
            let result = block_on(false, teams::members(&args.name, &args.with, true, env));
            crumbkit::finish(run, TEAMS_ADD, result)
        }
        TeamsVerb::Remove(args) => {
            let question = format!("Remove these people from the team {}?", args.members.name);
            if let Err(refused) = run.confirm(args.yes, &question) {
                return crumbkit::finish(run, TEAMS_REMOVE, Err(refused));
            }
            let members = &args.members;
            let result = block_on(
                false,
                teams::members(&members.name, &members.with, false, env),
            );
            crumbkit::finish(run, TEAMS_REMOVE, result)
        }
        TeamsVerb::List(args) => {
            let result = block_on(false, teams::list(&args, env));
            crumbkit::finish(run, TEAMS_LIST, result)
        }
        TeamsVerb::Show(args) => {
            let result = block_on(false, teams::show(&args.names, env));
            crumbkit::finish(run, TEAMS_SHOW, result)
        }
        TeamsVerb::Transfer(args) => {
            let question = format!(
                "Hand the team {} to {}? They become its owner, and you can no longer manage it.",
                args.name, args.to
            );
            if let Err(refused) = run.confirm(args.yes, &question) {
                return crumbkit::finish(run, TEAMS_TRANSFER, Err(refused));
            }
            let result = block_on(false, teams::transfer(&args.name, &args.to, env));
            crumbkit::finish(run, TEAMS_TRANSFER, result)
        }
        TeamsVerb::Delete(args) => {
            let question = format!(
                "Delete the team {}? Its grant is removed from every flare, its members lose that access, and its name becomes free.",
                args.name
            );
            if let Err(refused) = run.confirm(args.yes, &question) {
                return crumbkit::finish(run, TEAMS_DELETE, Err(refused));
            }
            let result = block_on(false, teams::delete(&args.name, env));
            crumbkit::finish(run, TEAMS_DELETE, result)
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
