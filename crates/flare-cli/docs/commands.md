# Commands of `flare`

This reference is generated from the help of `flare`. Edit the help in the tool's code, and not this file.

## `flare`

Serve a board's pages and bindings on this machine, open them in a browser, publish pages and save bindings, and share files through a flare server

### Commands

- [`flare serve`](#flare-serve): Serve a board's pages and bindings on this machine, or its pages behind a TLS proxy, until Ctrl-C
- [`flare open`](#flare-open): Open the running server's index, a page or a binding in a browser
- [`flare publish`](#flare-publish): Publish a new version of a page from an HTML or Markdown file
- [`flare bind`](#flare-bind): Save a binding that fills a page's slots with records
- [`flare login`](#flare-login): Sign in to a flare server in the browser and keep its token in the Keychain
- [`flare logout`](#flare-logout): Revoke this computer's token for the flare server and delete it
- [`flare share`](#flare-share): Share a Markdown file with named people, or as a response to flares
- [`flare grant`](#flare-grant): Grant an existing share to more people
- [`flare shares`](#flare-shares): List your shares or the shares granted to you
- [`flare activity`](#flare-activity): List the responses to flares since a cursor, oldest first
- [`flare commands`](#flare-commands): Explore flare's commands
- [`flare self`](#flare-self): Review flare's own use

### Arguments

- `--json`: Print the result as a JSON document. The default prints text. Works after every subcommand.
- `--fields <FIELD>`: Print only these comma-separated fields, in this order. The default prints the compact columns. Takes more than one value. Works after every subcommand.
- `--long`: Add the long columns after the compact ones. The default prints only the compact columns. Works after every subcommand.
- `--full`: With --json, print every field of every row. The default prints only the fields of the columns. Works after every subcommand.
- `--no-input`: Fail instead of asking a question. Works after every subcommand.

## `flare serve`

Serve a board's pages and bindings on this machine, or its pages behind a TLS proxy, until Ctrl-C

### Arguments

- `--board <DIR>`: The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
- `--board-program <PATH>`: The board program; without it, FLARE\_BOARD, then the first board on PATH
- `--port <N>`: The launch host's port on 127.0.0.1; 0 picks a free port. A hosted server listens on hosted.listen instead. Default: `0`.
- `--config <PATH>`: The config file, whose hosted section serves the pages behind a TLS proxy; without it, $XDG\_CONFIG\_HOME/flare/config.json or \~/.config/flare/config.json
- `--idle-exit <DURATION>`: Stop after this long without a request, such as 30m or 8h. A hosted server never stops by itself. Default: `8h`.

### Examples

Serve the board in the current directory.

```sh
flare serve
```

Serve another board and stop after an hour without a request.

```sh
flare serve --board ../notes --idle-exit 1h
```

### Columns

- `board`: The board's directory.
- `stopped`: Why the server stopped: `shutdown` after SIGTERM, `interrupted` after Ctrl-C or `idle` after the idle period.

Effects: additive, not idempotent, closed-world.

### Exit codes

- `130`: Ctrl-C stopped the server.

## `flare open`

Open the running server's index, a page or a binding in a browser

### Arguments

- `<NAME>`: The page or binding to open; without it, the index
- `--board <DIR>`: The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json

### Examples

Open the index of the running server.

```sh
flare open
```

Open the binding or page named kitchen.

```sh
flare open kitchen
```

### Columns

- `target`: The path the browser opens on the server, such as `/b/kitchen`.

Effects: additive, not idempotent, closed-world.

## `flare publish`

Publish a new version of a page from an HTML or Markdown file

### Arguments

- `<FILE>`: The page's file: .html for HTML, .md for Markdown. Required.
- `--name <NAME>`: The page's name, one segment of lowercase letters, digits and dashes. Required.
- `--slots <FILE>`: A JSON file that lists the page's slots
- `--board <DIR>`: The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
- `--board-program <PATH>`: The board program; without it, FLARE\_BOARD, then the first board on PATH
- `--dry-run`: Report what the command would change, and change nothing

### Examples

Publish card.html as the page task-card with the slots in slots.json.

```sh
flare publish card.html --name task-card --slots slots.json
```

Check card.md and publish nothing.

```sh
flare publish card.md --name task-card --dry-run
```

### Columns

- `name`: The page's name.
- `version`: The new version, or none on a dry run.
- `format`: The page's markup language: `html` or `markdown`.
- `slots`: The names of the page's slots.

Effects: additive, not idempotent, closed-world.

## `flare bind`

Save a binding that fills a page's slots with records

### Arguments

- `<PAGE>`: The page the binding fills. Required.
- `--name <NAME>`: The binding's name, one segment of lowercase letters, digits and dashes. Required.
- `<SLOT=PATH>`: Each slot and the record that fills it, as SLOT=PATH, separated by spaces. Required. Takes more than one value.
- `--board <DIR>`: The board's directory; without it, the nearest directory at or above the current one that holds .board/board.json
- `--board-program <PATH>`: The board program; without it, FLARE\_BOARD, then the first board on PATH
- `--dry-run`: Report what the command would change, and change nothing

### Examples

Save the binding kitchen, which fills task-card's task slot with /tasks/tap.

```sh
flare bind task-card --name kitchen task=/tasks/tap
```

### Columns

- `name`: The binding's name.
- `page`: The page the binding fills.
- `version`: The page version the binding is pinned to.

Effects: additive, not idempotent, closed-world.

Writes: applies all items or none.

## `flare login`

Sign in to a flare server in the browser and keep its token in the Keychain

### Arguments

- `<SERVER>`: The flare server, such as https://flare.example.org. Required.
- `--label <LABEL>`: The token's label; without it, the label of this computer's earlier login to the server, else a new flare-cli-\<hex\>

### Examples

Sign in to a flare server in the browser and keep its token in the Keychain.

```sh
flare login https://flare.example.org
```

### Columns

- `server`: The server's origin.
- `owner`: The address the token signs in as.
- `label`: The token's label.
- `expires`: When the token expires, in RFC 3339 UTC.

Effects: additive, not idempotent, closed-world.

## `flare logout`

Revoke this computer's token for the flare server and delete it

### Arguments

- `--yes`: Proceed without asking for confirmation

### Examples

Revoke this computer's token and delete it from the Keychain.

```sh
flare logout
```

### Columns

- `server`: The server's origin.
- `label`: The label of the revoked token.

Effects: destructive, not idempotent, closed-world.

## `flare share`

Share a Markdown file with named people, or as a response to flares

### Arguments

- `<FILE>`: The file to share: .md for Markdown. Required.
- `--title <TITLE>`: The share's title; without it, the file's name without its ending
- `--with <ADDRESS>`: The addresses to grant Reader, repeated or separated by commas. Takes more than one value.
- `--re <ID>`: The flares this share responds to, repeated or separated by commas. Takes more than one value.

### Examples

Share review.md with two people.

```sh
flare share review.md --with ana@example.org,jo@example.org
```

Share answer.md as a response to a flare.

```sh
flare share answer.md --re 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6
```

### Columns

- `id`: The share's id.
- `title`: The share's title.
- `granted`: The addresses that now hold a role on it.
- `responds_to`: The flares it responds to.

Effects: additive, not idempotent, closed-world.

## `flare grant`

Grant an existing share to more people

### Arguments

- `<ID>`: The share to grant. Required.
- `--with <ADDRESS>`: The addresses to grant Reader, repeated or separated by commas. Takes more than one value.
- `--re <ID>`: The flares the share responds to, repeated or separated by commas; only its owner may add them. Takes more than one value.

### Examples

Grant a share to one more person.

```sh
flare grant 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --with mei@example.org
```

### Columns

- `id`: The share's id.
- `title`: The share's title.
- `granted`: The addresses that now hold a role on it.
- `responds_to`: The flares it responds to.

Effects: additive, not idempotent, closed-world.

## `flare shares`

List your shares or the shares granted to you

The server lists at most 500 shares in one answer and has no cursor for the rest, so --all lists at most 500 shares and says more follow when there are more.

### Arguments

- `--with-me`: List the shares granted to you in place of your own
- `--limit <N>`: Print at most this many rows, up to 500. Default: `50`.
- `--all`: Print every row

### Examples

List your shares, newest first.

```sh
flare shares
```

List the 20 newest shares granted to you.

```sh
flare shares --with-me --limit 20
```

### Columns

- `id`: The share's id.
- `title`: The share's title.
- `owner`: The owner's identifier.
- `role`: The caller's role: `owner`, `admin` or `reader`.
- `version`: The latest version.
- `created`: When it was created, in RFC 3339 UTC.

Effects: read-only, closed-world.

## `flare activity`

List the responses to flares since a cursor, oldest first

### Arguments

- `<FLARE>`: The flares whose responses to list, at most 16. Required. Takes more than one value.
- `--since <CURSOR>`: List only responses after this cursor, which an earlier call printed
- `--limit <N>`: Print at most this many rows, up to 100. Default: `50`.
- `--all`: Print every row

### Examples

List the responses to a flare, oldest first.

```sh
flare activity 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6
```

List the responses after the cursor an earlier call printed.

```sh
flare activity 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --since fc1.AAAAAAAAAC8
```

### Columns

- `flare`: The flare it responds to.
- `share`: The responding share's id.
- `title`: The responding share's title.
- `by`: The responder's identifier.
- `version`: The version of the flare it responds to.
- `at`: When it named the flare, in RFC 3339 UTC.
- `also`: The other flares it responds to that the caller can open.

Effects: read-only, closed-world.

## `flare commands`

Explore flare's commands

### Commands

- [`flare commands list`](#flare-commands-list): List a branch of flare's commands with their arguments and examples

## `flare commands list`

List a branch of flare's commands with their arguments and examples

### Arguments

- `<PATH>`: The names of the commands below the root that lead to the branch, such as `books list`. Takes more than one value.
- `--limit <N>`: Print at most this many rows. Default: `25`.
- `--all`: Print every row

### Examples

List every command.

```sh
flare commands list
```

List flare serve with its arguments.

```sh
flare commands list serve
```

### Columns

- `path`: The command line that runs the command, such as `shelf books list`.
- `about`: The command's one-line description, or its full description when it has no one-line description.

Effects: read-only, closed-world.

## `flare self`

Review flare's own use

### Commands

- [`flare self usage`](#flare-self-usage): Report the patterns in flare's usage log where callers waste effort

## `flare self usage`

Report the patterns in flare's usage log where callers waste effort

### Arguments

- `--since <DURATION>`: How far back to read the usage log, such as 12h, 7d or 2w. Default: `7d`.
- `--log <FILE>`: Read this usage log instead of the tool's own
- `--exclude-program <NAME>`: Leave out the runs that a program with this name marked, such as flightdeck. Takes more than one value.

### Examples

Report the findings of the last 7 days.

```sh
flare self usage
```

### Columns

- `finding`: What the pattern is.
- `runs`: The runs that show the pattern.
- `command`: The command path below the tool's name, such as `books show`, or the tool's name for its root command.
- `suggestion`: What the pattern suggests, or the figure that raised it.

Effects: read-only, closed-world.
