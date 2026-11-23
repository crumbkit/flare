# flare-auth-oidc

flare-auth-oidc signs people in to flare with an OpenID Connect provider,
such as Google, and implements `flare_core::Authenticator` for the sessions
it creates. It holds no HTTP routes, cookies or pages. The server that hosts
it serves two routes: one calls `start` and redirects to the provider, and
the callback calls `complete`.

The crate docs in `src/lib.rs` give the order of the ID token checks, the
limits, and the state tables of pending sign-ins, sessions and the key set.

## Who may sign in

`OidcConfig` takes an `Admit` rule. `Admit::Emails` admits the verified
emails on an allow-list, and each entry names its principal and what it may
reach. `Admit::Domain` admits every verified account of one Google
Workspace domain, and each signs in as its email in lower case with the
rule's reach. Only the `hd` claim of the signed ID token decides membership:
an email that ends in the domain is not enough, and a personal Google
account has no `hd`, so it is refused as `NoHostedDomain`. An account of
another organisation is refused as `OtherHostedDomain`.

`hd` is a Google claim. The crate does not check that the issuer is Google,
so with another issuer `Admit::Domain` trusts that issuer's `hd`. Use it
with the issuer `https://accounts.google.com`. Under `Admit::Domain`,
`authorize` allows a principal only while one of its sessions is live.

## Pending sign-ins and sessions

`start` seals the pending sign-in into the value the server keeps in a
cookie, with AES-256-GCM under a numbered key from the key store and a
random 12-byte nonce, so the crate stores nothing for a sign-in that has not
finished and no number of starts evicts another. Keys rotate after 30 days
and stay readable for an hour. A kept value opens for 10 minutes. A
second callback with the same value sends the code to the provider again,
and the provider refuses the used code as `CodeRejected`.

`complete` takes the session the browser already holds, if any, and ends it
in the step that creates the new one, after every check has passed. One
principal holds at most 8 live sessions. A session expires 12 hours after the
last request that extended it and never later than 7 days after sign-in.
`connect` keeps sessions and keys in memory, where a restart signs everyone
out. `connect_stored` takes stores that several processes share, such as the
DynamoDB stores of `flare-aws`.

## The client secret

`KeychainSecret` reads the client secret from a macOS Keychain item.
`FileSecret` reads it from a file, such as a container's secret mount, on a
host that has no Keychain. It follows symbolic links and checks the open
file: a regular file, owned by the user the process runs as or by root, not
writable by its group or by others, and 1 to 4,096 bytes long. It trims one
trailing newline. A world-readable file is accepted, because container
runtimes mount secrets 0444 or 0644 by default; mode 0400 is the
recommended mode.

## Tests

The tests run against a fake provider inside the test process
(`tests/support/fake_provider.rs`), which serves discovery, token and key
set endpoints on 127.0.0.1 with a new RSA key for each test. No test calls a
real provider. `KeychainSecret` is tested on any Unix system against a fake
`security` program.

## The live Google check

This is a manual procedure, and no CI job runs it. It needs a Google OAuth
client of the type "Web application" whose authorised redirect URI is
`http://localhost:8740/auth/oidc/callback`, and its client secret in the
macOS Keychain.

1. Store the client secret in the Keychain item of the service
   `flare-oidc-google`. The command below asks for the secret, so the secret
   never appears on a command line:

   ```sh
   security add-generic-password -s flare-oidc-google -a flare -w
   ```

2. Start the example with the client ID and the email that may sign in:

   ```sh
   cargo run -p flare-auth-oidc --example google_sign_in -- <client id> <email>
   ```

   The example listens on 127.0.0.1:8740 and [::1]:8740 and prints nothing.
   When it cannot start, it exits with the reason.

3. Open `http://localhost:8740/auth/oidc/start` and sign in with the allowed
   account. The page names the principal `owner`.

4. Start the example again with an email other than the account's, and sign
   in again. The page says `NotAllowed`.

5. This step is optional, and needs an account of a Google Workspace
   organisation. Start the example with the organisation's domain in place
   of an email:

   ```sh
   cargo run -p flare-auth-oidc --example google_sign_in -- <client id> --domain <domain>
   ```

   Sign in with an account of the organisation. The page names the
   account's email as the principal. Then sign in with a personal Google
   account. The page says `NoHostedDomain`.
