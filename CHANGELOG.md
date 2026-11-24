# Changelog

This file records the notable changes to flare. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and flare follows
[Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- `GET /api/shares` answers `next`, a cursor, when more shares follow the
  page, and takes `cursor=<cursor>` to read the page after it.
  `flare shares --cursor <CURSOR>` resumes a list, a list cut by `--limit`
  names the next call with its cursor, and `flare shares --with-me --all`
  lists every share granted to you, past the 500 that one answer holds.

- `flare skills install` writes flare's agent skill to `flare/SKILL.md` in
  each of `~/.claude/skills`, `~/.agents/skills` and `$CODEX_HOME/skills`
  that exists, or in the directory that `--dir` names. The skill tells an
  agent how to sign in, share, read, grant and follow responses with
  `flare`, and the binary carries it, so the skill matches the installed
  version. Install replaces only a copy that flare wrote, and `--force`
  replaces any other. `flare skills check` lists each copy that is missing
  or differs and exits with 1 when it lists one, and `flare skills print`
  prints the skill.
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

- A `flare::state::unusable` failure from the saved login or the login
  labels file now names the file at the start of its message, so the text
  form, which prints no fields, says which file to fix. A login.json that
  flare cannot read or parse reads "the saved login <path> could not be
  used: <reason>", and a labels.json that flare cannot read, parse or
  write, or that would pass 64 KiB, reads "the login labels file <path>
  could not be used: <reason>". A state directory that flare cannot create
  for either file reads "the state directory <path> could not be created:
  <reason>". A file past its size limit gives the reason "it is longer than
  4096 bytes" or "it is longer than 65536 bytes". The `path` field keeps
  its value. The failure's help line now reads "fix or remove the state
  file or directory that the problem names".
- Three failures give the file they name in a field of the JSON failure
  document, so an agent no longer reads the path out of the message.
  `flare::board::program_unavailable` gives `program`, the board program
  flare could not resolve or that did not answer. `flare::state::unusable`
  gives `path`, the file or directory flare could not use: the saved login
  or label file, the state directory it could not create, or the loopback
  state's directory or server file.
  `flare::opener::failed` gives `program`, the opener that did not start,
  ran past its deadline or exited with an error. Each field is `null` when
  flare does not hold the path apart from the message. The messages read
  as before, except that a board program that does not answer now reads
  "the board program /path: did not answer".
- `flare::metrics::unreadable` gives the hint "pass a readable log file"
  and the alternative "pipe the log to flare metrics on standard input" as
  two recoveries, in place of one hint that joined them with "or". With
  standard input on a terminal, its message ends "standard input is a
  terminal".
- A failure that needs a new sign-in now gives a `terminal` recovery for
  `flare login <server>` with the server filled in, because only a person
  can finish the browser sign-in. This covers `flare::login::needed` with a
  saved login, `flare::login::failed`, and the server's
  `flare::tokens::sign_in_needed`, `flare::tokens::code_refused` and
  `flare::shares::no_email`. With no saved login flare does not know the
  server, so `flare::login::needed` gives a hint that names `flare login`.
  These recoveries were `run` recoveries before.
- `flare::network::bad_answer` recovers with a repair and then a retry:
  check that the server runs flare with the share API, then run the same
  command again. In JSON its recovery is of kind `after`. After `flare
  login`'s token exchange, every step that would run the login again is a
  `terminal` recovery for `flare login <server>`, because only a person
  can finish the new sign-in. That covers the step after the check of
  `flare::network::bad_answer`, and `flare::network::unreachable`,
  `flare::tokens::unavailable` and `flare::system::unavailable` from the
  exchange, which recovered with a retry before.
- A `flare::shares::partly_shared` refusal names `flare grant` as a call
  with the share the server named, and a `flare::activity::bad_cursor`
  refusal names `flare activity` with the flare ids the command was given
  and without a cursor, with an alternative that runs it with `--since`
  once the right cursor is found.
- `flare activity` prints its next command as a step: in text, a `Next:`
  sentence on standard error with `flare activity <id>... --since <cursor>`
  under it, and in JSON, the same call under `next`. The summary line
  `Next: --since <cursor>` on standard output is gone, and the JSON
  `summary.cursor` and `summary.more` stay.
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

### Fixed

- A release run whose `release` job cannot read whether the tag already has
  a release, because of a token, rate-limit, network or GitHub fault, now
  prints gh's error and `could not read whether <tag> has a release, so the
  job made no release; re-run the failed jobs once GitHub answers`, and
  fails before any write. Before this fix the job treated every failed read
  as "no release" and ran `gh release create`, which hid the cause and could
  give a tag a second release beside a draft that an earlier run left.
- `flare share` and `flare share --update` no longer tell the caller to run
  the same command again after the server answers with success and flare
  cannot read the answer, or does not answer a request it was sent,
  because the server may already have made the share or the version, and
  a second run would make a second one. The failures keep their codes,
  `flare::network::bad_answer` and `flare::network::unreachable`, and
  their recovery says to check that `flare shares` does not already list
  the share or the version before running the command again. A 502, 503
  or 504 answer with no problem document, which a proxy in front of the
  server may send after the server made the share, gets the same check,
  and so does the server's own `flare::shares::unavailable` refusal of a
  share or a version, which it sends when it does not know whether the
  write happened; `flare shares` keeps the retry for that refusal. A
  connection that was refused sends nothing, so it keeps the retry, any
  other answer that is not a success keeps the server check and retry, and
  other commands keep their recoveries.
- The recoveries of `flare::server::not_running` and
  `flare::server::already_running` carry the `--board` that the failed
  command was given, so the next `flare serve` or `flare open` names the
  same board. A `flare::server::port_in_use` for the sign-in port or the
  hosted listen address, which `flare serve` cannot move, asks for the
  program that holds the port to stop before `flare serve` runs again; a
  held `--port` still recovers with `flare serve` without `--port`. The
  `flare serve` call of `port_in_use` keeps the failed command's `--board`,
  `--board-program`, `--config` and `--idle-exit`, so a hosted server is
  not run again as a local one.
- Sign-ins that arrive while another sign-in is fetching the provider's key
  set wait for that fetch and take its outcome. When the fetch fails, each
  of them is refused with the same `ProviderUnavailable` or
  `ProviderMisbehaved`, so ten sign-ins that arrive while the provider is
  down cost one fetch and one request deadline. Before, each waiting
  sign-in fetched again in turn, or was refused as an unknown key. A
  sign-in that is dropped during its fetch no longer stops the next
  sign-in with a new key id from fetching for 60 seconds.
- `BoardStore` publishes a page whose first publish wrote its files and
  made no commit, which a put stopped at its deadline or answered
  `board::put::head_moved` leaves behind. The page's first committed
  version names no previous version. Before, every later publish of that
  page answered `Unavailable` until a person deleted the files.
- `save_binding`, `move_binding`, and a publish or a binding whose origin
  is a page version refuse a version that `HEAD` cannot reach, such as one
  on another branch or one a reset orphaned, so a binding can no longer pin
  a version that `versions` does not list. Reading a version with `at`
  still reads one commit. `docs/board-store.md` holds flare-board's state
  tables, crash points, invariants and accepted gaps.
