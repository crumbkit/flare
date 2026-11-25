# Working on flare

flare serves agent-written pages from a board and works on board records
through them. It is a Cargo workspace. `flare-core` holds the types and
traits that the other crates code against. The two `flare-auth-*` crates
sign people in, `flare-board` keeps pages and records in a board,
`flare-render` renders pages, `flare-server` serves them, and `flare-cli`
builds the `flare` command. The rules are the Rust standards that
[rust-standards.toml](rust-standards.toml) adopts. A change is ready when the
steps of the `lane` and `flare` jobs in
[.github/workflows/ci.yml](.github/workflows/ci.yml) pass with
`CARGO_BUILD_WARNINGS=deny`. Tests that run the board program read its path
from `FLARE_TEST_BOARD`. flare-aws's tests read stand-in URLs from
`FLARE_TEST_DYNAMODB` and `FLARE_TEST_S3`. The default filter of
[.config/nextest.toml](.config/nextest.toml) leaves flare-aws, flare-board
and flare-cli's `serve`, `pages`, `metrics` and `hosted_stores` tests out of
`cargo nextest run`. A full local
run sets those variables and passes `--ignore-default-filter`.

- Write the test first, named as a sentence (BASE-36).
- Give each new dependency a reason in `Cargo.toml` (BASE-23).
- A new implementation of `Authenticator`, `PageStore`, `RecordStore`,
  `ShareStore`, `GrantStore`, `FileStore`, `TokenStore` or `ResponseStore`
  calls every function of the matching suite in `flare_core::contract` from
  its own tests.

## Writing

Docs, comments and commit messages use plain declarative sentences with a
named subject. Avoid "not X but Y", fragments, metaphors and closing
flourishes. A stated limit says what it counts and what happens when it is
hit. Commit messages describe changed behaviour.
