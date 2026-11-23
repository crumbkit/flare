# flare-aws

`flare-aws` keeps flare's shares, grants, login codes, CLI tokens and
response links in one DynamoDB table and its files in one S3 bucket.
`DynamoShareStore`, `DynamoGrantStore`, `S3FileStore`, `DynamoTokenStore`
and `DynamoResponseStore` implement flare-core's `ShareStore`,
`GrantStore`, `FileStore`, `TokenStore` and `ResponseStore`, and each runs every function of the matching contract suite in
`flare_core::contract` from this crate's tests.

## Configuration

`HostedConfig` holds three keys, each a string:

| Key | What it names | Rule |
| --- | --- | --- |
| `region` | The AWS region of the table and the bucket. | 1 to 32 lower-case letters, digits and hyphens, starting and ending with a letter or digit. |
| `table` | The DynamoDB table. | 3 to 255 bytes of letters, digits, `_`, `-` and `.`. |
| `bucket` | The S3 bucket. | 3 to 63 lower-case letters, digits and hyphens, starting and ending with a letter or digit. |

The config refuses any other key, and refuses a key that names a credential
with a message that names the AWS provider chain. Credentials come only from
the provider chain: environment variables, the shared config files, or the
role of the host. `Stores::connect` builds both clients, and
`Stores::check` confirms that the table has the expected keys and that the
bucket exists.

`infra/flare-stores.template.json` is a CloudFormation template that
defines the table, the private bucket, its bucket policy and the IAM policy
that allows exactly the calls the stores make. Its test,
`tests/template.rs`, pins each of those properties.

## What it stores

The table has a string partition key `pk` and a string sort key `sk`, and
no index.

| `pk` | `sk` | What it holds |
| --- | --- | --- |
| `SHARE#<id>` | `META` | The share: owner, kind, title, creation time and `latest`. |
| `SHARE#<id>` | `V#<10-digit number>` | One version: kind, text, publisher and time. |
| `SHARE#<id>` | `G#<grantee>` | One grant: role, granter, creation time and `rev`, a random 128-bit value. |
| `SHARE#<id>` | `N#grants` | The share's count of grants. |
| `OWN#<owner>` | `N#shares` | The owner's count of shares. |
| `OWN#<owner>` | `E#<20-digit inverted time>#<id>` | The owner's entry for one share, newest first. |
| `TO#<grantee>` | `E#<20-digit inverted time>#<id>` | The grantee's entry for one shared share, with the role and the granter. |
| `CODE#<64-hex digest>` | `CODE` | One login code, by the SHA-256 of its text: owner, CLI port, expiry, the expiry again in `expires_at`, and `used_by` once used. |
| `TOKEN#<id>` | `TOKEN` | One CLI token: the SHA-256 of its secret as 32 bytes, owner, label, creation time, expiry and, once revoked, the revoked time. |
| `LABELS#<owner>` | `L#<label>` | The id of the owner's latest token with that label. |
| `SEQ#responses` | `SEQ` | The head: the highest position given to a response link. |
| `RESP#<response>` | `F#<flare>` | A response's link to one flare: version, time and position. |
| `RE#<flare>` | `P#<20-digit position>` | The same link under the flare, in position order. |

The table's TTL reads `expires_at`, which only login codes carry, so
DynamoDB deletes a code some time after it expires. A read treats an
expired code as refused before that. Tokens stay after they expire or are
revoked, so a check of one answers that it no longer signs in. The table
never holds a raw code or token.

Each file is the object `files/<share>/<file>` in the bucket, with its media
type as the object's content type.

Every write is one `TransactWriteItems` call, which DynamoDB applies whole
or not at all. A grant or a revoke by an admin carries a condition on the
admin's own grant item in the same transaction, so it lands only while that
admin still holds the Admin role. Every read is consistent, so it sees every
write that finished before it.

## Lifecycle

The stores keep no state of their own, so a server that crashes loses
nothing that a transaction or a put had finished.

### A share's items

| State | Event | Next state | Owner of the event |
| --- | --- | --- | --- |
| no items | `create` with a free id | share, version 1 and owner entry exist together | the caller |
| no items | `create` with a taken id | the first share is unchanged, `IdTaken` | the caller |
| `latest = n` | `publish` | `latest = n + 1`, version `n + 1` and the owner entry's `latest` change together | the owner |
| `latest = n` | `publish` meets a conflict or a moved `latest` | nothing written, read again and retried | the owner |
| `latest = n` | `publish` with `n` at 100 | nothing written, `TooManyVersions` | the owner |
| no items | `create` for an owner with 500 shares | nothing written, `TooManyShares` | the caller |
| any | a write whose answer is lost, or whose deadline ends | the write may be applied; the store resends with the same token and returns a refusal of the resend as it is, because DynamoDB answers a token whose write was applied with that success; at the deadline it settles by a consistent read: a match is success, otherwise `Unavailable`, which means the outcome is unknown | the store |
| a version item | any write | none: version items are put once and never updated | nobody |

### A grant's items

| State | Event | Next state | Owner of the event |
| --- | --- | --- | --- |
| none | `grant` with `expected` none | grant item, grantee entry and count + 1 exist together | the owner, or an admin |
| a role | `grant` or `revoke` from the current role | grant item and entry change together; a revoke subtracts 1 from the count | the owner, an admin or the person |
| any | the `rev`, the admin's grant or the count fails its condition | nothing written; an admin who lost the Admin role is `StaleAuthority`, then a differing role is `Changed`, then a full share is `TooMany`; the same role with another `rev` is read again and retried | the writer |
| any | a transaction conflict | retried with a fresh token, at most five times, then `Unavailable` | the store |
| any | an unknown outcome of a grant or revoke | settled by the grant item's `rev`; the revoke of an owner or of the grantee accepts an absent item, and the revoke of another admin answers `Unavailable` for an absent item, because another write may have removed it after that admin lost the Admin role | the store |

### A login code, a token and a response link

`docs/tokens.md` gives the states of a login code and a token. In the
table:

| State | Event | Next state | Owner of the event |
| --- | --- | --- | --- |
| no code | `issue_code` | the code item exists, unused | the server, for a signed-in browser |
| unused, before its expiry | `use_code` | `used_by` holds a random mark, set only while absent and before the expiry; two uses together give one success | the exchange |
| used or expired | `use_code` | nothing written, `CodeRefused` | the exchange |
| a slot naming token A, or none | `create` of token B | B's item exists, the slot names B, and A, when still unrevoked, has B's creation time as its revoked time, all in one transaction | the exchange |
| active | `revoke` by its owner | the revoked time is set, only while absent, so a later revoke keeps the first time | the owner |
| head `n` | `link` of a new pair | the counter is `n + 1` and both link items exist together; a link that meets another link's counter move reads again | the share or grant |
| a pair exists | `link` of the same pair | nothing written; the first link is answered | the share or grant |

A lost answer is settled like the others: an issue by the code's owner,
port and expiry, a use by its `used_by` mark, a create by the token's
digest, a revoke by the revoked time, and a link by the pair item, whose
first link a repeat answers anyway.

### A file

| State | Event | Next state | Owner of the event |
| --- | --- | --- | --- |
| no object | `put` | the object exists | the caller |
| an object | `put` with the same key | the first object is unchanged, `IdTaken` | the caller |
| no object | `put` that meets another put of the same key in flight (409) | retried, and the retry meets the other put's object | the store |
| no object | `put` whose answer is lost, or that the SDK resends after a 5xx or a timed-out attempt | resent; every 412 is settled by reading the object: success when its bytes and media type equal the file's, `IdTaken` when they differ, `Unavailable` when the read fails | the store |
| an object | any other write | none: no replace and no delete is permitted | nobody |

### Crash points

- A crash before a transaction commits leaves the old state. After it
  commits, the share, grant, count and entries are all new, because
  DynamoDB applies a transaction whole.
- A transaction can commit and lose its answer. The store resends with one
  token while the parameters are identical. It waits 6 seconds after an
  unknown answer before the first resend and twice as long before each
  later one, up to four times the first wait, and stops at a 20-second
  deadline. It then settles an
  unknown outcome by a consistent read, and answers `Unavailable` only when
  it cannot tell. `Unavailable` from a write therefore means the outcome is
  unknown, and a late commit within the token's 10 minutes is possible. A
  caller that retries `create` and gets `IdTaken` draws a new id, so one
  unused share can stay in the owner's list.
- `use_code` commits before the token is written. A crash between the two
  spends the code with no token, and the person runs `flare login` again.
- A link commits after the response share exists. A crash between the two
  leaves a response that answers no flare, which its owner can link with
  `flare grant <id> --re <flare>`.
- Files go to S3 before the share is created. A crash between the two
  leaves files under an id with no share. No request can reach them,
  because every read of a file starts from a share. Nothing removes them.

### Invariants

- A share's version numbers run from 1 to `latest` with no gap, and no
  version changes.
- The grant count equals the number of grant items of the share, because
  the count changes in the transaction that changes the grants.
- Every grant item has one grantee entry with the same role, and every
  owner entry matches its share's title, creation time and `latest`.
- flare never replaces or deletes a stored object.
- The counter equals the highest link position, and every position up to
  it belongs to exactly one link, because the counter moves in the
  transaction that writes the link.
- A label slot names the only unrevoked token of its owner and label that
  `create` wrote.

## Testing

The tests run against two local stand-ins: DynamoDB Local and SeaweedFS's
S3 server. Neither enforces IAM or a bucket policy, and DynamoDB Local runs
transactions one after another, so it never answers with a transaction
conflict. The template's test pins the policies instead.

Start both stand-ins on free ports. SeaweedFS takes the made-up keys that
the tests sign with; no test uses a real AWS account or real credentials.
DynamoDB Local's licence does not allow republishing its image, so pull it
from its own registry.

```sh
docker run -d --rm --name flare-dynamodb -p 127.0.0.1:8000:8000 \
  public.ecr.aws/aws-dynamodb-local/aws-dynamodb-local@sha256:6ada631ec7aa5fc83446855eb17233e3dad184fec47477e8140c6c569cfe0c86 \
  -jar DynamoDBLocal.jar -inMemory -sharedDb
docker run -d --rm --name flare-s3 -p 127.0.0.1:8333:8333 \
  -e AWS_ACCESS_KEY_ID=flaretest -e AWS_SECRET_ACCESS_KEY=flaretestsecret \
  chrislusf/seaweedfs@sha256:4b42c212543a0acd4c0e675215ecdcaf62a186ef98def76c7e12701871222d15
```

Then run the tests with each stand-in's URL. A test fails with the name of
the variable when one is unset. Each test creates its own table and bucket,
so tests can share the stand-ins.

```sh
FLARE_TEST_DYNAMODB=http://127.0.0.1:8000 FLARE_TEST_S3=http://127.0.0.1:8333 \
  cargo nextest run -p flare-aws --all-features
```

| Tests | What they check |
| --- | --- |
| `tests/contract_dynamodb.rs`, `tests/contract_s3.rs` | Every function of the five contract suites, a second store on the same table, a code's TTL attribute, and objects that flare did not write. |
| `src/dynamodb/grants/tests.rs` | An admin's grant or revoke built before the admin is revoked is refused when sent, because the admin's check and the write are one transaction. |
| `src/dynamodb/transact/tests.rs` | Each grant and revoke transaction holds the admin's condition beside the write. |
| `tests/outcome_unknown.rs` | A write whose answers are all lost is resent with one token and settled by a read; a put that the SDK resends after S3 stored it answers success, and onto another object answers `IdTaken`; a lost revoke by an admin whose Admin role ends meanwhile answers `Unavailable`; lost and dropped code, token and link writes answer success and `Unavailable`. |
| `src/dynamodb/retry/tests.rs` | A refusal of a resend is returned without a settling read, and a send that never answers is resent a few times with growing waits until the deadline. |
| `tests/template.rs` | The template's table, bucket, bucket policy and IAM actions. |
| `tests/log.rs` | No email, title, text or id appears in the crate's logs. |

The `hosted` job in `.github/workflows/ci.yml` starts the same images and
runs these tests and `cfn-lint` over the template.
