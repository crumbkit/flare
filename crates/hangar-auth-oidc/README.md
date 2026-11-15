# hangar-auth-oidc

hangar-auth-oidc signs people in to hangar with an OpenID Connect provider,
such as Google, and implements `hangar_core::Authenticator` for the sessions
it creates. It holds no HTTP routes, cookies or pages. The server that hosts
it serves two routes: one calls `start` and redirects to the provider, and
the callback calls `complete`.

The crate docs in `src/lib.rs` give the order of the ID token checks, the
limits, and the state tables of pending sign-ins, sessions and the key set.

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
   `hangar-oidc-google`. The command below asks for the secret, so the secret
   never appears on a command line:

   ```sh
   security add-generic-password -s hangar-oidc-google -a hangar -w
   ```

2. Start the example with the client ID and the email that may sign in:

   ```sh
   cargo run -p hangar-auth-oidc --example google_sign_in -- <client id> <email>
   ```

   The example listens on 127.0.0.1:8740 and [::1]:8740 and prints nothing.
   When it cannot start, it exits with the reason.

3. Open `http://localhost:8740/auth/oidc/start` and sign in with the allowed
   account. The page names the principal `owner`.

4. Start the example again with an email other than the account's, and sign
   in again. The page says `NotAllowed`.
