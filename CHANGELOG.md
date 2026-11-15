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
- `hangar-render` renders a page and the records that fill its slots into a
  whole HTML document, with Markdown pages shown as prose and raw HTML shown
  as text. `check_page` refuses a Mustache template in which a record value
  could choose a URL or a style, add markup or reach a partial. `edit_form`
  draws the form that edits a filled slot's editable fields, `FormPost` and
  `Edit::apply` turn its post into a record write or a conflict, and
  `conflict_view` shows the conflict and saves the edit onto the newer record.

### Changed

- `OidcConfig::new` in `hangar-auth-oidc` takes an `Admit` rule in place of
  an `AllowList`. Wrap an existing allow-list in `Admit::Emails` to keep its
  behaviour.
