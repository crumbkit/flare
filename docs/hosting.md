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
  proxy. hangar caps the callbacks that wait on Google at 4 and answers
  503 `server::busy` past that, but it cannot tell clients apart
  behind the proxy.
- Point the proxy's health check at `/up`.

Caddy passes Host through and adds no Authorization header by default:

```text
hangar.example.com {
	reverse_proxy 127.0.0.1:8080
}
```

nginx sends its own upstream name as Host unless told otherwise:

```nginx
server {
    listen 443 ssl;
    server_name hangar.example.com;
    location / {
        proxy_pass http://127.0.0.1:8080;
        proxy_set_header Host $host;
    }
}
```

On fly.io, the edge proxy terminates TLS and passes Host through. Set
`internal_port` to the port of `hosted.listen`, and bind `hosted.listen` to
`0.0.0.0` or `[::]` so the edge can reach it:

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
