# Changelog

This file records the notable changes to hangar. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and hangar follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `hangar-core` defines the validated types of pages, bindings, records,
  provenance and principals, the `Authenticator`, `PageStore` and
  `RecordStore` traits, in-memory stores, and a contract suite for each
  trait that every implementation runs from its own tests.
- `hangar-auth-oidc` signs people in with an OpenID Connect provider such as
  Google. It runs the authorisation code flow with PKCE, verifies the RS256
  ID token, admits only verified emails on its allow-list, keeps sessions in
  memory and reads the client secret from the macOS Keychain.
- `hangar-auth-oidc` can admit every verified account of one Google
  Workspace domain with `Admit::Domain`. The signed `hd` claim of the ID
  token decides membership, and the email's domain does not. A sign-in
  without `hd` is refused as `NoHostedDomain`, and one with another `hd` as
  `OtherHostedDomain`. Each admitted account signs in as its email, and
  `authorize` allows it while one of its sessions is live.
- `hangar-auth-loopback` signs in the person who runs a local hangar server.
  Each server gets a random `h-<32 hex>.localhost` host name and a bearer
  token, which it writes to a private server file. A one-use launch code,
  valid for 60 seconds, sets a host-only session cookie. Every request is
  checked for a loopback peer, the exact Host and, when it changes
  something, the server's own Origin.
- `hangar-board` keeps pages, bindings and record edits in a board. Its
  `BoardStore` runs the board program for every read and write, so each
  publish, save, move and edit is one commit. A page version is the commit
  that made it. Each record edit adds a `hangar/edit/v1` record that names
  who made it and from which version, in the same commit. A typed Markdown
  record, such as a task, reads as one JSON object with its prose under
  `$body`.
- `RecordStore::read_all` reads several records at one state of the store,
  and every page store reads a binding's fills with it, so a fit check
  never mixes two states of the records. Its default body reads the paths
  in turn.
- `RecordFixture::with_bodies` and `PageFixture::with_bodies` run the
  contract suites with bodies in a store's own form.

### Changed

- The record suite's concurrent-writes clause races the two fixture bodies
  that differ from the record's current body, so a store may answer a
  write of the current body without writing.

- `OidcConfig::new` in `hangar-auth-oidc` takes an `Admit` rule in place of
  an `AllowList`. Wrap an existing allow-list in `Admit::Emails` to keep its
  behaviour.
