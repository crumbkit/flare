# Running flare on a host

`flare serve` serves a board's pages as a hosted site when its config has a
`hosted` section. People sign in with Google and read the pages that the
config's `admit` rule lets them reach. This page says what the operator sets
up around it.

## What the hosted site serves

- `/up`, for health checks. It answers 200 with no sign-in.
- The stylesheet, `/` and `/flare/`, the index of pages.
- `/pages/<name>`, the newest version of a page.
- `/auth/oidc/start` and `/auth/oidc/callback`, the Google sign-in.
- The share API, when the config has `hosted.stores`. The share API
  section below lists its routes.
- Every other path answers 404, including bindings and forms, which only
  the local site serves.

## The config

```json
{
  "hosted": { "origin": "https://flare.example.com", "listen": "127.0.0.1:8080" },
  "oidc": {
    "issuer": "https://accounts.google.com",
    "client_id": "<from the Google console>",
    "client_secret_file": "/run/secrets/flare-client-secret",
    "admit": { "domain": "example.com", "reach": "everything" }
  }
}
```

- `hosted.origin` is the public origin people type: `https://<host>` or
  `https://<host>:<port>`, with no path, not even a trailing slash.
- `hosted.listen` is the address of the one plain-HTTP listener, such as
  `127.0.0.1:8080`.
- `oidc.client_secret_file` is required with a `hosted` section. It is an
  absolute path.
- `admit` is the same rule the local site uses; the root README describes
  it. A hosted site also takes `{ "anyone": true, "reach": ... }`, which
  admits every Google account with a verified address and needs
  `hosted.creators`.
- An `emails` admit rule admits any Google account that says a listed
  address is verified, whatever Workspace domain the account belongs to.
  A personal Google account can be created on an address at any domain and
  keeps that address after the mailbox is lost, so a listed address at a
  domain where someone else, such as a former employee, could have created
  one lets that account sign in. A `domain` rule admits only accounts of
  the Workspace domain, so it avoids this.
- Under a `domain` rule, a CLI token keeps working after its owner's
  account is suspended or leaves the Workspace domain, until the token
  expires up to 90 days after it was issued. The server checks only the
  owner's address on each request, and an address alone does not show
  that the account left. To end such a person's access sooner, delete
  their token items from the table: each has the partition key
  `TOKEN#<id>`, the sort key `TOKEN` and the person's address in `owner`.
  A deleted token answers 401 `flare::tokens::sign_in_needed`. The other
  remedy is to switch to an `emails` rule that leaves the person off and
  restart the server.
- Register `<origin>/auth/oidc/callback` as the redirect URI of the Google
  client, such as `https://flare.example.com/auth/oidc/callback`.
- A config error exits with `flare::config::invalid` and names the key.

## The share API

The share API lets people share Markdown files with named people from the
`flare` command. The site serves it when the config's `hosted` section has
`stores`.

```json
"hosted": {
  "origin": "https://flare.example.com",
  "listen": "127.0.0.1:8080",
  "stores": { "region": "eu-west-1", "table": "flare-shares", "bucket": "flare-files" },
  "creators": ["ana@example.com", "@example.com"],
  "names": { "ana": "ana@example.com" }
}
```

- `stores` names the DynamoDB table and the S3 bucket. The template
  `infra/flare-stores.template.json` creates both, with the table's
  time-to-live on `expires_at` and a managed policy that grants what flare
  needs.
- flare reads AWS credentials from the AWS SDK's default chain, such as
  the environment, a profile or an instance role. They are never in the
  config.
- `flare serve` checks the table and the bucket at start and exits with
  `flare::stores::unavailable` when it cannot read them.
- `creators` says who may share and respond. It is a list of addresses
  and `@domain` entries, such as `"ana@example.com"` and `"@example.com"`,
  where a domain entry admits every verified address at exactly that
  domain and not its subdomains. It can instead be the value
  `"anyone admitted"`, which lets everyone the `admit` rule admits share.
  Under an `anyone` admit rule, `"anyone admitted"` means open sharing:
  any account the issuer verifies, which with Google is any Google
  account, can sign in and create shares. `flare serve` logs a warning at
  start when the config has this combination. A domain entry such as
  `@gmail.com` likewise admits every address at that public domain.
  Without `creators`, everyone a `domain` or `emails` rule admits may.
  `creators` without `stores` is refused, and an `anyone` admit rule needs
  `creators` written out, so that letting every signed-in person share is a
  choice. Anything else in `creators` is refused naming `hosted.creators`.
- `names` maps usernames, written in lower-case letters, digits, dots,
  hyphens and underscores, to addresses, such as
  `{ "ana": "ana@example.com" }`, so that `flare share --with ana` resolves
  to `ana@example.com`. A full address still resolves as itself, and a name
  that is not listed is refused as not an email address. `names` without
  `stores` is refused, and a bad entry is refused naming `hosted.names`.
- A person who is not a creator gets 403 `flare::shares::not_a_creator`.

Routes:

- `GET /cli/login` sends a signed-in browser back to `flare login` with a
  login code.
- `POST /api/tokens/exchange` trades a login code for a token.
  [tokens.md](tokens.md) gives the life of codes and tokens.
- `DELETE /api/tokens/<id>` revokes the caller's token.
- `POST /api/shares` creates a share, with its grants and the flares it
  responds to.
- `GET /api/shares?scope=mine|with-me&limit=<n>` lists shares, at most 500.
- `GET /api/shares/<id>` reads a share.
- `POST /api/shares/<id>/versions` publishes the next version of a share,
  from a `format` and `content`. Only the owner may, and the answer is the
  share summary with the new `version`.
- `POST /api/grants/<id>` grants a share to more addresses.
- `GET /api/activity?flare=<id>&since=<cursor>&since_last=<name>&limit=<n>`
  lists the responses and comment events on flares that the caller may
  read, oldest first, at most 100 a page.
- `GET` and `POST /api/comments`, `PATCH` and `DELETE /api/comments/<id>`
  and `GET` and `PUT /api/cursors/<name>` read and write comments and named
  cursors. `docs/comments.md` gives their life.

- Every `/api/` route except the exchange needs a bearer token, and the
  comment routes and activity also take the session cookie of a
  same-origin browser request. A missing,
  expired or revoked token answers 401 `flare::tokens::sign_in_needed`.
  Under an `emails` admit rule, so does the token of an owner whom the
  list no longer names. Under a `domain` or `anyone` rule a token works
  until it expires or is revoked.
- Every refusal is an RFC 9457 problem document with a `code` such as
  `flare::shares::not_found`.
- A flare the caller may not read answers exactly as a missing one, 404
  `flare::shares::not_found`.
- Activity shows only responses the caller may read. It gives no count of
  the others.
- The activity cursor is `fc1.` and a base64url position. A cursor flare did
  not write answers 400 `flare::activity::bad_cursor`.

## The client secret file

- `flare serve` reads the file once, at start.
- The file must be a regular file, owned by the user flare runs as or by
  root, not writable by group or others, and 1 to 4,096 bytes long. One
  trailing newline is dropped.
- A symlink is followed, so a container secret mount works.
- A world-readable file is accepted, because container runtimes mount
  secrets that way by default. Mode 0400 is recommended.
- A file that breaks a rule exits with `flare::oidc::secret_unavailable`.
  The message names the path and the rule, and never the content.

## The proxy

flare does not terminate TLS. A reverse proxy in front of it does, and
forwards plain HTTP to `hosted.listen`.

- The proxy must pass the Host header through unchanged. flare refuses a
  request whose Host is not the origin's host with 403
  `request::host`.
- The proxy must pass the Authorization header of `/api/` requests through
  unchanged and must not add one to any request. flare refuses an
  Authorization header outside `/api/` with 401 `auth::bad_credential`.
- flare reads no `X-Forwarded-*` or `Forwarded` header, and it does not
  use the peer address.
- Only the proxy may reach `hosted.listen`. Bind it to a loopback or
  private address, or firewall it. A client that reaches it directly
  skips the proxy's rate limits.
- Rate-limit `/auth/oidc/start`, `/auth/oidc/callback` and
  `/api/tokens/exchange` per client at the proxy. flare cannot tell clients apart behind the proxy. It lets 4
  callbacks wait on Google at once. The next 16 callbacks queue for a
  slot in the order they arrived, for up to 10 seconds each, and answer
  503 `server::busy` when the wait runs out. A callback that finds 16
  already queued answers 503 `server::busy` at once. A person's callback
  therefore waits behind the callbacks already queued, and a client that
  keeps more than 20 callbacks open at once can still make sign-ins fail
  until the proxy limits it.
- A callback can take up to 40 seconds: 10 waiting for a slot and 30 in
  work. Keep the proxy's upstream read timeout above that. nginx's
  default is 60 seconds.
- Point the proxy's health check at `/up`.

nginx sends its own upstream name as Host unless told otherwise. This
example also limits each client address to 10 sign-in requests a minute,
with a burst of 10, and answers 429 past that. `limit_req_zone` belongs in
the `http` block, which a file in `conf.d` is included into:

```nginx
limit_req_zone $binary_remote_addr zone=flare_sign_in:10m rate=10r/m;

server {
    listen 443 ssl;
    server_name flare.example.com;
    location /auth/oidc/ {
        limit_req zone=flare_sign_in burst=10 nodelay;
        limit_req_status 429;
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
    }
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
    }
}
```

Caddy passes Host through and adds no Authorization header by default:

```text
flare.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

Stock Caddy has no per-client rate limit, so this example sets none. To
add one, build Caddy with the third-party `rate_limit` module from
`github.com/mholt/caddy-ratelimit` using `xcaddy`, or put a proxy that has
one, such as nginx above, in front of flare.

On fly.io, the edge proxy terminates TLS and passes Host through. Set
`internal_port` to the port of `hosted.listen`, and bind `hosted.listen` to
`0.0.0.0` or `[::]` so the edge can reach it. That address is also
reachable from every other machine in the same fly.io organisation over
its private network, without the edge proxy, so run flare in an
organisation that holds no machine you do not trust. The configuration
below sets no per-client rate limit, so on fly.io flare's callback queue
is the only limit on sign-in requests:

```toml
[http_service]
  internal_port = 8080
  force_https = true

  [[http_service.checks]]
    path = "/up"
```

## More than one process

- When `hosted.stores` is set, sessions and the keys that seal pending
  sign-ins live in the table, so any number of processes behind one origin
  honour each other's sessions and sign-ins, and a restart or a deploy
  signs nobody out.
- Without `hosted.stores`, sessions and keys live in the memory of one
  process. Run exactly one process, and expect a restart to sign everyone
  out and void every sign-in in progress.
- Shares, login codes and CLI tokens live in the table and the bucket, so
  a restart keeps them.
- The table's TTL deletes expired sessions and old keys. Every read
  compares the expiry with the clock first, so a late deletion changes no
  answer.

## Sessions and revocation

- A session expires 12 hours after the last request that extended it. A
  request extends it only when 10 minutes of the 12 hours are used, so a
  busy page costs a few writes an hour. No session lives longer than 7 days
  after sign-in. A person keeps at most 8 sessions; a ninth sign-in ends the
  one that expires first. Without `hosted.stores` the process holds at most
  1,024 sessions.
- A person removed from the `admit` rule keeps a live session until it
  expires, except on the comment and activity routes, which check the
  share API's admit rule for a cookie as they do for a token. With `hosted.stores`, ending every session of one person means
  deleting the items under `SESSOF#<person id>` and the sessions they
  name; without it, restarting the process ends every session.

## The domain name

- flare sets only `__Host-` cookies, which are bound to its exact host.
- Serve flare on a name whose parent domain has no untrusted subdomains.
  A sibling host can set many cookies on the parent domain and push every
  request past the header size limit.
- A hosted reply's `Content-Security-Policy` has `connect-src 'self'`,
  which limits the requests a page makes to this site's origin. It has no
  `script-src`, so a hosted page runs no script today.
- Every reply carries `Strict-Transport-Security: max-age=63072000`, so a
  browser that has visited uses HTTPS for two years.
