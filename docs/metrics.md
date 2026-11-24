# Metrics

A flare site can count the shares it created, the pages it served, the
people who signed in and the requests it refused. Each counted event is one
line on the server's log on standard error, in CloudWatch Embedded Metric
Format (EMF). flare keeps no counter in memory and writes no record for a
count.

The config's `log.metrics` setting turns the lines on:

```json
{ "log": { "metrics": true } }
```

`log.metrics` is off when the config does not set it, so a local
`flare serve` writes no metric line unless its config asks for them. The
hosted config in [hosting.md](hosting.md) sets it to `true`. When it is
off, the log holds only the lines it held before metrics existed, and the
`served` line of each request is unchanged when it is on. A misspelt key
in the `log` section stops `flare serve` with `flare::config::invalid`
before it binds.

## What a count means

- A page view is one page response: a `GET /pages/<name>` that answered
  200. One person who reloads a page counts twice. flare has no count of
  unique viewers, and no metric says who viewed a page or which page it
  was.
- A sign-in is one OpenID Connect callback that signed a browser in. One
  person who signs in on two browsers counts twice.
- Opening a share on the hosted site is not a page view. `flare show` and
  other readers open a share with `GET /api/shares/<name>`, which shows as
  `Requests` with the `Route` `/api/shares/<name>` and the `StatusClass`
  `2xx`. That count includes the owner's own reads of their shares and
  every reread, so it is the number of share reads and says nothing about
  how many people read a share.
- An exchange that fails because the token store is unavailable answers
  503 and counts no refused token; it shows only as `Requests` with the
  `StatusClass` `5xx`.
- Every metric is a count of events. No metric holds a person, an email
  address, a token, a session, an IP address, a share id, a share title or
  a page name, in a value or in a dimension. The `Route` dimension holds
  the route's pattern from the route table, such as `/pages/<name>`, and
  never the path a browser asked for.

## No route serves the counts

No route of a flare site serves the counts, and there is no `/metrics`
URL. A person who signs in to the site cannot read them. On a hosted site
on AWS, the operator reads them in CloudWatch, and anyone the operator
gives CloudWatch read access in the account can read them too. On a local
`flare serve`, the person who runs it reads them with `flare metrics`.

A `/metrics` route was left out for three reasons. On Lambda, a request
reaches one instance, which holds only the counts of the requests it
served and is gone later. The hosted site's route table lists every route
it serves, and a new open route would show the site's traffic to anyone.
A route that needs a sign-in would show it to every colleague who can
sign in.

## The metrics

Every metric is in the CloudWatch namespace `Flare`, has the unit `Count`,
and has the value 1 on each line.

| Metric | Dimensions | What it counts |
| --- | --- | --- |
| `Requests` | `Route`, `StatusClass` | Every request the server answered, refusals included. |
| `SharesCreated` | none | Each `POST /api/shares` that created a share. |
| `PagesViewed` | none | Each `GET /pages/<name>` that answered 200. |
| `SignIns` | none | Each OpenID Connect callback that signed a browser in. |
| `SignInsRefused` | `Reason` | Each OpenID Connect callback that refused a sign-in. |
| `TokensExchanged` | none | Each login code that `POST /api/tokens/exchange` traded for a token. |
| `TokensRefused` | none | Each exchange that answered 4xx, and each other `/api/` request answered 401, whether or not it carried a bearer token. |

### Dimensions

- `Route` is the pattern of the route that answered, such as `/flare/`,
  `/pages/<name>` or `/api/shares/<name>`. It is `oidc` for the local
  sign-in host, and `-` when no route answered, such as a 404 for an
  unknown path, a refused Host header or a busy server. These are the
  values of `route` in the `served` log line.
- `StatusClass` is the class of the answer's status: `2xx`, `3xx`, `4xx`
  or `5xx`.
- `Reason` is why a sign-in was refused, one of:

| Reason | What happened |
| --- | --- |
| `malformed` | The callback's query was malformed. |
| `no_kept_state` | The browser sent no sign-in state. |
| `wrong_state` | The callback's state differed from the browser's. |
| `unknown_state` | The sign-in was unknown or expired. |
| `provider_denied` | The provider refused the sign-in. |
| `code_rejected` | The provider refused the authorization code. |
| `provider_unavailable` | The provider did not answer in time. |
| `provider_misbehaved` | The provider sent a reply the protocol does not allow. |
| `token_rejected` | The ID token failed a check. |
| `email_unverified` | The account's email was missing or not verified. |
| `not_allowed` | The admit rule does not admit the account. |
| `too_many_sessions` | Too many sessions were live. |
| `no_randomness` | The system's random number generator failed. |
| `cannot_seal` | The sign-in state could not be sealed. |
| `no_hosted_domain` | The account belongs to no Google Workspace organisation. |
| `other_hosted_domain` | The account belongs to another Google Workspace organisation. |
| `no_handover_slot` | The local server's table of hand-over codes was full. |
| `other` | A refusal that a newer flare-auth-oidc added and this list does not name. |

A renamed metric or dimension breaks the dashboards and alarms that name
it, so a rename is a breaking change and gets a changelog entry.

## One line

A page view writes this line, on one line:

```json
{"PagesViewed":1,"_aws":{"CloudWatchMetrics":[{"Dimensions":[[]],"Metrics":[{"Name":"PagesViewed","Unit":"Count"}],"Namespace":"Flare"}],"Timestamp":1795424400000}}
```

A request writes its dimensions beside the count:

```json
{"Requests":1,"Route":"/pages/<name>","StatusClass":"2xx","_aws":{"CloudWatchMetrics":[{"Dimensions":[["Route","StatusClass"]],"Metrics":[{"Name":"Requests","Unit":"Count"}],"Namespace":"Flare"}],"Timestamp":1795424400000}}
```

`Timestamp` is the server's wall-clock time in milliseconds since the Unix
epoch. The keys are in byte order, which CloudWatch does not mind. A metric line is the bare JSON object, with no time or level before
it, because CloudWatch reads only a log event that is one JSON object.

## Reading the counts on AWS

On Lambda, the function's log group receives every line. CloudWatch reads
the EMF lines and turns them into metrics in the `Flare` namespace, with
no code in flare, no extra permission and no write to DynamoDB. Every
instance writes its own lines and CloudWatch adds them up, so the number
of instances does not change a count.

- The log group keeps the lines for its retention, which the hosted
  deployment sets to 90 days.
- CloudWatch keeps the extracted metrics for up to 15 months: one-minute
  points for 15 days, then coarser points.
- Turning `log.metrics` off stops new lines. The lines and metrics already
  kept stay until their retention ends.

## Reading the counts locally

A local `flare serve` writes the lines to standard error. flare keeps
nothing; the lines are wherever the operator sends standard error. Keep
them in a file and total them with `flare metrics`:

```sh
flare serve 2> flare.log
flare metrics flare.log
flare metrics flare.log.1 flare.log --json
flare serve 2>&1 | tee flare.log
cat flare.log | flare metrics
```

`flare metrics` prints the count of each metric on each day in UTC, then
the total of each metric. It reads every line of every file it is given,
or standard input without a file, and skips each line that is not a Flare
EMF line, such as the `served` lines, a truncated line or a line of
another namespace. It reports how many lines it read and how many it
skipped. When a log file was rotated, pass every file that covers the
period.

The totals of `flare metrics` and of CloudWatch come from the same lines,
but CloudWatch buckets them by minute and by the time zone the console
shows, and `flare metrics` buckets them by day in UTC.

## What the counts do not show

- A count does not show who made a request, which page they read or which
  share they created.
- A lost line is not detected. A line that standard error could not take,
  such as when it is closed, is lost, and the request still answers as it
  would have.
- No test calls AWS. The tests check each line against the EMF fields
  that CloudWatch documents, and the first deployment of a hosted site is
  the check that CloudWatch extracts them.
