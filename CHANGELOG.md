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
- `hangar-auth-loopback` signs in the person who runs a local hangar server.
  Each server gets a random `h-<32 hex>.localhost` host name and a bearer
  token, which it writes to a private server file. A one-use launch code,
  valid for 60 seconds, sets a host-only session cookie. Every request is
  checked for a loopback peer, the exact Host and, when it changes
  something, the server's own Origin.
