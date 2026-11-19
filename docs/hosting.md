# Running hangar on a host

`hangar serve` serves a board's pages as a hosted site when its config has a
`hosted` section. People sign in with Google and read the pages that the
config's `admit` rule lets them reach. This page says what the operator sets
up around it.

## What the hosted site serves

- `/up`, for health checks. It answers 200 with no sign-in.
- The stylesheet, `/` and `/hangar/`, the index of pages.
- `/pages/<name>`, the newest version of a page.
- `/auth/oidc/start` and `/auth/oidc/callback`, the Google sign-in.
- Every other path answers 404, including bindings and forms, which only
  the local site serves.

## The config

```json
{
  "hosted": { "origin": "https://hangar.example.com", "listen": "127.0.0.1:8080" },
  "oidc": {
    "issuer": "https://accounts.google.com",
    "client_id": "<from the Google console>",
    "client_secret_file": "/run/secrets/hangar-client-secret",
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
  it.
- An `emails` admit rule admits any Google account that says a listed
  address is verified, whatever Workspace domain the account belongs to.
  A personal Google account can be created on an address at any domain and
  keeps that address after the mailbox is lost, so a listed address at a
  domain where someone else, such as a former employee, could have created
  one lets that account sign in. A `domain` rule admits only accounts of
  the Workspace domain, so it avoids this.
- Register `<origin>/auth/oidc/callback` as the redirect URI of the Google
  client, such as `https://hangar.example.com/auth/oidc/callback`.
- A config error exits with `hangar::config::invalid` and names the key.

## The client secret file

- `hangar serve` reads the file once, at start.
- The file must be a regular file, owned by the user hangar runs as or by
  root, not writable by group or others, and 1 to 4,096 bytes long. One
  trailing newline is dropped.
- A symlink is followed, so a container secret mount works.
- A world-readable file is accepted, because container runtimes mount
  secrets that way by default. Mode 0400 is recommended.
- A file that breaks a rule exits with `hangar::oidc::secret_unavailable`.
  The message names the path and the rule, and never the content.

## The proxy

hangar does not terminate TLS. A reverse proxy in front of it does, and
forwards plain HTTP to `hosted.listen`.

- The proxy must pass the Host header through unchanged. hangar refuses a
  request whose Host is not the origin's host with 403
  `request::host`.
- The proxy must not add an Authorization header. hangar refuses a request
  that has one with 401 `auth::bad_credential`.
- hangar reads no `X-Forwarded-*` or `Forwarded` header, and it does not
  use the peer address.
- Only the proxy may reach `hosted.listen`. Bind it to a loopback or
  private address, or firewall it. A client that reaches it directly
  skips the proxy's rate limits.
- Rate-limit `/auth/oidc/start` and `/auth/oidc/callback` per client at the
  proxy. hangar cannot tell clients apart behind the proxy. It lets 4
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
limit_req_zone $binary_remote_addr zone=hangar_sign_in:10m rate=10r/m;

server {
    listen 443 ssl;
    server_name hangar.example.com;
    location /auth/oidc/ {
        limit_req zone=hangar_sign_in burst=10 nodelay;
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
hangar.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

Stock Caddy has no per-client rate limit, so this example sets none. To
add one, build Caddy with the third-party `rate_limit` module from
`github.com/mholt/caddy-ratelimit` using `xcaddy`, or put a proxy that has
one, such as nginx above, in front of hangar.

On fly.io, the edge proxy terminates TLS and passes Host through. Set
`internal_port` to the port of `hosted.listen`, and bind `hosted.listen` to
`0.0.0.0` or `[::]` so the edge can reach it. That address is also
reachable from every other machine in the same fly.io organisation over
its private network, without the edge proxy, so run hangar in an
organisation that holds no machine you do not trust. The configuration
below sets no per-client rate limit, so on fly.io hangar's callback queue
is the only limit on sign-in requests:

```toml
[http_service]
  internal_port = 8080
  force_https = true

  [[http_service.checks]]
    path = "/up"
```

## One process

- Sessions and the key that seals pending sign-ins live in the memory of
  one process. Run exactly one hangar process behind an origin.
- A second process, or an old and a new process that overlap during a
  deploy, fail sign-ins at random and honour each session on only one of
  them. Stop the old process before starting the new one.
- A restart signs everyone out and voids every sign-in in progress.

## Sessions and revocation

- A session lasts 12 hours. A person keeps at most 8 sessions; a ninth
  sign-in ends the one that expires first. The server holds at most 1,024
  sessions.
- A person removed from the `admit` rule keeps a live session until it
  expires. Restarting the process is the only way to end every session at
  once.

## The domain name

- hangar sets only `__Host-` cookies, which are bound to its exact host.
- Serve hangar on a name whose parent domain has no untrusted subdomains.
  A sibling host can set many cookies on the parent domain and push every
  request past the header size limit.
- Every reply carries `Strict-Transport-Security: max-age=63072000`, so a
  browser that has visited uses HTTPS for two years.
