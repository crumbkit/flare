---
name: flare
description: Share a Markdown document with named people through a flare server with the `flare` command, read a share, grant it to more people, list shares and follow the responses to a flare. Use it when someone asks you to share or send a document with flare, to read a flare someone shared, or to check who responded to one.
---

# flare

`flare` shares Markdown files through a flare server. A share has an id, a
title, an owner and numbered versions. The people it is granted to read it
in the browser on the share's page. A share can respond to other shares,
called flares, and the owners of those flares can then read it.

## Before you start

- Pass `--json` to every command and read the JSON document it prints on
  standard output. Pass `--no-input` too, so a command fails in place of
  asking a question that nobody can answer.
- When a command fails, it exits with 1 and prints one
  `flare/failure/v1` document, with `code`, `message` and `recovery` at
  the top level, such as `flare::login::needed` for `code`. A command
  that takes many ids, such as `flare schemas show`, instead prints its
  rows under `data` and a failure for each id it could not serve under
  `failures`, each in the same shape.
- Act on the failure's `recovery` by its `kind`:
  - `run`: run the command in `call`, then run the original command
    again. A word in angle brackets in `call`, such as `<ADDRESS>`, is a
    value to fill in from the failed command or from the person, and `...`
    after it means the value may repeat.
  - `terminal`: only a person can run the command in `call`, such as a
    browser sign-in. Ask the person to run it, and wait for them before
    you run the original command again.
  - `retry`: run the original command again.
  - `after`: make the repair that `repair` describes first, then take
    the step in `then`, which is a `run` or `retry` step as above.
  - `hint`: follow the advice in `text`. When it names a step for a
    person, ask the person.
  - `none`: flare offers no next step. Tell the person the failure's
    `message` and the recovery's `reason`.
  - A failure may also list `alternatives`, other recoveries of the same
    kinds, to use when the first one does not apply.
- `flare schemas show <id>` prints the JSON Schema of a document, such as
  `flare schemas show flare/shares/v1`. `flare schemas list` lists every
  document id.
- `flare commands list` lists every command, and `flare <command> --help`
  describes its flags and the columns it prints.

## Sign in

`flare login <server>` opens the browser to sign in to a flare server and
keeps a token in the macOS Keychain. Only a person can finish the sign-in,
so a failure that needs a new sign-in, such as `flare::login::needed`,
gives a `terminal` recovery, or a `hint` when flare does not know the
server. Never look for, print or copy the token.

## Share a document

- `flare share notes.md --with <address>` shares a Markdown file with the
  people whose addresses `--with` names. Repeat `--with` or separate the
  addresses with commas. The JSON's `data.id` is the share's id.
- `flare share answer.md --re <id>` shares a file as a response to one or
  more flares. The owners of those flares become readers of it.
- `flare share notes.md --update <id>` publishes the file as the next
  version of a share you own. It changes only the content, so pass no
  `--title`, `--with` or `--re` with it.
- `flare share notes.md --dry-run` checks the file as the server would and
  shares nothing. It needs no login. Add `--out notes.html` to write the
  rendered HTML and look at it. Run a dry run first when you are unsure the
  file will pass.

## Read and grant

- `flare show <id>` prints the latest version of a share you can open: its
  title, owner, your role and the Markdown. `--version <n>` prints an older
  version.
- `flare grant <id> --with <address>` grants a share you own to more people.
- `flare shares` lists your own shares, newest first. `flare shares --with-me`
  lists the shares granted to you.

## Follow responses

`flare activity <id>` lists the responses to a flare, oldest first. Its
JSON holds the next step under `next`: each step has a `text` and a
`commands` list whose `call` is the next `flare activity` command, with
the same flares and `--since` and the cursor where this page ended. Keep
that call and run it as given to list only the newer responses. Do not
build the `--since` argument yourself. `summary.more` is true when more
responses wait past this page.

## Comments

The command line does not read or write comments. People read and write
comments on the share's page in the browser.

## Keep this skill current

`flare skills install` writes this skill into each agent skills directory,
and `flare skills check` reports a copy that is missing or out of date.
`flare skills print` prints the skill that the installed binary carries.
