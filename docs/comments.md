# Comments, replies and named cursors

A person who can open a flare comments on it and replies to a comment. The
CLI and the hosted page use the same API. This page gives the life of a
comment, of its key slot and of a named cursor, and what each crash leaves.

## Commands and routes

| Command | Route | Credential |
| --- | --- | --- |
| `flare comments <flare>` | `GET /api/comments?flare=<id>` | token or session cookie |
| `flare comment <flare> --body <text>` | `POST /api/comments` | token or session cookie |
| `flare reply <comment> --flare <flare> --body <text>` | `POST /api/comments` with `parent` | token or session cookie |
| `flare comment-edit <comment> --body <text>` | `PATCH /api/comments/<id>` | token or session cookie |
| `flare comment-delete <comment> --yes` | `DELETE /api/comments/<id>` | token or session cookie |
| `flare activity <flare> --since-last <name>` | `GET /api/activity`, `PUT /api/cursors/<name>` | token or session cookie, then token |
| (none) | `GET /api/cursors/<name>` | token |

- `--body-file <path>` replaces `--body` and reads at most 64 KiB.
- A comment is 1 to 8,000 characters of text. The server refuses any
  other with `flare::comments::invalid`.
- `flare comment` and `flare reply` send a key. Without `--key` the CLI
  draws 32 random hex digits for each run. A key is 1 to 64 letters,
  digits, `.`, `_` or `-`.
- When a comment or a reply with a drawn key fails in a way that may have
  saved it (a server fault, a passed deadline, a connection dropped after
  the request was sent, an unreadable success answer, or
  `flare::comments::unavailable`), the failure's recovery is the same
  command with `--key <the drawn key>`. Running it returns the saved
  comment, or saves it once. A command given `--key` recovers with a plain
  retry, because it already repeats its key.
- `POST` answers 201 for a new comment and 200 for a repeated key.
- `flare activity` lists comment, reply, edit and delete events with
  responses, oldest first.

## Comment and reply

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| live | `POST /api/comments` succeeds for a person who can open the flare | live (edit), deleted | Id, flare, parent for a reply, author address, created time, body, the anchor of a comment on page text, the author's key slot, one `comment` or `reply` event at a position, and the flare's comment count raised by one |
| live, edited | Its author sends `PATCH /api/comments/<id>` | live (edit), deleted | The new body replaces the old one and the edited time is set; one `edited` event at a new position; no earlier body is kept |
| deleted | Its author sends `DELETE /api/comments/<id>` | none | The body and the anchor are erased; id, author, times, parent and the key slot stay; one `deleted` event at a new position |

- A reply to a deleted comment stays live and keeps its parent.
- A reply has no replies. A reply to a reply fails with
  `flare::comments::reply_depth`.
- A comment whose flare its author can no longer open stays in its state,
  and its author can neither read nor change it until a grant returns.
- A flare holds at most 2,000 comments and replies. The stored count is
  never lowered, so a delete does not make room, and the 2,001st write
  fails with `flare::comments::too_many`.
- The share summary that opening a share returns has `comments`, the
  number of comments and replies that are not deleted. It is absent when
  the comment store did not answer.
- Only the author edits or deletes. Another person gets
  `flare::comments::not_author`, and a person who cannot open the flare
  gets `flare::comments::not_found`.
- An edit adds a new `edited` event each time, also when the body is the
  same, so `flare activity` lists it again.
- A comment's events carry no flare version. A comment on page text
  carries the version in its anchor.

## Comment on page text

A reader of a Markdown flare selects text in its article and comments on
it. The comment's `anchor` holds what the page needs to find that text
again.

| Field | Holds |
| --- | --- |
| `quote` | The selected text, 3 to 500 characters, with each run of whitespace as one space |
| `prefix` | Up to 64 characters of page text just before the quote |
| `suffix` | Up to 64 characters of page text just after the quote |
| `version` | The flare version the text was selected on |

- Only a comment on a Markdown flare takes an anchor. A reply with one
  fails with `flare::comments::anchor_on_reply`, a comment on an HTML
  flare with one fails with `flare::comments::anchor_kind`, and an anchor
  whose fields are out of range or whose version the flare does not have
  fails with `flare::comments::anchor_invalid`. Each answers 400 and
  writes nothing.
- An edit keeps the anchor. A delete erases it with the body.
- `flare comments` prints the quote in the column before the body.
- The page finds the quote in the version it shows, as one of these:

| Shown as | When |
| --- | --- |
| anchored | The quote is where it was on its own version; the text is marked and the card shows its quote |
| re-anchored | The quote is found once elsewhere, or by its prefix and suffix among several copies; the card adds "Made on version N. The text has moved since then." |
| outdated | The quote is gone, or several copies match equally; nothing is marked and the card says "The quoted text changed on version N." |

- On a wide page the cards show in a margin beside the article, anchored
  and re-anchored ones in page order and then the outdated ones. Each card
  sits level with the line where its text starts, and a card that would
  overlap the one above it is pushed down. While a new card on selected
  text is empty, the selection is drawn in the same tint as the mark. On a
  phone each card shows under the paragraph that holds its text, and an
  outdated card shows in Comments at the end of the page.
- To tell anchored from re-anchored on a later version, the page reads the
  comment's own version with `GET /s/<id>?version=<n>`. Until that answers,
  the card shows its quote with no mark and no note. When it fails, the
  search alone places the card, so a quote found once shows as moved.

## Activity

- Responses and comment events take positions from one counter, so one
  cursor orders both.
- `flare activity` leaves out a comment, reply or edit event whose
  comment is deleted by the time of the read, because the body is gone.
  It still lists the `deleted` event.
- A cursor grants nothing. Each read checks that the caller can open each
  named flare.

## Key slot

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| held | The transaction that writes the comment writes it | none | Author, flare, key and the comment id; a repeat with the same author, flare and key answers that comment and writes nothing |

## Named cursor

| State | Entered when | Leaves to | Stored |
| --- | --- | --- | --- |
| absent | No `PUT /api/cursors/<name>` by this caller yet | saved | Nothing; `GET` answers `flare::cursors::not_found` |
| saved | `PUT /api/cursors/<name>` by its caller | saved | Caller, name and position; a later `PUT` replaces the position only when it is larger |

- A person keeps at most 32 named cursors. The 33rd name fails with
  `flare::cursors::too_many`.
- `flare activity --since-last <name>` starts from the largest of
  `--since`, the cursor in `<state home>/flare/cursors.json` and the
  cursor the server saved under the name. It saves the last cursor of the
  run in the file and then on the server. A cursor only moves forward in
  both places.
- The file maps each server's origin to its names and holds at most
  64 KiB. A file that cannot be read or written fails the command with
  `flare::state::unusable` before it prints, and deleting the file clears
  it.
- A refusal of the server's save leaves a note on standard error and the
  exit code 0, because the file on this computer has the cursor.

## Browser requests

- The cookie routes are the five comment routes and `GET /api/activity`.
  Every other API route takes a token only, and the cursor routes are among
  them.
- A request with the session cookie needs `Sec-Fetch-Site: same-origin`.
  One that is not `GET` also needs an `Origin` header equal to the
  site's origin and a JSON `Content-Type`. Any other request fails with
  `request::origin` (403).
- A request with an `Authorization` header is a token request, whatever
  cookies it holds.
- A reading page with code or a diagram carries the script policy, which
  has `script-src 'self'` and no `connect-src`, so its scripts can make no
  request (see [reading.md](reading.md)). Every other hosted page carries
  `connect-src 'self'`, which limits the requests a page makes to the
  site's own origin, and runs no script.

## Crash points

1. The server crashes before the comment transaction commits. Nothing is
   written. The retry with the same key writes the comment once.
2. The transaction commits and the answer is lost. The comment is live. The
   retry with the same key reads the key slot and answers the same comment.
3. Two writes race for the counter. The counter move is conditional, so one
   commits and the other reads the counter again and takes the next position.
4. An edit or a delete commits and the answer is lost. A repeat of the
   delete changes nothing more and answers the deleted comment. A repeat of
   the edit adds one more `edited` event.
5. `flare activity --since-last` fails before it saves the cursor in the
   file, as when the server is down. Nothing moves, and the next call lists
   the same items.
6. `flare activity --since-last` saves the cursor in the file and then
   fails to print, as when the reader of its output closed the pipe. The
   next call skips what the run did not print.
7. `flare activity --since-last` saves the file and the server refuses its
   save. This computer's next call starts at the new cursor. Another
   computer starts at the older one and lists some items again.

## Invariants

- Each event is written in the same transaction as the counter move, so an
  event is readable once its position is the head.
- A reply's parent is a top-level comment.
- Only the author changes a comment, and the server checks the author
  before any write.
- A deleted comment holds no body and no anchor, and no event holds a
  deleted comment's body.
- Each read checks that the caller can open the flare.
