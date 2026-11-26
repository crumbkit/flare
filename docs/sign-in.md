# Signing in through a single sign-on proxy

A hosted flare can sign people in with the signed token that a single
sign-on proxy in front of it sets as a cookie, in place of flare's own
OpenID Connect sign-in. The `flare` command can then take its token from a
helper program that the deployment provides. This page says how to set up
both.

## Which proxies work

- The proxy must set a signed JSON Web Token (a JWT) as a cookie on every
  request it forwards.
- The token must be signed with RS256 or ES256, by a key that the proxy
  publishes as a JSON Web Key Set (a JWKS) at an `https` address.
- The token must carry `iss`, `aud`, `exp` and a claim that holds the
  person's email address, `email` by default.
- flare never reads an unsigned identity header such as `X-Forwarded-Email`,
  `X-Forwarded-User` or `Remote-User`. A proxy that only sets such headers
  cannot be used, because anything that reaches `hosted.listen` could set
  them.
- The proxy must not add an `Authorization` header to page requests. flare
  answers a page request that carries one with 401 `auth::bad_credential`.

## The config

A hosted config takes exactly one of an `oidc` section and a `proxy`
section. `flare serve` exits with `flare::config::invalid` when it has both
or neither.

```json
{
  "hosted": { "origin": "https://flare.example.com", "listen": "127.0.0.1:8080" },
  "proxy": {
    "issuer": "https://sso.example.com",
    "audience": "flare",
    "jwks_url": "https://sso.example.com/.well-known/jwks.json",
    "cookie": "sso_token",
    "algorithms": ["RS256"],
    "admit": { "domain": "example.com", "reach": "everything" },
    "sign_in_url": "https://sso.example.com/sign-in",
    "sign_out_url": "https://sso.example.com/sign-out"
  }
}
```

- `issuer` must equal the token's `iss` exactly.
- `audience` must be one of the token's `aud` values. Give flare its own
  audience at the proxy, so a token for another application behind the same
  proxy is refused.
- `jwks_url` is the proxy's key set. It must be `https`.
- `cookie` is the name of the cookie that carries the token.
- `algorithms` lists `RS256`, `ES256` or both. Nothing else is accepted.
- `identity_claim` names the claim that holds the email address. It is
  `email` when left out.
- `admit` is the same rule that the `oidc` section takes. The hosted
  section's share API follows it too.
- `sign_in_url`, optional, is linked from the 401 page that a reader with no
  cookie gets, and `flare login` sends the browser there when it has no
  cookie.
- `sign_out_url`, optional, is linked from the 403 page that a person the
  admit rule refuses gets, so they can sign in as someone else.
- The `/auth/oidc/*` routes answer 404 on a proxy site.

## How a token is checked

flare accepts a token only when every check passes, in this order:

1. The token is at most 8 KiB.
2. Its header's `alg` is one that `algorithms` lists, and its `kid` names a
   key in the key set.
3. The signature verifies with that key.
4. `iss` equals `issuer`, and `aud` contains `audience`.
5. `exp` has not passed, and `nbf` and `iat`, when present, are not in the
   future, each with 60 seconds of leeway.
6. The identity claim holds an email address, and `email_verified`, when
   present, is true.
7. The admit rule admits the address.

- A token that fails a check from 1 to 6 gets 401 `auth::proxy_refused`,
  with no redirect and no sign-in link, so a misconfigured check cannot
  send the browser round in a loop. The log names the check.
- A token that fails check 7 gets 403 `auth::not_admitted`.
- A request with no cookie gets 401 `auth::sign_in_needed`.

## The key set

- `flare serve` fetches the key set before it binds. When it cannot, when
  the set holds no RS256 or ES256 key, or when it holds more than 20 keys,
  usable or not, it exits with `flare::proxy::keys_unavailable`, which
  names `proxy.jwks_url`.
- flare fetches the set again after one hour, and at most once a minute.
- A token whose `kid` is not in the held set makes flare fetch the set
  again. A key that is still missing gets 401 `auth::proxy_refused`.
- When a fetch fails, or returns a set that flare would refuse at start,
  flare keeps the keys it holds, and tokens signed by them keep working.
  A token whose `kid` is not held gets 503 `auth::unavailable` until a
  fetch succeeds.

## The share API

- The share API accepts the proxy's token as an `Authorization: Bearer`
  header, as well as flare's own CLI tokens.
- A script on the site's own pages reads the comment and activity routes
  with the proxy's cookie, with the same same-origin checks as flare's own
  session cookie.
- `flare login <server>` still works: `/cli/login` issues a login code to
  any browser that carries the proxy's cookie.

## The token command

A deployment can give the `flare` command its token from a helper program,
so a person signs in once through the proxy and needs no `flare login`. The
helper is named in the `client` section of flare's config file,
`~/.config/flare/config.json`:

```json
{
  "client": {
    "server": "https://flare.example.com",
    "token_command": ["/usr/local/bin/sso-token", "--audience", "flare"],
    "token_deadline_seconds": 20
  }
}
```

- `server` is the hosted flare's origin.
- `token_command` is the program and its arguments, run without a shell.
- `token_deadline_seconds` is from 1 to 120, and 20 when left out.
- Every share command runs the helper once, before its first request, with
  no standard input and `FLARE_SERVER` set to `server`. It sends the first
  line of the helper's standard output as the bearer.
- The helper's standard error goes to the terminal, so it can tell the
  person to sign in.
- The command keeps the token in memory for one command. It never writes the
  token to a file, the Keychain or a log, and it reads neither the Keychain
  nor `login.json`.
- `flare logout` revokes nothing and sends no request, and its `revoked`
  field is false, unless a login saved earlier with `flare login` exists.
  It then revokes that login's token as usual.

| Code | When |
| --- | --- |
| `flare::login::token_command_failed` | The helper did not start, or exited with a status other than 0. The message names the program and the status. |
| `flare::login::token_command_timeout` | The helper ran past its deadline. flare stops it within one second of the deadline. |
| `flare::login::token_command_output` | The output was empty, over 16 KiB, or its first line was not one token of visible ASCII characters. The output is never printed. |
| `flare::tokens::sign_in_needed` | The server refused the token. The next step names the helper. |

## Checking a deployment

The tests build tokens with the claims above. Check a real proxy once after
it is set up:

1. Start `flare serve`. It fetches the key set, so a start that binds shows
   that `jwks_url` works.
2. Open `<origin>/` in a browser that has signed in through the proxy. The
   index of pages shows that the cookie, issuer, audience and identity claim
   are right.
3. Open it in a private window with no cookie. The 401 page should link to
   `sign_in_url`.
4. From the host, send a request straight to `hosted.listen` that carries
   only an identity header. It must answer 401:

   ```bash
   curl -i -H 'Host: flare.example.com' -H 'X-Forwarded-Email: someone@example.com' http://127.0.0.1:8080/
   ```
5. With the `client` section set, run `flare shares`. It lists the shares
   the person can see.
