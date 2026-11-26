# Login codes and CLI tokens

`flare login <server>` signs a person in to a hosted flare in the browser and
gives the command a token. The command sends the token as a bearer header on
every share API request. This page gives the life of a login code and of a
token. A config whose `client` section names a token command gives the
command its token from that program instead, and saves nothing;
[sign-in.md](sign-in.md) describes it.

## How a login runs

1. The command picks the label and records it in
   `<state home>/flare/labels.json` under the server's origin.
2. The command listens on a free port of 127.0.0.1 and opens
   `<server>/cli/login?port=<port>&state=<random>` in the browser.
3. The browser signs in with Google. The server issues a login code and
   sends the browser to `http://127.0.0.1:<port>/callback` with the code and
   the state.
4. The command checks the state and posts the code and a label to
   `POST /api/tokens/exchange`. The server marks the code used and answers
   a new token.
5. The command saves the token in the Keychain, as the generic password of
   the service `flare-cli` and the account that is the server's origin. It
   writes the server, the token's id, the label and the owner to
   `<state home>/flare/login.json`, with mode 0600.

- The command waits 300 seconds for the browser, then fails with
  `flare::login::failed`.
- The label is `--label`, else the label of the last login to that server
  in `labels.json`, else `flare-cli-<8 hex>`.
- The server revokes an owner's older token with the same label when it
  issues a new one. The label is recorded before the exchange, so the next
  login also revokes a token that the command never received or never
  saved.
- The token is written to the Keychain program's standard input, never on
  a command line.

## Login code

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| issued | A signed-in browser finishes the `flare login` redirect | used, expired | SHA-256 hash, owner address, CLI loopback port, expiry 60 seconds after issue |
| used | The CLI exchanges the code at `POST /api/tokens/exchange` | none | The used mark, kept until the expiry passes, then deleted |
| expired | 60 seconds pass with no exchange | none | Treated as absent on read; DynamoDB's time-to-live deletes the item |

- A code item carries `expires_at`, the attribute the table's time-to-live
  setting reads. DynamoDB deletes an expired item some time after its
  expiry, usually within a few days. Every read compares the expiry with the
  clock, so the delay changes nothing a person sees.

## Token

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| active | A code exchange succeeds | revoked, expired | SHA-256 hash, owner address, label, created time, expiry 90 days after creation |
| revoked | `flare logout`, `DELETE /api/tokens/{id}` by its owner, or a new token for the same owner with the same label | none | Revoked time; the hash stays so a revoked token answers 401 |
| expired | The expiry passes | none | Nothing changes; every check compares the expiry with the clock |

- A request with an expired, revoked or unknown token answers 401
  `flare::tokens::sign_in_needed`. The command prints `flare login <server>`
  as the next step.
- An active token authenticates only while the server's `oidc.admit` rule
  admits its owner's address, and the server checks this on every request.
  Under an `emails` rule, once the server runs with the owner's address
  taken off the list, every token of that owner answers 401
  `flare::tokens::sign_in_needed`, and the stored token does not change.
  If the address goes back on the list before the token expires, the token
  works again.
- Under a `domain` or `anyone` rule the server cannot tell from an address
  whether its account still belongs to the domain, so a token keeps working
  until it expires or its owner revokes it.
- `flare logout --yes` revokes the token, deletes the Keychain item and
  forgets `login.json`. A token the server no longer knows still counts as
  logged out. With a token command it revokes nothing, sends no request
  and prints `revoked: false`.

## Crash points

1. The server crashes after it marks the code used and before it writes the
   token. The code is spent and no token exists. The CLI gets an error and
   the person runs `flare login` again.
2. The server writes the token and the response is lost. The token is
   active and unknown to the CLI. The next `flare login` with the same label
   revokes it.
3. The Keychain refuses the save. The token is active and unused. The next
   `flare login` revokes it.
4. Two exchanges of one code race. The store marks the code used with a
   conditional write, so exactly one exchange wins and the other gets
   `flare::tokens::code_refused`.

## Invariants

- The server never stores a raw code or token, and never logs one.
- A code is marked used before the token it buys is written.
- An expired or revoked token never authenticates, nor does a token whose
  owner the admit rule no longer admits, and the check runs on every
  request.
- A token authenticates as its owner's verified address and no other.
- Login codes and tokens live in the DynamoDB table, so they survive a
  restart of the server, unlike browser sessions.
