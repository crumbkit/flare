//! Every failure of the `hangar` command, with its stable code and the next
//! step it names (CLI-3). No message holds a token, a launch host, a launch
//! URL or a client secret.

use crumbkit::{Call, Code, Failure, Recovery};

/// No server answers for the board.
pub const SERVER_NOT_RUNNING: Code<'static> = Code::new("hangar", "server", "not_running");
/// A server already serves the board.
pub const SERVER_ALREADY_RUNNING: Code<'static> = Code::new("hangar", "server", "already_running");
/// Another program holds the port.
pub const SERVER_PORT_IN_USE: Code<'static> = Code::new("hangar", "server", "port_in_use");
/// The board's directory does not exist.
pub const BOARD_NOT_FOUND: Code<'static> = Code::new("hangar", "board", "not_found");
/// The directory holds no board.
pub const BOARD_NOT_A_BOARD: Code<'static> = Code::new("hangar", "board", "not_a_board");
/// The board program has no `log --at`.
pub const BOARD_TOO_OLD: Code<'static> = Code::new("hangar", "board", "too_old");
/// The board program was not found or did not answer.
pub const BOARD_PROGRAM_UNAVAILABLE: Code<'static> =
    Code::new("hangar", "board", "program_unavailable");
/// The board could not answer a read or a write.
pub const BOARD_UNAVAILABLE: Code<'static> = Code::new("hangar", "board", "unavailable");
/// The config file could not be read.
pub const CONFIG_UNREADABLE: Code<'static> = Code::new("hangar", "config", "unreadable");
/// The config file is not a hangar config.
pub const CONFIG_INVALID: Code<'static> = Code::new("hangar", "config", "invalid");
/// The client secret could not be read from the Keychain.
pub const OIDC_SECRET_UNAVAILABLE: Code<'static> =
    Code::new("hangar", "oidc", "secret_unavailable");
/// The OpenID Connect provider could not be reached or is misconfigured.
pub const OIDC_PROVIDER_UNAVAILABLE: Code<'static> =
    Code::new("hangar", "oidc", "provider_unavailable");
/// No page or binding has the name.
pub const PAGES_NOT_FOUND: Code<'static> = Code::new("hangar", "pages", "not_found");
/// The page's template has flaws.
pub const PAGES_TEMPLATE_FLAWS: Code<'static> = Code::new("hangar", "pages", "template_flaws");
/// The page file is neither HTML nor Markdown.
pub const PAGES_UNKNOWN_FORMAT: Code<'static> = Code::new("hangar", "pages", "unknown_format");
/// The page's name, text or slots are not valid.
pub const PAGES_INVALID: Code<'static> = Code::new("hangar", "pages", "invalid");
/// The page file or the slots file could not be read.
pub const PAGES_UNREADABLE: Code<'static> = Code::new("hangar", "pages", "unreadable");
/// The binding's records do not fit the page.
pub const BINDINGS_MISFIT: Code<'static> = Code::new("hangar", "bindings", "misfit");
/// A binding with the name exists.
pub const BINDINGS_NAME_TAKEN: Code<'static> = Code::new("hangar", "bindings", "name_taken");
/// A record the binding names does not exist.
pub const BINDINGS_RECORD_NOT_FOUND: Code<'static> =
    Code::new("hangar", "bindings", "record_not_found");
/// The binding's name or fills are not valid.
pub const BINDINGS_INVALID: Code<'static> = Code::new("hangar", "bindings", "invalid");
/// The program that opens the browser failed.
pub const OPENER_FAILED: Code<'static> = Code::new("hangar", "opener", "failed");
/// hangar's state directory cannot be used.
pub const STATE_UNUSABLE: Code<'static> = Code::new("hangar", "state", "unusable");
/// The operating system refused a socket, a thread or random bytes.
pub const SYSTEM_UNAVAILABLE: Code<'static> = Code::new("hangar", "system", "unavailable");

/// Why a `hangar` command failed.
#[derive(Debug, thiserror::Error, Failure)]
pub enum Failed {
    /// No server answers for the board.
    #[error("no hangar server answers for this board: {reason}")]
    #[failure(code = SERVER_NOT_RUNNING, recovery = Recovery::Run(Call::new(&["serve"])))]
    NotRunning {
        /// What `open` found.
        reason: String,
    },
    /// A server already serves the board.
    #[error("a hangar server already serves this board")]
    #[failure(code = SERVER_ALREADY_RUNNING, recovery = Recovery::Run(Call::new(&["open"])))]
    AlreadyRunning,
    /// Another program holds the port.
    #[error("port {port} is in use by another program")]
    #[failure(
        code = SERVER_PORT_IN_USE,
        recovery = Recovery::Run(Call::new(&["serve"])),
        field(port = *port)
    )]
    PortInUse {
        /// The port.
        port: u16,
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
        "the board program at {program} has no `log --at`; hangar needs a build of crumbkit/board e4ee3333 or later"
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
    #[error("the board program {problem}")]
    #[failure(
        code = BOARD_PROGRAM_UNAVAILABLE,
        hint = "pass --board-program with the absolute path of a board program",
        alternative = Recovery::hint("set HANGAR_BOARD to the absolute path of a board program")
    )]
    ProgramUnavailable {
        /// What went wrong.
        problem: String,
    },
    /// The board could not answer.
    #[error(
        "the board could not answer: {problem}. A first publish that was cut short leaves the page unreadable until hangar-board recovers it"
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
    /// The config file is not a hangar config.
    #[error("the config file {path} is not a hangar config: {problem}")]
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
        "the client secret could not be read from the Keychain service hangar-oidc-google: {problem}"
    )]
    #[failure(
        code = OIDC_SECRET_UNAVAILABLE,
        hint = "add the client secret to the Keychain as a generic password with the service hangar-oidc-google"
    )]
    SecretUnavailable {
        /// What went wrong.
        problem: String,
    },
    /// The provider could not be reached or is misconfigured.
    #[error("the OpenID Connect provider is unavailable: {problem}")]
    #[failure(code = OIDC_PROVIDER_UNAVAILABLE, recovery = Recovery::Retry)]
    ProviderUnavailable {
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
        /// The flaws, as hangar-render describes them.
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
    #[error("the program that opens the browser failed: {problem}")]
    #[failure(code = OPENER_FAILED, recovery = Recovery::Retry)]
    OpenerFailed {
        /// What went wrong.
        problem: String,
    },
    /// The state directory cannot be used.
    #[error("{problem}")]
    #[failure(code = STATE_UNUSABLE, hint = "fix the state directory as the problem says")]
    StateUnusable {
        /// What is wrong, as hangar-auth-loopback says it.
        problem: String,
    },
    /// The operating system refused a socket, a thread or random bytes.
    #[error("the operating system refused: {problem}")]
    #[failure(code = SYSTEM_UNAVAILABLE, recovery = Recovery::Retry)]
    System {
        /// What it refused.
        problem: String,
    },
}

/// Every code that `hangar` declares.
pub const CODES: &[Code<'static>] = Failed::CODES;
