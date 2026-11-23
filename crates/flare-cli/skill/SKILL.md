---
name: flare
description: Share a Markdown document with named people through a flare server with the `flare` command, read a share, grant it to more people, list shares and follow the responses to a flare. Use it when someone asks you to share, publish or send a document with flare, to read a flare someone shared, or to check who responded to one.
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
- When a command fails, the JSON has a `failures` list. Read each
  failure's `code`, such as `flare::login::needed`, and its `recovery`.
  When the recovery's `kind` is `run`, run the command in its `call`,
  filling in any value it names, and then run the original command again.
  When it is `retry`, run the original command again.
- `flare schemas show <id>` prints the JSON Schema of a document, such as
  `flare schemas show flare/shares/v1`. `flare schemas list` lists every
  document id.
- `flare commands list` lists every command, and `flare <command> --help`
  describes its flags and the columns it prints.

## Sign in

`flare login <server>` opens the browser to sign in to a flare server and
keeps a token in the macOS Keychain. Only a person can finish the sign-in.
When a command fails with `flare::login::needed`, ask the person to run
`flare login <server>` with their server's address, and wait for them. Do
not look for, print or copy the token.

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

`flare activity <id>` lists the responses to a flare, oldest first. The
JSON's `summary.cursor` marks where the list ended. Keep it, and pass it
next time as `flare activity <id> --since <cursor>` to list only the newer
responses. `summary.more` is true when more responses wait past this page.

## Comments

The command line does not read or write comments. People read and write
comments on the share's page in the browser.

## Keep this skill current

`flare skills install` writes this skill into each agent skills directory,
and `flare skills check` reports a copy that is missing or out of date.
`flare skills print` prints the skill that the installed binary carries.
