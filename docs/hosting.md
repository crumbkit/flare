# Running flare on a host

`flare serve` serves a board's pages as a hosted site when its config has a
`hosted` section. People sign in with Google, or through a single sign-on
proxy in front of the site, and read the pages that the config's `admit`
rule lets them reach. This page says what the operator sets up around it,
and [sign-in.md](sign-in.md) describes the proxy sign-in.

## What the hosted site serves

- `/up`, for health checks. It answers 200 with no sign-in.
- The stylesheet, `/` and `/flare/`, the index of pages.
- `/pages/<name>`, the newest version of a page.
- `/s/<id>`, the reading page of a Markdown share, for a signed-in reader
  the share is granted to. `?version=<n>` shows an older version.
  [reading.md](reading.md) describes the page.
- `/flare/assets/<file>`, the reading page's script files, with a cache of
  one year.
- `/auth/oidc/start` and `/auth/oidc/callback`, the Google sign-in. A site
  that signs people in through a proxy answers 404 on them.
- The share API, when the config has `hosted.stores`. The share API
  section below lists its routes.
- Every other path answers 404, including bindings and forms, which only
  the local site serves.

## The config

```json
{
  "hosted": { "origin": "https://flare.example.com", "listen": "127.0.0.1:8080" },
  "log": { "metrics": true },
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
- `log.metrics` set to `true` writes a CloudWatch metric line on standard
  error for each request, share, page view, sign-in and refusal. A hosted
  site sets it, so its operator can count use in CloudWatch; it is off
  when the config leaves it out. The counts hold no person, token or
  share, and no route of the site serves them.
  [metrics.md](metrics.md) lists the metrics.
- A hosted config takes exactly one of the `oidc` section and a `proxy`
  section. [sign-in.md](sign-in.md) describes the `proxy` section.
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
  "names": { "ana": "ana@example.com" },
  "get_flare": "https://example.com/flare",
  "identity": "email",
  "everyone_signed_in": false,
  "teams": "stored"
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
- `creators` says who may share, respond and create a team. Who may
  create is a rule that each app adopting flare supplies: flare-core's
  `Creators` trait, which `Flares::new` takes. The default rule,
  `AllAdmitted`, lets every person the `admit` rule admits create. The
  optional rule, `CreatorList`, allows only listed addresses and domains.
  The `flare` command picks one of these two from `creators` when it
  starts. An app that embeds flare-server passes its own rule, such as one
  that asks its directory for a group, and a rule whose directory does not
  answer refuses the request with `flare::creators::unavailable` (status
  503) before anything is written. In config, `creators` is a list of
  addresses and `@domain` entries, such as `"ana@example.com"` and
  `"@example.com"`,
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
- `identity` says how people are named: `"email"`, the default, or
  `{"username_claim": "<claim>"}`, which names them by the username in
  that claim of the sign-in's token. Under a username claim, `creators`
  and `names` hold usernames where this page shows addresses, `--with`
  takes a username and refuses an address with 400
  `flare::recipients::not_a_username`, and a caller whose sign-in carries
  no username gets 403 `flare::shares::no_username`. A malformed value
  stops `flare serve` with `flare::config::invalid` naming
  `hosted.identity`. [sign-in.md](sign-in.md) says how each sign-in reads
  the claim.
- `everyone_signed_in`, `false` by default, lets an everyone grant under an
  `anyone` admit rule open the share to every signed-in person. Such a
  grant stores the audience `signed-in` and opens to no one while the
  setting is off; the grant stays until its owner revokes it.
  `everyone_signed_in` without `stores` is refused.
- `teams` says where the share API's teams come from. Left out, or
  `"stored"`, flare keeps them in the table and people manage them with
  `flare teams`. `{"file": "<absolute path>"}` reads a teams document from
  that file, and `{"object": "teams/<name>"}` reads it from that object in
  the stores bucket, which the template's managed policy lets flare read
  and nothing else under `teams/`. The deployment's own sync job writes the
  document, such as an export of its directory, and flare only reads it:
  every team change then answers 403 `flare::teams::managed_elsewhere`.
  `flare serve` reads the document before it listens and exits with
  `flare::teams::document_refused` when it cannot read or use it. A bad
  value stops `flare serve` with `flare::config::invalid` naming
  `hosted.teams`, and `teams` without `stores` is refused. The table
  records whether its team grants are made under stored or directory
  teams, and `flare serve` exits with `flare::config::teams_changed`,
  writing nothing, when `teams` names the other ones while a team grant
  remains, because that grant would open to another group of people.
  [teams.md](teams.md#switching-between-stored-and-directory-teams) says
  how to switch.
  [teams.md](teams.md#teams-from-the-deployments-directory) gives the
  document's shape and limits.
- `get_flare` is the address that the reading page's Get Flare link opens,
  in a new tab, for a reader who has never shared a flare
  (`docs/reading.md`). It must be an `https` URL with a host and no user
  name or password; anything else stops `flare serve` with
  `flare::config::invalid` naming `hosted.get_flare` before it listens.
  Without `get_flare`, the reading page shows no link to anyone.
  `get_flare` without `stores` is refused. flare sends nothing to the
  address and counts no clicks.
- `content_origin` is the https origin of the content host, which serves the
  HTML file of an uploaded HTML share to a frame of the reading page
  ([html.md](html.md)). It must differ from `origin`, and the deployment
  should give it a registrable domain other than the site's, because the
  browser then keeps the uploaded page's storage and cookies apart from
  the site's even if a sandbox is ever misapplied. flare checks only that
  the two origins differ. The deployment supplies a hostname, a TLS
  certificate and a route that sends both hostnames to the same listener.
  Without `content_origin`, an HTML upload is refused with 422
  `flare::html::no_content_host`. `content_origin` without `stores` is
  refused.
- A person who is not a creator gets 403 `flare::shares::not_a_creator`.
- A share granted to everyone signed in opens to the people the `admit`
  rule admitted when the grant was made. Under a `domain` rule the grant
  opens to signed-in addresses at that domain. Under an `emails` rule it
  opens to every signed-in person while the rule stays `emails`, so an
  address added to the list later can open every share granted to everyone
  under that rule. Under an
  `anyone` rule the grant is refused with 409
  `flare::grants::everyone_unbounded`, because it would open the share to
  any Google account, unless `everyone_signed_in` is on. [teams.md](teams.md) gives the life of teams and
  group grants.

Routes:

- `GET /cli/login` sends a signed-in browser back to `flare login` with a
  login code.
- `POST /api/tokens/exchange` trades a login code for a token.
  [tokens.md](tokens.md) gives the life of codes and tokens.
- `DELETE /api/tokens/<id>` revokes the caller's token.
- `POST /api/shares` creates a share, with its grants and the flares it
  responds to.
- `GET /api/shares?scope=mine|with-me&limit=<n>&cursor=<cursor>` lists
  shares, at most 500 in one answer, newest first. When more follow, the
  answer's `next` holds the cursor to pass as `cursor` for the next page.
  With `scope=with-me`, `everyone=true` adds the shares granted to everyone.
- `GET /api/shares/<id>` reads a share.
- `POST /api/shares/<id>/versions` publishes the next version of a share,
  from a `format` and `content`. Only the owner may, and the answer is the
  share summary with the new `version`.
- `POST /api/shares` and `POST /api/shares/<id>/versions` also take a
  `multipart/form-data` body that carries the document's images, as
  [Images](#images) says.
- `GET /api/shares/<id>/versions/<n>/files/<file>` serves one image of
  version `n`.
- `POST /api/grants/<id>` grants a share to more addresses, to teams as
  `team:<name>`, or to everyone signed in with `everyone: true`.
- `DELETE /api/grants/<id>` ends the grants it names.
- `GET /api/grants/<id>` lists a share's grants, with the dormant grants
  of teams being deleted first.
- `POST /api/teams`, `GET /api/teams`, `GET /api/teams/<name>`,
  `PUT /api/teams/<name>/owner`, `DELETE /api/teams/<name>` and `POST` or
  `DELETE` `/api/teams/<name>/members` create, list, show, transfer and
  delete teams and change their members. One `DELETE /api/teams/<name>`
  removes at most 200 of the team's grants and answers `finished: false`
  when more remain; the owner repeats it.
- `GET /api/activity?flare=<id>&since=<cursor>&since_last=<name>&limit=<n>`
  lists the responses and comment events on flares that the caller may
  read, oldest first, at most 100 a page.
- `GET` and `POST /api/comments`, `PATCH` and `DELETE /api/comments/<id>`
  and `GET` and `PUT /api/cursors/<name>` read and write comments and named
  cursors. `docs/comments.md` gives their life.

- Every `/api/` route except the exchange needs a bearer token, and the
  comment routes, activity and the file route also take the session cookie
  of a same-origin browser request. A missing,
  expired or revoked token answers 401 `flare::tokens::sign_in_needed`.
  Under an `emails` admit rule, so does the token of an owner whom the
  list no longer names. Under a `domain` or `anyone` rule a token works
  until it expires or is revoked.
- Every refusal is an RFC 9457 problem document with a `code` such as
  `flare::shares::not_found`.
- A flare the caller may not read answers exactly as a missing one, 404
  `flare::shares::not_found`.
- Activity shows only responses the caller may read. It gives no count of
  the others. It reads a per-reader index that
  [activity.md](activity.md) describes.
- The activity cursor is `fc1.` and a base64url position. A cursor flare did
  not write answers 400 `flare::activity::bad_cursor`.
- The share list cursor is `fs1.` and base64url of the list's scope, the
  last entry's time and its share id. A cursor flare did not write, or one
  of the other scope, answers 400 `flare::shares::bad_cursor`, and two
  `cursor` values answer 400 `flare::requests::bad_query`.

### Images

A share or a version sent with images is a `multipart/form-data` body:

- One part named `share` holds the JSON body that `application/json`
  would carry.
- One part named `image` per image has a `filename` that is the path as
  the Markdown wrote it, such as `img/chart.png`.
- Any other part, a second `share` part or an `image` part with no
  filename answers 400 `flare::requests::bad_body`.

The server first checks that the caller may create or publish and that
the recipients resolve, and answers the share API's own refusals. It then
checks the upload in this order, and writes nothing when a check refuses
it:

| Check | Refusal |
| --- | --- |
| The body is at most 4 MiB (`Limits::upload_bytes`), or 4 MiB and 64 KiB when it holds an `html` part | 413 `request::body_too_large` |
| Each image is at most 5 MiB | 413 `flare::images::too_large` |
| There are at most 10 images | 422 `flare::images::too_many` |
| Each path is relative, with no empty, `.` or `..` segment, no backslash, colon or control character, and at most 255 bytes | 422 `flare::images::bad_path` |
| No path is sent twice | 422 `flare::images::duplicate` |
| The Markdown shows each path | 422 `flare::images::unused` |
| The first bytes name PNG, JPEG, GIF or WebP, whatever the filename says; SVG is refused | 422 `flare::images::format_refused` |
| The container parses | 422 `flare::images::unreadable` |
| The image decodes as its format | 422 `flare::images::not_an_image` |
| The image is at most 16,384 pixels a side and 40 million pixels in all, and decodes within 256 MiB | 422 `flare::images::too_many_pixels` |

- The server strips each image before it stores it. It keeps the
  compressed pixels byte for byte, the ICC profile, animation, and a new
  EXIF block that holds only the orientation. It drops every other EXIF
  field, GPS included, XMP, IPTC, comments, text chunks and any bytes after
  the image's end.
- The server decodes one image at a time. One decode of an image at the
  pixel limit takes up to about 400 MiB, so give `flare serve` at least
  1 GiB of memory, for example a Lambda function's memory size of 1024 MB.
- Each version keeps a table of its images: the path, a random file id,
  the media type and the size. The reading page shows a path the table
  lists from the file route, and shows any other relative image as its
  alternative text.
- `GET /api/shares/<id>/versions/<n>/files/<file>` serves a file only to a
  caller who may open the share, and only when version `n`'s table lists
  the file. A share the caller cannot open, a missing version, an unlisted
  file and a missing file all answer the same 404
  `flare::shares::not_found`.
- The file route's answer has `X-Content-Type-Options: nosniff`,
  `Content-Security-Policy: default-src 'none'; sandbox`,
  `Cache-Control: private, no-store` and `Content-Disposition: inline`.
- The server stores each file before it writes the share or the version.
  When the write is refused, or 5 minutes pass before it starts, the
  server deletes the files it stored before it answers. When the write's
  outcome is unknown, it deletes nothing.
- A delete that fails logs one warning with the share id and the file ids,
  and counts the metric `FilesCleanupFailed`. The answer is the upload's
  own refusal.
- A site whose `Sharing` has no file store answers an upload with an
  image with 503 `flare::shares::unavailable`, and writes nothing.
- A hosted `flare serve` keeps images in the bucket that `hosted.stores`
  names, under `files/<share id>/<file id>`.
- `flare files sweep --yes` deletes the files in that bucket that no
  version lists and that were stored at least 30 minutes ago. It reads the
  same config file as `flare serve`, takes `--config` as `flare serve`
  does, and needs credentials that may read the table and list, read and
  delete in the bucket. It examines at most 10,000 files in one run,
  starting at a random share and wrapping to the start of the list, so
  hourly runs reach every file of a larger bucket over time. It prints how
  many it examined, deleted and left undecided, and whether it stopped
  before it examined every file. `flare serve` runs no sweep, so the deployment schedules
  this command, for example hourly with a cron job, a systemd timer or a
  scheduled task: `flare files sweep --config /etc/flare/config.json --yes`.

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

- flare sets only `__Host-` cookies, which are bound to its exact host. A
  site that signs people in through a proxy reads the proxy's cookie, whose
  scope the proxy sets.
- Serve flare on a name whose parent domain has no untrusted subdomains.
  A sibling host can set many cookies on the parent domain and push every
  request past the header size limit.
- A reading page with code or a diagram carries the script policy, whose
  `Content-Security-Policy` has `script-src 'self'` and no `connect-src`,
  so its scripts can make no request. Every other hosted reply's policy
  has `connect-src 'self'`, which limits the requests a page makes to this
  site's origin, and those pages run no script.
- Every reply carries `Strict-Transport-Security: max-age=63072000`, so a
  browser that has visited uses HTTPS for two years.
