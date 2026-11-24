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

`flare serve` serves a board's pages, and `flare share` shares a Markdown
file with named people.
[crates/flare-cli/docs/commands.md](crates/flare-cli/docs/commands.md) lists
every command with its arguments and examples, as `flare <command> --help`
does. Every failure prints a code, such as `flare::shares::not_a_creator`, and
the next step. The board program comes from `--board-program`, else
`FLARE_BOARD`, else the first `board` on `PATH`.

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
[docs/tokens.md](docs/tokens.md) the life of a CLI token.

`"log": { "metrics": true }` makes `flare serve` write a metric line on
standard error for each request, share, page view, sign-in and refusal.
It is off without the key. [docs/metrics.md](docs/metrics.md) lists the
metrics and how to read them.

### The slots file

`flare publish --slots` reads a JSON list:

```json
[ { "name": "task", "type": "tasks/task/v2", "editable_fields": ["title", "status"],
    "sample": { "type": "tasks/task/v2", "body": { "title": "Example" } } } ]
```

See [AGENTS.md](AGENTS.md) for how to work on the repository. flare is
licensed under MIT or Apache-2.0, at your option.
