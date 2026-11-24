//! A failure of `flare login`'s token exchange. Running `flare login` again
//! starts a new browser sign-in, which only a person can finish, so every
//! step that would run the login again hands it to a person in a terminal.

use super::Failed;
use crumbkit::{Call, Code, Failure, Field, Recovery, Then};
use std::fmt;

/// A failure of `flare login`'s token exchange with `server`. It keeps the
/// failure's code, message and fields, and its recovery hands a retry to a
/// person as a `terminal` `flare login <server>`.
#[derive(Debug)]
pub struct SignInAgain {
    failed: Box<Failed>,
    server: String,
}

impl Failed {
    /// The same failure from `flare login`'s token exchange with `server`.
    pub(crate) fn in_a_sign_in(self, server: &str) -> Self {
        Self::SignIn(SignInAgain {
            failed: Box::new(self),
            server: server.to_owned(),
        })
    }
}

impl SignInAgain {
    /// `recovery` with each retry handed to a person.
    fn handed_over(&self, recovery: Recovery) -> Recovery {
        match recovery {
            Recovery::Retry => Recovery::Terminal(login(&self.server)),
            Recovery::After {
                repair,
                then: Then::Retry,
            } => Recovery::after(repair, Then::Terminal(login(&self.server))),
            other => other,
        }
    }
}

impl fmt::Display for SignInAgain {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.failed, formatter)
    }
}

impl std::error::Error for SignInAgain {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.failed.source()
    }
}

impl Failure for SignInAgain {
    fn code(&self) -> Code<'_> {
        self.failed.code()
    }

    fn causes(&self) -> Vec<String> {
        self.failed.causes()
    }

    fn recovery(&self) -> Recovery {
        self.handed_over(self.failed.recovery())
    }

    fn alternatives(&self) -> Vec<Recovery> {
        self.failed
            .alternatives()
            .into_iter()
            .map(|recovery| self.handed_over(recovery))
            .collect()
    }

    fn fields(&self) -> Vec<Field<'_>> {
        self.failed.fields()
    }

    fn url(&self) -> Option<&str> {
        self.failed.url()
    }
}

/// `flare login` with `server`.
fn login(server: &str) -> Call {
    Call::new(&["login"]).value("server", server.to_owned())
}

/// The recovery of a failure that needs a new sign-in. Only a person can
/// finish the browser sign-in, so with a known server it is a `terminal`
/// recovery for `flare login <server>`. crumbkit renders a terminal call
/// only when every argument has a value, and with no saved login flare does
/// not know the server, so in that case the recovery is a hint that names
/// the command and the server's address the person supplies.
pub(super) fn sign_in(server: Option<&str>) -> Recovery {
    match server {
        Some(server) => Recovery::Terminal(login(server)),
        None => Recovery::hint(
            "a person runs flare login with their flare server's address, such as https://flare.example.org, in a terminal",
        ),
    }
}

#[cfg(test)]
mod tests;
