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
- `hangar-render` renders a page and the records that fill its slots into a
  whole HTML document, with Markdown pages shown as prose and raw HTML shown
  as text. `check_page` refuses a Mustache template in which a record value
  could choose a URL or a style, add markup or reach a partial. `edit_form`
  draws the form that edits a filled slot's editable fields, `FormPost` and
  `Edit::apply` turn its post into a record write or a conflict, and
  `conflict_view` shows the conflict and saves the edit onto the newer record.
