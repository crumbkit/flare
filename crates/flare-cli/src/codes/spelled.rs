//! The spelling of each code of [`Failed`](super::Failed).

use crumbkit::Code;

/// No server answers for the board.
pub const SERVER_NOT_RUNNING: Code<'static> = Code::from_spelling("flare::server::not_running");
/// A server already serves the board.
pub const SERVER_ALREADY_RUNNING: Code<'static> =
    Code::from_spelling("flare::server::already_running");
/// Another program holds the port.
pub const SERVER_PORT_IN_USE: Code<'static> = Code::from_spelling("flare::server::port_in_use");
/// The board's directory does not exist.
pub const BOARD_NOT_FOUND: Code<'static> = Code::from_spelling("flare::board::not_found");
/// The directory holds no board.
pub const BOARD_NOT_A_BOARD: Code<'static> = Code::from_spelling("flare::board::not_a_board");
/// The board program has no `log --at`.
pub const BOARD_TOO_OLD: Code<'static> = Code::from_spelling("flare::board::too_old");
/// The board program was not found or did not answer.
pub const BOARD_PROGRAM_UNAVAILABLE: Code<'static> =
    Code::from_spelling("flare::board::program_unavailable");
/// The board could not answer a read or a write.
pub const BOARD_UNAVAILABLE: Code<'static> = Code::from_spelling("flare::board::unavailable");
/// The config file could not be read.
pub const CONFIG_UNREADABLE: Code<'static> = Code::from_spelling("flare::config::unreadable");
/// The config file is not a flare config.
pub const CONFIG_INVALID: Code<'static> = Code::from_spelling("flare::config::invalid");
/// The share API's table records another identifier kind than the config.
pub const CONFIG_IDENTITY_CHANGED: Code<'static> =
    Code::from_spelling("flare::config::identity_changed");
/// The share API's table holds team grants made under other teams than
/// the config names.
pub const CONFIG_TEAMS_CHANGED: Code<'static> = Code::from_spelling("flare::config::teams_changed");
/// The client secret could not be read from the Keychain or its file.
pub const OIDC_SECRET_UNAVAILABLE: Code<'static> =
    Code::from_spelling("flare::oidc::secret_unavailable");
/// The OpenID Connect provider could not be reached or is misconfigured.
pub const OIDC_PROVIDER_UNAVAILABLE: Code<'static> =
    Code::from_spelling("flare::oidc::provider_unavailable");
/// The single sign-on proxy's key set could not be fetched or holds no
/// usable key.
pub const PROXY_KEYS_UNAVAILABLE: Code<'static> =
    Code::from_spelling("flare::proxy::keys_unavailable");
/// No page or binding has the name.
pub const PAGES_NOT_FOUND: Code<'static> = Code::from_spelling("flare::pages::not_found");
/// The page's template has flaws.
pub const PAGES_TEMPLATE_FLAWS: Code<'static> = Code::from_spelling("flare::pages::template_flaws");
/// The page file is neither HTML nor Markdown.
pub const PAGES_UNKNOWN_FORMAT: Code<'static> = Code::from_spelling("flare::pages::unknown_format");
/// The page's name, text or slots are not valid.
pub const PAGES_INVALID: Code<'static> = Code::from_spelling("flare::pages::invalid");
/// The page file or the slots file could not be read.
pub const PAGES_UNREADABLE: Code<'static> = Code::from_spelling("flare::pages::unreadable");
/// The binding's records do not fit the page.
pub const BINDINGS_MISFIT: Code<'static> = Code::from_spelling("flare::bindings::misfit");
/// A binding with the name exists.
pub const BINDINGS_NAME_TAKEN: Code<'static> = Code::from_spelling("flare::bindings::name_taken");
/// A record the binding names does not exist.
pub const BINDINGS_RECORD_NOT_FOUND: Code<'static> =
    Code::from_spelling("flare::bindings::record_not_found");
/// The binding's name or fills are not valid.
pub const BINDINGS_INVALID: Code<'static> = Code::from_spelling("flare::bindings::invalid");
/// The program that opens the browser failed.
pub const OPENER_FAILED: Code<'static> = Code::from_spelling("flare::opener::failed");
/// A file or directory in flare's state directory cannot be used.
pub const STATE_UNUSABLE: Code<'static> = Code::from_spelling("flare::state::unusable");
/// The share API's table or bucket is missing or cannot be read.
pub const STORES_UNAVAILABLE: Code<'static> = Code::from_spelling("flare::stores::unavailable");
/// The teams document that `hosted.teams` names could not be read or was
/// refused when `flare serve` started.
pub const TEAMS_DOCUMENT_REFUSED: Code<'static> =
    Code::from_spelling("flare::teams::document_refused");
/// A log that `flare metrics` was given could not be read.
pub const METRICS_UNREADABLE: Code<'static> = Code::from_spelling("flare::metrics::unreadable");
/// The operating system refused a socket, a thread or random bytes.
pub const SYSTEM_UNAVAILABLE: Code<'static> = Code::from_spelling("flare::system::unavailable");
