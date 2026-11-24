# Flare activity index entries

`flare activity` and `GET /api/activity` read a per-reader activity index.
An index entry records that one reader can open both a response and a flare
it responds to, so the activity call lists that response to that reader.
The call reads only the caller's own entries, so it never reads a response
someone else hid from the caller, and its cost does not grow with the
responses the caller cannot open. Each entry has a marker that names its
position.

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | Always, until an index write | indexed | Nothing |
| indexed | A link of the response to the flare, a grant on the response, or a grant on the flare is written, and the reader can then open both | removed | Entry under `ACT#<reader>` at `A#<flare>#<position>` with the response, flare version and link time; marker at `R#<response>#<flare>` with the position. The position is new from `SEQ#responses`, taken in the same transaction. |
| removed | The reader's grant on the response is revoked or the reader leaves it | indexed, after a new grant | Nothing. `unindex` deletes the entry and its marker in one transaction per flare. |

A revoke of the reader's grant on the flare changes no entry: activity
refuses the flare to that reader, and a new grant on the flare makes the old
entries valid again.

Who gets an entry:

- After a link of response R to flare F: R's owner and each grantee of R
  who owns F or holds a grant on F.
- After a grant of person P on R: one entry for each flare R links to that
  P can open.
- After a grant of P on F: one entry for each share P owns or holds a grant
  on that links to F.

## Crash points

1. The link is written and the index writes do not finish. The response has
   entries for some readers or none. The share call answers
   `flare::shares::partly_shared`, and repeating `flare grant --re` writes
   the missing entries; markers make the repeat skip entries already
   written.
2. A grant is written and its index writes do not finish. The reader holds
   the grant and misses entries. Repeating the grant writes them, because
   `flare grant` indexes every named person again, including people who
   already hold a grant.
3. A revoke is written and `unindex` does not finish. The reader has stale
   entries. Activity opens every response before listing it, so a stale
   entry is skipped without a trace, and a repeated revoke deletes it. A
   later grant of the response to the same reader also deletes it before
   writing a new entry, so the response takes a new position.
4. An index transaction's outcome is unknown. The writer reads the markers
   and writes only the missing entries.
5. Two writers move the counter at once. One transaction fails on the
   counter condition, reads the head again and retries.

## Invariants

- An entry and the counter move in one transaction, so a reader that reads
  the head and then its entries up to the head misses no entry at or below
  the head.
- Each reader holds at most one entry per response and flare.
- A grant or link is written before its index writes, and a revoke before
  its unindex, so a failure leaves extra entries that reads skip, never
  access without a grant.
- Each writer writes its own item first and then reads the other side with
  consistent reads, so a grant racing a link leaves the entry.
- Activity lists a response only after it opens both the response and the
  flare for the caller, whatever the index says.

## Limits

- A link fans out to at most 257 readers: the response's owner and its 256
  grantees. One transaction moves the counter once and writes 49 entries
  with their markers, which is 99 of DynamoDB's 100 items, so a link takes
  at most 6 transactions.
- A grant on a response reads at most 1,000 of its links. A grant on a
  flare reads at most 500 shares the person owns and 500 shares granted to
  them, newest first, and checks them for links to the flare in batches of
  100. A person who holds more than 500 grants gets no entries for the
  older ones from that grant.
- One activity call examines at most 1,000 entries. When it reaches that
  many, it answers the responses it found with `more` true and a cursor at
  the last entry it examined. Only entries whose responses the caller can
  no longer open can fill that many without filling the page.
- Every link and every index transaction moves the one counter, so writes
  across the whole table take positions one at a time.

## What a reader sees

- An entry takes a new position when it is written. A response that a
  reader is granted after their cursor shows as new activity, even when the
  response is older than the cursor. `flare grant` removes any entry a
  newly granted reader still has for the response before it writes the new
  one, so this holds after a revoke whose removal failed. Two cases keep
  the old position, so the response does not show as new. In the first, a
  grant answers `flare::shares::partly_shared` after the grant is stored
  and before that person's old entry is removed; a repeated grant finds the
  person already holds the grant and keeps the old entry. In the second,
  the owner of a flare is granted the response automatically when it
  responds to their flare, and that grant removes no old entry.
- A reader whose grant on a flare is revoked and later granted again keeps
  their entries at their old positions, so responses that were below their
  cursor do not show again as new.
- Links written before this index existed have no entries, so activity
  does not list them. Repeating `flare grant <response> --re <flare>`
  writes their entries.
