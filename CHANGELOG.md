# Changelog

This file records the notable changes to flare. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and flare follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `flare teams create`, `add`, `remove`, `list`, `show` and `delete` manage
  named teams of people. `flare share` and `flare grant` grant a share to a
  team with `--with team:<name>`, and the owner grants it to everyone
  signed in with `--with everyone`. `flare revoke` ends grants and `flare grants`
  lists them. Deleting a team leaves its grants as dormant grants that open
  nothing, and a team name is never reused. An everyone grant opens the
  share only to the people the `admit` rule admits, and an `anyone` admit
  rule refuses it. `docs/teams.md` gives the life of teams and group
  grants.
- `flare schemas list` lists the JSON documents that flare prints, and
  `flare schemas show <id>...` prints the JSON Schema of each of 1 to 100
  ids, so an agent learns the shape of `flare ... --json` from flare
  itself. An id that names no document fails with
  `flare::schemas::not_found`, whose recovery runs `flare schemas list`.
- Each tagged release attaches `flare` for macOS on Apple silicon and for
  Linux on arm64 and x86_64, and `brew install crumbkit/tap/flare` installs
  it. `docs/releasing.md` says how to install flare and how a release is
  cut.
- `flare show <id> [--version n]` prints one version of a share you can
  open, with its title, owner, your role, the latest version and the
  Markdown. `GET /api/shares/<id>` takes `?version=n`, and its answer
  names the version it holds.
- `flare share <file> --dry-run` checks the file with the server's own
  checks and renders it with flare-render, with no login and no request,
  and prints what it checked. `--out <path>` writes the rendered HTML.
  It also works with `--update`.
  flare has had no release, so the `flare/share/v1` document changes in
  place under the pre-release exemption of crumbkit's ADR-0007: `id`,
  `title` and `version` may be null, which a dry run prints, and the
  document gains a `checks` list.
- `flare serve` writes one CloudWatch Embedded Metric Format line on
  standard error for each request, created share, page view, sign-in,
  refused sign-in, token exchange and refused token when its config sets
  `"log": { "metrics": true }`. The setting is off when the config leaves
  it out. The lines hold counts only: a page view is one page response,
  no line names a person, a token, a session or a share, and no route
  serves the counts. `flare metrics [<file>...]` totals the lines of a log
  by day and by metric. `docs/metrics.md` lists every metric.
- `flare share <file> --update <id>` publishes the file as the next version
  of a share you own, through `POST /api/shares/<id>/versions`. Readers
  open the new content at once, and `flare share` and `flare grant` print
  the share's version. The option refuses `--title`, `--with` and `--re`.
- `flare login <server>` signs in to a hosted flare in the browser and
  keeps a token in the macOS Keychain. The server stores only the token's
  SHA-256 hash, and a token expires 90 days after its creation. A new login
  with the same label revokes the older token, and `flare logout --yes`
  revokes the token and deletes it. A token stops working once the
  server's `emails` admit rule no longer lists its owner.
  `docs/tokens.md` gives the life of login codes and tokens.
- `flare share <file> --with <addresses> --re <flare>` shares a Markdown
  file with named people as a response to flares. `flare grant`, `flare
  shares` and `flare activity` grant a share to more people, list shares
  and list the responses to flares. A flare the caller may not read answers
  exactly as a missing one, and activity gives no count of hidden
  responses.
  A response is also shared with the owner of each flare it responds to,
  as Reader, unless they already hold a role on it.
- `flare serve` serves the share API on a hosted site whose config has
  `hosted.stores`. `hosted.creators` lists who may share, and the admit
  rule `{ "anyone": true }` admits every verified address.
- `flare-contract` holds the share API's request, response and problem
  types and their JSON Schema. Every refusal is an RFC 9457 problem
  document with a `flare::<area>::<kind>` code.
- `flare-aws` keeps login codes, CLI tokens and response links in DynamoDB
  through `DynamoTokenStore` and `DynamoResponseStore`. The table deletes
  expired login codes through its time-to-live on `expires_at`.

- `hangar-aws` keeps shares and grants in one DynamoDB table and files in
  an S3 bucket, through `DynamoShareStore`, `DynamoGrantStore` and
  `S3FileStore`. Each write is one transaction, so a grant or revoke lands
  only while its writer is still an admin and the grant holds the role the
  writer read. A write whose answer is lost is resent with the same token
  and, at the 20-second deadline, settled by a consistent read.
  `HostedConfig` reads the region, table and bucket and refuses any
  credential key; credentials come only from the AWS provider chain.
  `infra/hangar-stores.template.json` defines the table, the private bucket
  and the IAM policy the stores need.
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
- `hangar-render` renders a page and the records that fill its slots into a
  whole HTML document, with Markdown pages shown as prose and raw HTML shown
  as text. `check_page` refuses a Mustache template in which a record value
  could choose a URL or a style, add markup or reach a partial. `edit_form`
  draws the form that edits a filled slot's editable fields, `FormPost` and
  `Edit::apply` turn its post into a record write or a conflict, and
  `conflict_view` shows the conflict and saves the edit onto the newer record.
- `hangar-server` serves a board's pages and bindings on loopback
  addresses. Each page renders inside a sandboxed `srcdoc` frame under a
  frame policy. A binding shows one edit form per editable slot, and a
  post writes the record only when its form token matches the session,
  binding, slot and record version it was drawn for; a stale form shows
  the conflict view. The person who runs the server signs in with a
  launch code, and accounts that the config admits sign in with Google
  through `localhost:8740`. Every refusal has a code, as an RFC 9457
  problem document or a small HTML page.
- The `hangar` command: `hangar serve`, `hangar open`, `hangar publish`
  and `hangar bind`, built on crumbkit.
- `BoardStore::check_program` in `hangar-board` checks that the board
  program can read history, and answers `TooOld` for a board older than
  crumbkit/board e4ee3333.
- Dropping the future of a `hangar-board` call before it resolves stops
  the board child's process group and waits until the child has ended. A
  caller that gives up on a write, such as a request past its deadline,
  knows that the write will not commit later.
- `hangar serve` serves a board's pages as a hosted site on a host behind a
  TLS-terminating proxy when its config has a `hosted` section with an
  `origin` and a `listen` address. People sign in with Google through
  `<origin>/auth/oidc/start` and read `/`, `/hangar/` and `/pages/<name>`.
  The hosted site refuses a request with another Host, an Authorization
  header, or a cross-origin or cross-site write; sets only `__Host-`
  cookies; sends HSTS on every reply; writes no server file; and never
  exits by itself. docs/hosting.md says how to run it behind a proxy.
- `FileSecret` in `hangar-auth-oidc` reads the client secret from a file
  that is regular, owned by the running user or root, not group- or
  world-writable, and 1 to 4,096 bytes long. `hangar serve` reads it from
  `oidc.client_secret_file`, which a hosted config requires.
- `hangar-server` takes a `Site`, either `Site::Local` with the gate and the
  state directory or `Site::Hosted` with a `PublicOrigin` and the OpenID
  Connect authenticator. `Limits::token_calls` caps the sign-in callbacks
  that wait on the provider at once. A callback past the cap waits for a
  slot in arrival order for up to `Limits::token_wait`, and at most
  `Limits::token_waiters` wait at once; a callback that finds the queue
  full or whose wait runs out answers 503 `server::busy`.
- `SignedIn::session_secret` in `hangar-auth-oidc` gives the secret of the
  session credential, so a server can set it as a cookie without matching
  on the credential's kind.
- `hangar-core` has shares: a Markdown document that one owner publishes in
  numbered versions and opens to named people with a Reader or Admin grant.
  The `Shares` service decides every request from the caller's verified
  email, the share and its grants. A person with no grant gets the same
  `NotFound` as for a share that does not exist, a revoke takes effect on
  the next request, and an admin manages readers and cannot touch another
  admin or publish. A share holds at most 100 versions and 256 grants, and
  one owner at most 500 shares; a write past a limit is refused and writes
  nothing.
- `ShareStore`, `GrantStore` and `FileStore` in `hangar-core`, each with a
  contract suite and an in-memory store behind the `testing` feature. A
  grant or revoke names the role its writer saw and the admin it acts
  under, and the store refuses it when either changed.
- `Principal` carries the email address that its sign-in verified
  (`Principal::with_email` and `Principal::email`), and `hangar-auth-oidc`
  sets it under both admit rules.

### Changed

- The project is renamed from hangar to flare. The crates are `flare-core`,
  `flare-auth-loopback`, `flare-auth-oidc`, `flare-board`, `flare-render`,
  `flare-server`, `flare-aws` and `flare-cli`, and the command is `flare`.
  Failure codes start with `flare::`. The cookies are `flare_session` and
  `flare_oidc_state`, with the `__Host-` prefix on the hosted site. The
  config file is `flare/config.json` under `$XDG_CONFIG_HOME` or
  `~/.config`, the state directory is `$XDG_STATE_HOME/flare`, the Keychain
  service is `flare-oidc-google`, and the environment variables start with
  `FLARE_`. The record types are `flare/page/v1`, `flare/binding/v1` and
  `flare/edit/v1`, and the home path is `/flare/`. flare reads none of the
  old names: a browser signs in again, a config file moves from
  `~/.config/hangar` to `~/.config/flare`, a running `hangar serve` is
  stopped and `flare serve` started, and each page that hangar published is
  published again with `flare publish`. The entries above and below this
  one use the names of the time they were written.
- `Email` moves from `hangar-auth-oidc` to `hangar-core`, which
  `hangar-auth-oidc` re-exports. Parsing one answers a `ParseError`, and
  `ConfigError::Email` is gone.
- The record suite's concurrent-writes clause races the two fixture bodies
  that differ from the record's current body, so a store may answer a
  write of the current body without writing.

- `OidcConfig::new` in `hangar-auth-oidc` takes an `Admit` rule in place of
  an `AllowList`. Wrap an existing allow-list in `Admit::Emails` to keep its
  behaviour.
- A pending sign-in in `hangar-auth-oidc` is sealed with AES-256-GCM into
  the kept-state cookie, and the server keeps no table of them, so any
  number of sign-ins can be in progress and a restart voids them all. A
  kept value is refused 10 minutes after its start. A replayed callback is
  refused by the provider as `CodeRejected`.
- The local sign-in host reads its kept-state cookie byte by byte, as the
  hosted site does, so a non-ASCII cookie that another app set on
  `localhost` no longer fails a sign-in.
- `OidcAuthenticator::complete` takes the browser's current session
  credential, if any, and ends that session only when the new one is
  created. A principal keeps at most 8 sessions; a ninth sign-in ends its
  session that expires first.
- `App::new` in `hangar-server` takes a `Site` in place of a `Gate` and a
  `StateDir`.
