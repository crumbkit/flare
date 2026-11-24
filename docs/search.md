# Searching flare

`flare search '<query>'` and `GET /api/search?q=<query>` find the shares you
can open: the shares you own and the shares granted to you. A search never
finds a share that `flare show` would refuse you, and nothing you type can
widen that.

In this version, words and phrases match share titles only. A search does
not read a share's Markdown or its comments. Results come newest first, by
the time each share was created.

## The syntax

| You type | It matches |
| --- | --- |
| `budget` | a title that holds budget, in any case, also inside a longer word such as budgets |
| `budget review` | a title that holds both words; a space means every part must match |
| `"budget review"` | a title that holds the phrase, with its space |
| `type:markdown` | a Markdown share; `type:html` matches HTML shares, which flare does not serve yet |
| `from:me` | a share you own |
| `from:ana@example.org` | a share that person owns |
| `from:ana` | a share owned by the person the server knows as ana, when its operator configured names |
| `from:ana,jo` | a share owned by either; a comma inside a value means any of |
| `-draft` | a title without draft; a leading minus leaves out what the part matches |

Field names may be typed in any case, and a value with a space or a comma
goes in quotes, such as `from:"ana lima"`. The answer's `query` holds the
search's canonical text: one space between parts, field names in lower
case, and quotes only where a value needs them.

## Ten examples

Each example runs as mei, who owns three shares, and to whom ana and jo
granted four more.

| Search | Finds |
| --- | --- |
| `budget` | Q3 budget, Budget review notes, Budget review 2027 |
| `"budget review"` | Budget review notes, Budget review 2027 |
| `review draft` | Review: first draft of the plan, Launch review, first draft |
| `from:me` | mei's own three shares |
| `from:ana@example.org` | Draft notes, Budget review notes |
| `from:ana,jo` | the four shares ana and jo granted to mei |
| `-from:me` | the same four: everything mei can open that she does not own |
| `notes -draft` | Budget review notes, Team notes |
| `type:markdown` | all seven shares mei can open |
| `review from:me -"first draft"` | Budget review 2027 |

A share ana owns and did not grant to mei, titled Secret budget, is in no
answer, whatever mei types. crates/flare-core/tests/search.rs runs these ten
searches.

## Errors

A search with an error does not run. The API answers 422 with an RFC 9457
problem document whose `errors` list holds each error's code, its span and
one sentence that says how to fix it. A span counts characters, as Unicode
scalar values, from 0, and its end is not included. `flare search` prints
the query with a line of carets under each error and exits with 2.

| Code | Typed | What to do |
| --- | --- | --- |
| `flare::search::unknown_filter` | `form:erin`, `updated:7d` | use `type:` or `from:`, or quote the text to search for it |
| `flare::search::unclosed_quote` | `"budget review` | add the closing quote |
| `flare::search::empty_value` | `from:`, `from:ana,` | give the filter a value |
| `flare::search::empty_phrase` | `""` | put words between the quotes, or remove them |
| `flare::search::nothing_to_exclude` | `budget - review` | put the minus right before a part |
| `flare::search::too_long` | more than 1000 characters | shorten the search |
| `flare::search::unknown_type` | `type:pdf` | use `type:markdown` or `type:html` |
| `flare::search::unknown_person` | `from:nobody` | use `me`, an address or a name the server knows |

The problem document's own code is `flare::search::unknown_person` when
every error names an unknown person, and `flare::search::invalid` otherwise.
`flare search` finds every error but an unknown person before it sends
anything, and an unknown person fails with exit code 1.

## Limits

One search reads at most 500 of your own shares and 500 of the shares
granted to you. When either list holds more, the answer's `partial` is
true, `flare search` says that a match may be missing, and a narrower
search finds it. One answer holds at most 500 shares, set by `limit`, and
`next` is always null in this version.

`GET /api/search/fields` lists the filters, with what kind of value each
takes and the values a search builder offers.

## The shared case file

`crates/flare-query/cases/search-cases.json` lists inputs with their parsed
parts and canonical text, or their error codes and spans. Every parser of
this syntax, in flare, in Board and in the later browser builder, must pass
each case. Later versions of the syntax add their cases to the same file.
