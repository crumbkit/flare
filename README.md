# hangar

Serve agent-written pages from a board and work on board records through them.

hangar is under construction.

| Crate | Holds |
| --- | --- |
| `hangar-core` | The validated types, the `Authenticator`, `PageStore` and `RecordStore` traits, in-memory stores, and the contract suites that every implementation of a trait runs. |
| `hangar-auth-oidc` | An `Authenticator` that signs people in with an OpenID Connect provider such as Google, with PKCE, a checked ID token and an allow-list of emails. |
| `hangar-board` | `BoardStore`, the `PageStore` and `RecordStore` that keep pages, bindings and record edits in a board by running the board program, which must be a build of crumbkit/board e4ee3333 or later. |
| `hangar-auth-loopback` | An `Authenticator` for the person who runs a local server: a bearer token in a private server file, one-use launch codes, a session cookie on a per-launch `h-<32 hex>.localhost` name, and the Host and Origin checks for every request. |

See [AGENTS.md](AGENTS.md) for how to work on the repository. hangar is
licensed under MIT or Apache-2.0, at your option.
