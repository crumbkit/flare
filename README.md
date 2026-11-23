# flare

Serve agent-written pages from a board and work on board records through them.

flare is under construction. The `flare` command serves a board on the Mac
that runs it, to the person who runs it and to the Google accounts that its
config admits. With a `hosted` section in its config, it serves a board's
pages behind a TLS proxy, and with `hosted.stores` it serves the share API,
through which `flare share` shares files with named people.

| Crate | Holds |
| --- | --- |
| `flare-core` | The validated types, the traits, the services, in-memory stores, and the contract suites that every implementation of a trait runs. |
| `flare-contract` | The share API's request, response and problem types, and their JSON Schema in `schema/`. |
| `flare-auth-oidc` | An `Authenticator` that signs people in with an OpenID Connect provider such as Google. |
| `flare-auth-loopback` | An `Authenticator` for the person who runs a local server, with launch codes and per-launch host names. |
| `flare-board` | `BoardStore`, which keeps pages, bindings and record edits in a board through the board program (crumbkit/board e4ee3333 or later). |
| `flare-aws` | The share, grant, file, token and response stores in DynamoDB and S3. |
| `flare-render` | Pure functions that check and render pages and their edit forms. |
| `flare-server` | The HTTP server: the routes, the sign-in hand-over, the share API and the publish and bind operations. |
| `flare-cli` | The `flare` command. |

## The flare command

| Command | What it does |
| --- | --- |
| `flare serve` | Serves the board until Ctrl-C, SIGTERM or 8 idle hours. |
| `flare open [<name>]` | Opens the running server's index, a page or a binding. |
| `flare publish <file> --name <name>` | Publishes a page from a `.html` or `.md` file. |
| `flare bind <page> --name <name> <slot>=<path>...` | Saves a binding that fills a page's slots with records. |
| `flare login <server>` | Signs in to a hosted flare and keeps its token in the Keychain. |
| `flare share <file> --with <addresses> --re <flare>` | Shares a Markdown file with named people, as a response to flares. |
| `flare grant <id> --with <addresses>` | Grants a share to more people. |
| `flare shares [--with-me]` | Lists your shares, or those granted to you. |
| `flare activity <flare>... [--since <cursor>]` | Lists the responses to flares, oldest first. |
| `flare logout --yes` | Revokes the token and deletes it. |

[crates/flare-cli/docs/commands.md](crates/flare-cli/docs/commands.md) lists
every argument. Every failure prints a code, such as
`flare::shares::not_a_creator`, and the next step. The board program comes
from `--board-program`, else `FLARE_BOARD`, else the first `board` on `PATH`.

### The config file

`flare serve` reads `--config <path>`, else
`$XDG_CONFIG_HOME/flare/config.json`, else `~/.config/flare/config.json`.
Without a file, only the person who runs the server signs in. Every object
refuses unknown keys, including `client_secret`: the secret comes from the
file `oidc.client_secret_file` names, else the Keychain item
`flare-oidc-google`.

```json
{
  "oidc": {
    "issuer": "https://accounts.google.com",
    "client_id": "<from the Google console>",
    "admit": { "emails": [ { "email": "a@example.com", "principal": "google:a", "reach": "everything" } ] }
  }
}
```

`admit` is `{ "emails": [...] }`, `{ "domain": "example.com", "reach": ... }`
or `{ "anyone": true, "reach": ... }`. `reach` is `"everything"` or a list of
`{ "page": "<name>" }`, `{ "binding": "<name>" }` and `{ "record": "<path>" }`
entries. Register `http://localhost:8740/auth/oidc/callback` as the redirect
URI. [docs/hosting.md](docs/hosting.md) gives the hosted config, and
[docs/tokens.md](docs/tokens.md) the life of a CLI token, and
[docs/comments.md](docs/comments.md) the life of a comment and a cursor.

### The slots file

`flare publish --slots` reads a JSON list:

```json
[ { "name": "task", "type": "tasks/task/v2", "editable_fields": ["title", "status"],
    "sample": { "type": "tasks/task/v2", "body": { "title": "Example" } } } ]
```

See [AGENTS.md](AGENTS.md) for how to work on the repository. flare is
licensed under MIT or Apache-2.0, at your option.
