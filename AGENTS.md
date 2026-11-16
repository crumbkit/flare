# Working on hangar

hangar serves agent-written pages from a board and works on board records
through them. It is a Cargo workspace. `hangar-core` holds the types and
traits that the other crates code against. The two `hangar-auth-*` crates
sign people in, `hangar-board` keeps pages and records in a board,
`hangar-render` renders pages, `hangar-server` serves them, and `hangar-cli`
builds the `hangar` command. The rules are the Rust standards that
[rust-standards.toml](rust-standards.toml) adopts. A change is ready when the
steps of the `lane` job in [.github/workflows/ci.yml](.github/workflows/ci.yml)
pass with `CARGO_BUILD_WARNINGS=deny`. Tests that run the board program read
its path from `HANGAR_TEST_BOARD`.

- Write the test first, named as a sentence (BASE-36).
- Give each new dependency a reason in `Cargo.toml` (BASE-23).
- A new implementation of `Authenticator`, `PageStore` or `RecordStore` calls
  every function of the matching suite in `hangar_core::contract` from its own
  tests.

## Writing

Docs, comments and commit messages use plain declarative sentences with a
named subject. Avoid "not X but Y", fragments, metaphors and closing
flourishes. A stated limit says what it counts and what happens when it is
hit. Commit messages describe changed behaviour.
