# hangar

Serve agent-written pages from a board and work on board records through them.

hangar is under construction.

| Crate | Holds |
| --- | --- |
| `hangar-core` | The validated types, the `Authenticator`, `PageStore` and `RecordStore` traits, in-memory stores, and the contract suites that every implementation of a trait runs. |
| `hangar-auth-oidc` | An `Authenticator` that signs people in with an OpenID Connect provider such as Google, with PKCE, a checked ID token and an allow-list of emails. |

See [AGENTS.md](AGENTS.md) for how to work on the repository. hangar is
licensed under MIT or Apache-2.0, at your option.
