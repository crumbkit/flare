//! Every failure of the `flare` command, with its stable code and the next
//! step it names (CLI-3). No message holds a token, a login code, a launch
//! host, a launch URL or a client secret.

use crumbkit::{Call, Failure, Recovery};

mod all;
pub mod search;
pub mod server;
pub mod sharing;
pub mod sign_in;
mod spelled;

#[cfg(test)]
mod tests;

pub use all::CODES;
pub use server::HeldPort;
pub use sharing::{NextRun, Refused, SharingFailed};
pub use sign_in::SignInAgain;
pub use spelled::{
    BINDINGS_INVALID, BINDINGS_MISFIT, BINDINGS_NAME_TAKEN, BINDINGS_RECORD_NOT_FOUND,
    BOARD_NOT_A_BOARD, BOARD_NOT_FOUND, BOARD_PROGRAM_UNAVAILABLE, BOARD_TOO_OLD,
    BOARD_UNAVAILABLE, CONFIG_IDENTITY_CHANGED, CONFIG_INVALID, CONFIG_UNREADABLE,
    METRICS_UNREADABLE, OIDC_PROVIDER_UNAVAILABLE, OIDC_SECRET_UNAVAILABLE, OPENER_FAILED,
    PAGES_INVALID, PAGES_NOT_FOUND, PAGES_TEMPLATE_FLAWS, PAGES_UNKNOWN_FORMAT, PAGES_UNREADABLE,
    PROXY_KEYS_UNAVAILABLE, SERVER_ALREADY_RUNNING, SERVER_NOT_RUNNING, SERVER_PORT_IN_USE,
    STATE_UNUSABLE, STORES_UNAVAILABLE, SYSTEM_UNAVAILABLE, TEAMS_DOCUMENT_REFUSED,
};

/// Why a `flare` command failed.
#[derive(Debug, thiserror::Error, Failure)]
pub enum Failed {
    /// No server answers for the board.
    #[error("no flare server answers for this board: {reason}")]
    #[failure(code = SERVER_NOT_RUNNING, recovery = server::serve(board.as_deref()))]
    NotRunning {
        /// What `open` found.
        reason: String,
        /// The `--board` that the command was given.
        board: Option<String>,
    },
    /// A server already serves the board.
    #[error("a flare server already serves this board")]
    #[failure(code = SERVER_ALREADY_RUNNING, recovery = server::open(board.as_deref()))]
    AlreadyRunning {
        /// The `--board` that the command was given.
        board: Option<String>,
    },
    /// Another program holds the port.
    #[error("port {port} is in use by another program")]
    #[failure(
        code = SERVER_PORT_IN_USE,
        recovery = server::port_in_use(*port, *held, again.clone()),
        field(port = *port)
    )]
    PortInUse {
        /// The port.
        port: u16,
        /// Which port it is.
        held: HeldPort,
        /// The failed `flare serve` with its arguments and without `--port`.
        again: Option<Call>,
    },
    /// The board's directory does not exist.
    #[error("{path} does not exist")]
    #[failure(
        code = BOARD_NOT_FOUND,
        hint = "pass --board with the directory that holds .board/board.json",
        field(path = path)
    )]
    BoardNotFound {
        /// The directory given.
        path: String,
    },
    /// The directory holds no board.
    #[error("{path} holds no .board/board.json, so it is not a board")]
    #[failure(
        code = BOARD_NOT_A_BOARD,
        hint = "pass --board with a board's directory",
        alternative = Recovery::hint("run `board init` in this directory to make it a board"),
        field(path = path)
    )]
    NotABoard {
        /// The directory.
        path: String,
    },
    /// The board program has no `log --at`.
    #[error(
        "the board program at {program} has no `log --at`; flare needs a build of crumbkit/board e4ee3333 or later"
    )]
    #[failure(
        code = BOARD_TOO_OLD,
        hint = "pass --board-program with a board built from crumbkit/board e4ee3333 or later",
        field(program = program)
    )]
    TooOld {
        /// The program's path.
        program: String,
    },
    /// The board program was not found or did not answer.
    #[error("the board program {}{problem}", before(.program.as_deref(), ": "))]
    #[failure(
        code = BOARD_PROGRAM_UNAVAILABLE,
        hint = "pass --board-program with the absolute path of a board program",
        alternative = Recovery::hint("set FLARE_BOARD to the absolute path of a board program"),
        field(program = program.as_deref())
    )]
    ProgramUnavailable {
        /// The program's path, when flare knows it.
        program: Option<String>,
        /// What went wrong.
        problem: String,
    },
    /// The board could not answer.
    #[error(
        "the board could not answer: {problem}. A first publish that was cut short leaves the page unreadable until flare-board recovers it"
    )]
    #[failure(code = BOARD_UNAVAILABLE, recovery = Recovery::Retry)]
    BoardUnavailable {
        /// What went wrong.
        problem: String,
    },
    /// The config file could not be read.
    #[error("the config file {path} could not be read: {problem}")]
    #[failure(
        code = CONFIG_UNREADABLE,
        hint = "make the config file readable",
        alternative = Recovery::hint("pass --config with another file"),
        field(path = path)
    )]
    ConfigUnreadable {
        /// The file.
        path: String,
        /// What went wrong.
        problem: String,
    },
    /// The config file is not a flare config.
    #[error("the config file {path} is not a flare config: {problem}")]
    #[failure(
        code = CONFIG_INVALID,
        hint = "fix the config file as the problem says; the README describes each key",
        field(path = path)
    )]
    ConfigInvalid {
        /// The file.
        path: String,
        /// What is wrong.
        problem: String,
    },
    /// The client secret could not be read.
    #[error(
        "the client secret could not be read from the Keychain service flare-oidc-google: {problem}"
    )]
    #[failure(
        code = OIDC_SECRET_UNAVAILABLE,
        hint = "add the client secret to the Keychain as a generic password with the service flare-oidc-google"
    )]
    SecretUnavailable {
        /// What went wrong.
        problem: String,
    },
    /// The client secret file could not be read or is not safe to read.
    #[error("the client secret file {path} could not be used: {problem}")]
    #[failure(
        code = OIDC_SECRET_UNAVAILABLE,
        hint = "make oidc.client_secret_file name a regular file of 1 to 4096 bytes that the user flare runs as or root owns, with mode 0400 or 0600",
        field(path = path)
    )]
    SecretFileUnavailable {
        /// The file.
        path: String,
        /// Which rule it broke, never its content.
        problem: String,
    },
    /// The provider could not be reached or is misconfigured.
    #[error("the OpenID Connect provider is unavailable: {problem}")]
    #[failure(code = OIDC_PROVIDER_UNAVAILABLE, recovery = Recovery::Retry)]
    ProviderUnavailable {
        /// What went wrong.
        problem: String,
    },
    /// The single sign-on proxy's key set could not be fetched or holds no
    /// usable key, so `flare serve` binds nothing.
    #[error(
        "the single sign-on proxy's key set at proxy.jwks_url ({url}) is unavailable: {problem}"
    )]
    #[failure(
        code = PROXY_KEYS_UNAVAILABLE,
        hint = "check that proxy.jwks_url is the proxy's key set address and that the proxy publishes an RS256 or ES256 key there",
        field(url = url)
    )]
    ProxyKeysUnavailable {
        /// The configured key set address.
        url: String,
        /// What went wrong.
        problem: String,
    },
    /// No page or binding has the name.
    #[error("the board has no page or binding named {name}")]
    #[failure(
        code = PAGES_NOT_FOUND,
        hint = "name a page or binding that the board holds",
        field(name = name)
    )]
    PageNotFound {
        /// The name.
        name: String,
    },
    /// The template has flaws.
    #[error("{flaws}")]
    #[failure(code = PAGES_TEMPLATE_FLAWS, hint = "fix the template where the flaw is")]
    TemplateFlaws {
        /// The flaws, as flare-render describes them.
        flaws: String,
    },
    /// The page file's ending names no format.
    #[error("{path} does not end in .html or .md")]
    #[failure(
        code = PAGES_UNKNOWN_FORMAT,
        hint = "name the page file with .html for HTML or .md for Markdown",
        field(path = path)
    )]
    UnknownFormat {
        /// The file.
        path: String,
    },
    /// The page's name, text or slots are not valid.
    #[error("the page is not valid: {problem}")]
    #[failure(code = PAGES_INVALID, hint = "fix the page as the problem says")]
    PageInvalid {
        /// What is wrong.
        problem: String,
    },
    /// A page or slots file could not be read.
    #[error("{path} could not be read: {problem}")]
    #[failure(code = PAGES_UNREADABLE, hint = "pass a readable file", field(path = path))]
    PageUnreadable {
        /// The file.
        path: String,
        /// What went wrong.
        problem: String,
    },
    /// The records do not fit the page.
    #[error("the records do not fit the page: {problem}")]
    #[failure(
        code = BINDINGS_MISFIT,
        hint = "fill only the slots the page declares, each with a record of the slot's type"
    )]
    Misfit {
        /// What does not fit.
        problem: String,
    },
    /// A binding with the name exists.
    #[error("a binding named {name} exists")]
    #[failure(
        code = BINDINGS_NAME_TAKEN,
        hint = "choose another name with --name",
        field(name = name)
    )]
    NameTaken {
        /// The name.
        name: String,
    },
    /// A record that the binding names does not exist.
    #[error("the record {path} does not exist")]
    #[failure(
        code = BINDINGS_RECORD_NOT_FOUND,
        hint = "name a record that exists in the board",
        field(path = path)
    )]
    RecordNotFound {
        /// The record's path.
        path: String,
    },
    /// The binding's name or fills are not valid.
    #[error("the binding is not valid: {problem}")]
    #[failure(code = BINDINGS_INVALID, hint = "fix the binding as the problem says")]
    BindingInvalid {
        /// What is wrong.
        problem: String,
    },
    /// The program that opens the browser failed.
    #[error(
        "the program that opens the browser failed: {}{problem}",
        before(.program.as_deref(), " ")
    )]
    #[failure(
        code = OPENER_FAILED,
        recovery = Recovery::Retry,
        field(program = program.as_deref())
    )]
    OpenerFailed {
        /// The opener's path, when flare started one.
        program: Option<String>,
        /// What went wrong.
        problem: String,
    },
    /// A file or directory in the state directory cannot be used.
    #[error("{problem}")]
    #[failure(
        code = STATE_UNUSABLE,
        hint = "fix the state file or directory that the problem names",
        alternative = Recovery::hint("remove the state file or directory that the problem names"),
        field(path = path.as_deref())
    )]
    StateUnusable {
        /// The file or directory, when flare holds it.
        path: Option<String>,
        /// The whole message, which names the path where flare prints it.
        problem: String,
    },
    /// The share API's table or bucket is missing or cannot be read.
    #[error("the share API's stores cannot be used: {problem}")]
    #[failure(
        code = STORES_UNAVAILABLE,
        hint = "create the table and the bucket from infra/flare-stores.template.json, and give flare serve credentials that may use them"
    )]
    StoresUnavailable {
        /// What the check found, as flare-aws says it.
        problem: String,
    },
    /// The share API's table names people by another kind than
    /// `hosted.identity`, so `flare serve` wrote nothing and binds nothing.
    #[error(
        "the share API's table names people by {recorded}, and hosted.identity names them by {configured}"
    )]
    #[failure(
        code = CONFIG_IDENTITY_CHANGED,
        hint = "set hosted.identity back to the kind the table records",
        alternative = Recovery::hint("point hosted.stores at a new table for the new kind"),
        field(recorded = recorded)
    )]
    IdentityChanged {
        /// The kind the table records, `email` or `username`.
        recorded: String,
        /// The kind `hosted.identity` names.
        configured: String,
    },
    /// The teams document that `hosted.teams` names could not be read or
    /// was refused, so `flare serve` binds nothing.
    #[error("{problem}")]
    #[failure(
        code = TEAMS_DOCUMENT_REFUSED,
        hint = "fix the teams document that hosted.teams names, or the sync job that writes it"
    )]
    TeamsDocumentRefused {
        /// Why, naming hosted.teams and never a member.
        problem: String,
    },
    /// A log that `flare metrics` was given could not be read.
    #[error("{path} could not be read: {problem}")]
    #[failure(
        code = METRICS_UNREADABLE,
        hint = "pass a readable log file",
        alternative = Recovery::hint("pipe the log to flare metrics on standard input"),
        field(path = path)
    )]
    MetricsUnreadable {
        /// The file, or `standard input`.
        path: String,
        /// What went wrong.
        problem: String,
    },
    /// The operating system refused a socket, a thread or random bytes.
    #[error("the operating system refused: {problem}")]
    #[failure(code = SYSTEM_UNAVAILABLE, recovery = Recovery::Retry)]
    System {
        /// What it refused.
        problem: String,
    },
    /// A sharing command failed before or after the server answered.
    #[error(transparent)]
    #[failure(forward)]
    Sharing(#[from] SharingFailed),
    /// The share API refused the request.
    #[error(transparent)]
    #[failure(forward)]
    Refused(#[from] Refused),
    /// A failure of `flare login`'s token exchange.
    #[error(transparent)]
    #[failure(forward)]
    SignIn(SignInAgain),
}

/// `path` followed by `separator`, or nothing without a path, so a message
/// that names a path reads as it did when the path was in its problem.
fn before(path: Option<&str>, separator: &str) -> String {
    path.map(|path| format!("{path}{separator}"))
        .unwrap_or_default()
}
