# Commands of `flare`

This reference is generated from the help of `flare`. Edit the help in the tool's code, and not this file.

## `flare`

Serve a board's pages and bindings on this machine, open them in a browser, publish pages and save bindings, and share files through a flare server

### Commands

- [`flare serve`](#flare-serve): Serve a board's pages and bindings on this machine, or its pages behind a TLS proxy, until Ctrl-C
- [`flare open`](#flare-open): Open the running server's index, a page or a binding in a browser
- [`flare publish`](#flare-publish): Publish a new version of a page from an HTML or Markdown file
- [`flare bind`](#flare-bind): Save a binding that fills a page's slots with records
- [`flare login`](#flare-login): Sign in to a flare server and keep its token in the Keychain
- [`flare logout`](#flare-logout): Revoke this computer's flare token and delete it
- [`flare share`](#flare-share): Share a Markdown file
- [`flare show`](#flare-show): Show one version of a share you can open, or its latest
- [`flare grant`](#flare-grant): Grant a share to readers
- [`flare revoke`](#flare-revoke): End grants on a share
- [`flare grants`](#flare-grants): List grants
- [`flare teams`](#flare-teams): Manage teams
- [`flare shares`](#flare-shares): List your shares or those granted to you
- [`flare activity`](#flare-activity): List the responses to flares, oldest first
- [`flare metrics`](#flare-metrics): Count the metric lines in a flare server's log, by day
- [`flare commands`](#flare-commands): Explore flare's commands
- [`flare schemas`](#flare-schemas): Explore the JSON documents that flare prints
- [`flare skills`](#flare-skills): Manage flare's agent skill
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

Sign in to a flare server and keep its token in the Keychain

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

Revoke this computer's flare token and delete it

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

Share a Markdown file

### Arguments

- `<FILE>`: The file to share: .md for Markdown. Required.
- `--title <TITLE>`: The title; without it, the file name without its ending
- `--with <WHO>`: Readers, comma-separated: addresses, team:\<name\>, everyone. Takes more than one value.
- `--re <ID>`: The flares it responds to, whose owners become Readers; repeated or comma-separated. Takes more than one value.
- `--update <ID>`: Publish the file as your share's next version

  It changes only the content; grants stay with flare grant.
- `--dry-run`: Report what the command would change, and change nothing
- `--out <PATH>`: With --dry-run, write the HTML here

### Examples

Share review.md with two people.

```sh
flare share review.md --with ana@example.org,jo@example.org
```

Share answer.md as a response to a flare.

```sh
flare share answer.md --re 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6
```

Publish review.md as the next version of a share you own.

```sh
flare share review.md --update 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6
```

Check review.md as the server would and write its HTML to review.html, sharing nothing.

```sh
flare share review.md --dry-run --out review.html
```

### Columns

- `id`: The share's id; none on a dry run of a new share.
- `title`: The share's title; none on a dry run of `--update`, which does not read the share.
- `version`: The share's latest version; none on a dry run.
- `granted`: The addresses that now hold a role on it; none after `--update` or a dry run.
- `responds_to`: The flares it responds to; none after `--update` or a dry run.

Effects: additive, not idempotent, closed-world.

Children: A dry run lists its checks.

## `flare show`

Show one version of a share you can open, or its latest

### Arguments

- `<ID>`: The share to show. Required.
- `--version <N>`: The version to show, a number from 1; without it, the latest

### Examples

Show the latest version of a share.

```sh
flare show 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6
```

Show the first version of a share.

```sh
flare show 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --version 1
```

### Columns

- `id`: The share's id.
- `title`: The share's title.
- `owner`: The owner's identifier.
- `role`: The caller's role: `owner`, `admin` or `reader`.
- `version`: The version whose content this holds.
- `latest`: The share's latest version.

Effects: read-only, closed-world.

Children: The version's Markdown follows the share, one line of text for each line of the file.

## `flare grant`

Grant a share to readers

### Arguments

- `<ID>`: The share to grant. Required.
- `--with <WHO>`: Who to grant Reader, repeated or separated by commas: an address, team:\<name\> for a team you belong to, or everyone. Takes more than one value.
- `--everyone`: Grant Reader to everyone signed in whom the server's sign-in rule covers; only the share's owner may
- `--re <ID>`: The flares the share responds to, repeated or separated by commas; only its owner may add them, and the flares' owners become Readers of it. Takes more than one value.

### Examples

Grant a share to one more person.

```sh
flare grant 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --with mei@example.org
```

Grant a share to the members of a team you belong to.

```sh
flare grant 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --with team:design
```

Open a share you own to everyone signed in.

```sh
flare grant 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --everyone
```

### Columns

- `id`: The share's id.
- `title`: The share's title.
- `version`: The share's latest version.
- `granted`: The addresses, `team:<name>` and `everyone` that now hold a role on it; none after `--update`, which does not report them.
- `responds_to`: The flares it responds to; none after `--update`, which does not report them.

Effects: additive, not idempotent, closed-world.

## `flare revoke`

End grants on a share

### Arguments

- `<ID>`: The share whose grants end. Required.
- `--with <WHO>`: Whose grant ends, repeated or separated by commas: an address, team:\<name\> or everyone. Required, or give `--everyone` in its place. Takes more than one value.
- `--everyone`: End the grant to everyone signed in; only the share's owner may. Required, or give `--with` in its place.
- `--yes`: Proceed without asking for confirmation

### Examples

End one person's grant on a share.

```sh
flare revoke 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --with mei@example.org --yes
```

Close a share you own to everyone signed in.

```sh
flare revoke 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6 --everyone --yes
```

### Columns

- `id`: The share's id.
- `revoked`: The addresses, `team:<name>` and `everyone` whose grant ended; one that held no grant is left out.

Effects: destructive, not idempotent, closed-world.

## `flare grants`

List grants

### Arguments

- `<ID>`: The share whose grants to list; you must own it or hold Admin on it. Required.

### Examples

List the people, teams and everyone grants of a share.

```sh
flare grants 3f9c0a17e2b44d6f8a1c5b7e9d20f4a6
```

### Columns

- `grantee`: The person's address, `team:<name>` or `everyone`.
- `kind`: Who the grant is to: `person`, `team` or `everyone`.
- `role`: The grant's role: `admin` or `reader`.
- `state`: `active`, or `dormant` for a grant to a deleted team, which opens nothing until the share's owner revokes it.

### Columns that `--long` adds

- `granted_by`: Who wrote the grant's current role.
- `created`: When the grant was first written, in RFC 3339 UTC.
- `audience`: For an everyone grant, the mail domain it opens to, or `admitted` for the server's email list.

Effects: read-only, closed-world.

## `flare teams`

Manage teams

### Commands

- [`flare teams create`](#flare-teams-create): Create a team that you own, with you as its first member
- [`flare teams add`](#flare-teams-add): Add people to a team you own
- [`flare teams remove`](#flare-teams-remove): Remove people from a team you own, or leave a team you belong to
- [`flare teams list`](#flare-teams-list): List the teams you belong to
- [`flare teams show`](#flare-teams-show): Show the members of teams you belong to
- [`flare teams delete`](#flare-teams-delete): Delete a team you own; its grants stay and open nothing, and its name is never used again

## `flare teams create`

Create a team that you own, with you as its first member

### Arguments

- `<NAME>`: The team's name: 1 to 40 lowercase letters, digits and dashes. Required.

### Examples

Create the team design.

```sh
flare teams create design
```

### Columns

- `name`: The team's name.
- `owner`: The owner's address.
- `role`: What you are to the team: `owner` or `member`.
- `members`: How many members the team has, its owner included.

### Columns that `--long` adds

- `created`: When the team was created, in RFC 3339 UTC.

Effects: additive, not idempotent, closed-world.

## `flare teams add`

Add people to a team you own

### Arguments

- `<NAME>`: The team's name. Required.
- `--with <PERSON>`: The people, repeated or separated by commas: each an address, or a name the server resolves. Required. Takes more than one value.

### Examples

Add two people to the team design.

```sh
flare teams add design --with mei@example.org,jo@example.org
```

### Columns

- `team`: The team's name.
- `members`: How many members the team has now.
- `changed`: The addresses added or removed; one already present, or already absent, is left out.

Effects: additive, not idempotent, closed-world.

## `flare teams remove`

Remove people from a team you own, or leave a team you belong to

### Arguments

- `<NAME>`: The team's name. Required.
- `--with <PERSON>`: The people, repeated or separated by commas: each an address, or a name the server resolves. Required. Takes more than one value.
- `--yes`: Proceed without asking for confirmation

### Examples

Remove one person from the team design.

```sh
flare teams remove design --with jo@example.org
```

### Columns

- `team`: The team's name.
- `members`: How many members the team has now.
- `changed`: The addresses added or removed; one already present, or already absent, is left out.

Effects: destructive, not idempotent, closed-world.

## `flare teams list`

List the teams you belong to

### Arguments

- `--limit <N>`: Print at most this many rows, up to 100. Default: `50`.
- `--all`: Print every row

### Examples

List the teams you belong to.

```sh
flare teams list
```

### Columns

- `name`: The team's name.
- `owner`: The owner's address.
- `role`: What you are to the team: `owner` or `member`.
- `members`: How many members the team has, its owner included.

### Columns that `--long` adds

- `created`: When the team was created, in RFC 3339 UTC.

Effects: read-only, closed-world.

## `flare teams show`

Show the members of teams you belong to

### Arguments

- `<NAME>`: The teams' names, up to 100. Required. Takes more than one value.

### Examples

Show the members of the team design.

```sh
flare teams show design
```

### Columns

- `team`: The team's name.
- `member`: The member's address.
- `role`: `owner` for the team's owner, else `member`.

Effects: read-only, closed-world.

## `flare teams delete`

Delete a team you own; its grants stay and open nothing, and its name is never used again

### Arguments

- `<NAME>`: The team's name. Required.
- `--yes`: Proceed without asking for confirmation

### Examples

Delete the team design without a question.

```sh
flare teams delete design --yes
```

### Columns

- `team`: The deleted team's name.
- `grants_dormant`: Always true: each flare's grant to the team stays and opens nothing until the flare's owner revokes it.
- `name_reusable`: Always false: the name is never used again, so no new team can take over the dormant grants.

Effects: destructive, not idempotent, closed-world.

## `flare shares`

List your shares or those granted to you

The server lists at most 500 shares in one answer. A list cut short names the next call with --cursor, which resumes the list after the last share printed, and --all follows the cursor until the list ends.

### Arguments

- `--with-me`: List the shares granted to you in place of your own
- `--everyone`: With --with-me, also list the shares open to everyone signed in
- `--cursor <CURSOR>`: List the shares after this cursor, which an earlier flare shares printed
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

List the shares granted to you after the cursor an earlier call printed.

```sh
flare shares --with-me --cursor fs1.AQAAAABpJIEAqqqqqqqqqqqqqqqqqqqqqg
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

List the responses to flares, oldest first

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

## `flare metrics`

Count the metric lines in a flare server's log, by day

The counts come from the metric lines that flare serve writes on standard error when its config sets log.metrics to true. A page view is one page response, so one person who reloads a page counts twice. Lines that are not metric lines are skipped and counted.

### Arguments

- `<FILE>`: The log files to read, oldest first; without them, standard input. Takes more than one value.

### Examples

Count the metric lines in flare.log by day.

```sh
flare metrics flare.log
```

Count the metric lines of a rotated log and its newer file as JSON.

```sh
flare metrics flare.log.1 flare.log --json
```

### Columns

- `day`: The day, as `YYYY-MM-DD` in UTC.
- `metric`: The metric's name, such as `PagesViewed`.
- `count`: The number of events the metric counted that day.

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

## `flare schemas`

Explore the JSON documents that flare prints

### Commands

- [`flare schemas list`](#flare-schemas-list): List the JSON documents that flare prints, with a description of each
- [`flare schemas show`](#flare-schemas-show): Print the JSON Schema of each document

## `flare schemas list`

List the JSON documents that flare prints, with a description of each

### Arguments

- `--limit <N>`: Print at most this many rows. Default: `25`.
- `--all`: Print every row

### Examples

List every document that flare prints.

```sh
flare schemas list
```

### Columns

- `id`: The document's id, such as `shelf/books-list/v1`, which `schemas show` takes.
- `kind`: output when a command prints the document with its schema id, unwrapped when a command prints it without the envelope that holds its schema id, and file when it is a file format that users write and no command prints.
- `form`: stream when a command prints the document as a stream, one document on each line as each row arrives, and document when a command prints it once or it is a file format.
- `description`: What the document holds, in one line.

### Columns that `--long` adds

- `commands`: The command lines that print the document. It is empty for the failure document, which every command prints when it fails with --json, and for a file format.

Effects: read-only, closed-world.

## `flare schemas show`

Print the JSON Schema of each document

### Arguments

- `<ID>`: The ids of the documents that `schemas list` lists, separated by spaces. Required. Takes more than one value.

### Examples

Print the JSON Schema of the document that flare shares prints.

```sh
flare schemas show flare/shares/v1
```

### Columns

- `id`: The document's id, such as `shelf/books-list/v1`.
- `schema`: The document's 2020-12 JSON Schema.

Effects: read-only, closed-world.

## `flare skills`

Manage flare's agent skill

### Commands

- [`flare skills install`](#flare-skills-install): Install flare's skills into each agent skills directory that exists
- [`flare skills check`](#flare-skills-check): Check that each installed copy of flare's skills matches this flare
- [`flare skills print`](#flare-skills-print): Print flare's skill

## `flare skills install`

Install flare's skills into each agent skills directory that exists

### Arguments

- `--dir <DIR>`: Install into this skills directory in place of the agent skills directories
- `--force`: Replace a file or a link that install did not write with the skill's own folder

### Examples

Install flare's skills into each agent skills directory that exists.

```sh
flare skills install
```

Install flare's skills, replacing a file or a link that install did not write.

```sh
flare skills install --force
```

### Columns

- `state`: What install did: failed, skipped, written or unchanged.
- `skill`: The skill's name, which names its folder in each skills directory, such as `shelf`.
- `dirs`: The skills directories that share this copy, as named. Directories that resolve to one place through a symbolic link share one copy.
- `reason`: Why install skipped or failed, or what it replaced under --force, such as `linked to /src/shelf/skill`, or null.

### Columns that `--long` adds

- `path`: The SKILL.md file that install wrote or left, or the skill's other file whose write failed, at its real path.

Effects: additive, idempotent, closed-world.

## `flare skills check`

Check that each installed copy of flare's skills matches this flare

### Arguments

- `--dir <DIR>`: Check this skills directory in place of the agent skills directories

### Examples

Report each agent skills directory where flare's skills are missing or differ.

```sh
flare skills check
```

### Columns

- `state`: Whether the copy is the binary's: missing, differs, unreadable, linked or current.
- `skill`: The skill's name, which names its folder in each skills directory, such as `shelf`.
- `dirs`: The skills directories that share this copy, as named. Directories that resolve to one place through a symbolic link share one copy.
- `reason`: Why the copy is missing a file, differs, cannot be read or is linked, such as `written by shelf 0.2.0`, `REFERENCE.md is missing` or `linked to /src/shelf/skill`, or null.

### Columns that `--long` adds

- `path`: The SKILL.md file that was checked, at its real path.

Effects: read-only, closed-world.

## `flare skills print`

Print flare's skill

### Examples

Print flare's skill.

```sh
flare skills print
```

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
