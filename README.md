# flare

Serve agent-written pages from a board and work on board records through them.

flare is under construction. The `flare` command serves a board on the Mac
that runs it, to the person who runs it, and to Google accounts that its
config admits. With a `hosted` section in its config, it serves a board's
pages on a host behind a TLS proxy to the Google accounts that the config
admits; [docs/hosting.md](docs/hosting.md) says how to run it.

| Crate | Holds |
| --- | --- |
| `flare-core` | The validated types, the `Authenticator`, `PageStore` and `RecordStore` traits, in-memory stores, and the contract suites that every implementation of a trait runs. |
| `flare-auth-oidc` | An `Authenticator` that signs people in with an OpenID Connect provider such as Google, with PKCE, a checked ID token and an allow-list of emails. |
| `flare-board` | `BoardStore`, the `PageStore` and `RecordStore` that keep pages, bindings and record edits in a board by running the board program, which must be a build of crumbkit/board e4ee3333 or later. |
| `flare-auth-loopback` | An `Authenticator` for the person who runs a local server: a bearer token in a private server file, one-use launch codes, a session cookie on a per-launch `h-<32 hex>.localhost` name, and the Host and Origin checks for every request. |
| `flare-render` | Pure functions that check and render pages, draw the edit form of a filled slot, and turn its post into a record write or a conflict view. |
| `flare-server` | The HTTP server: the route table, the reply headers, the form tokens, the OpenID Connect hand-over, the documents around flare-render's output, and the publish and bind operations. |
| `flare-cli` | The `flare` command. |

## The flare command

| Command | What it does |
| --- | --- |
| `flare serve [--board <dir>] [--port <n>] [--board-program <path>] [--config <path>] [--idle-exit <duration>]` | Checks the board program, the config and the Keychain secret, binds `127.0.0.1`, writes the server file and serves until Ctrl-C (exit 130), SIGTERM or 8 hours without a request (exit 0). It removes the server file when it stops. With a `hosted` section in the config, it reads the client secret from `oidc.client_secret_file`, binds `hosted.listen`, writes no server file and serves until Ctrl-C or SIGTERM. |
| `flare open [<page-or-binding>] [--board <dir>]` | Proves that the running server holds its token, then asks it for a one-use launch code and opens it in the browser. Without a name it opens the index at `/flare/`. |
| `flare publish <file> --name <name> [--slots <file.json>] [--dry-run]` | Publishes a new version of a page from a `.html` or `.md` file. A template with flaws is refused before anything is written. |
| `flare bind <page> --name <name> <slot>=<record path>... [--dry-run]` | Saves a binding that pins the page's newest version and fills its slots with records. |

The board program comes from `--board-program`, else `FLARE_BOARD`, else the
first `board` on `PATH`. Every failure prints a code, such as
`flare::board::too_old`, and the next step. `flare commands list` lists
every command and `flare self usage` reports how the command is used.

### The config file

`flare serve` reads `--config <path>`, else
`$XDG_CONFIG_HOME/flare/config.json`, else `~/.config/flare/config.json`.
Without a file, only the person who runs the server signs in. Every object
refuses keys it does not name. The client secret is never in the file: it is
read from the file that `oidc.client_secret_file` names, else from the
Keychain item with the service `flare-oidc-google`, and a `client_secret`
key is refused.

```json
{
  "oidc": {
    "issuer": "https://accounts.google.com",
    "client_id": "<from the Google console>",
    "admit": { "emails": [ { "email": "a@example.com", "principal": "google:a", "reach": "everything" } ] }
  }
}
```

`admit` is `{ "emails": [...] }` or `{ "domain": "example.com", "reach": "everything" }`.
`reach` is `"everything"` or a list of `{ "page": "<name>" }`,
`{ "binding": "<name>" }` and `{ "record": "<path>" }` entries. The redirect
URI to register with the provider is `http://localhost:8740/auth/oidc/callback`.

A hosted server's config adds a `hosted` section and names the secret file.
The redirect URI to register is then `<origin>/auth/oidc/callback`:

```json
{
  "hosted": { "origin": "https://flare.example.com", "listen": "127.0.0.1:8080" },
  "oidc": {
    "issuer": "https://accounts.google.com",
    "client_id": "<from the Google console>",
    "client_secret_file": "/run/secrets/flare-client-secret",
    "admit": { "domain": "example.com", "reach": "everything" }
  }
}
```

### The slots file

`flare publish --slots` reads a JSON list:

```json
[ { "name": "task", "type": "tasks/task/v2", "editable_fields": ["title", "status"],
    "sample": { "type": "tasks/task/v2", "body": { "title": "Example" } } } ]
```

See [AGENTS.md](AGENTS.md) for how to work on the repository. flare is
licensed under MIT or Apache-2.0, at your option.
