# Lifecycle of what flare-board stores

flare-board stores three things in one board: page versions, bindings and record edits. Each change to them is one `board put` or one `board put --batch`, and each put is one Git commit. `BoardStore::open` refuses a board whose `.board/board.json` sets `commit_on_put` to false.

## State tables

### Page (`/pages/<name>/page.html` or `page.md`, and `/pages/<name>/page.json`)

| State | Event | Owner | Next state |
| --- | --- | --- | --- |
| Absent | `publish` batch commits with every expect `none` | board put | Published at version C1 |
| Published at Cn | `publish` batch commits with expects equal to the files at Cn | board put | Published at Cn+1; Cn stays readable through `at` |
| Published at Cn | `publish` loses the expect to another publish | flare-board | unchanged; flare-board reads again and retries, at most `Limits::retries` times |
| Published at Cn | a page file changes in the working tree with no commit | someone outside flare | Published at Cn with a dirty tree; the next publish reads the dirty files' blobs, replaces them and commits |
| Absent | a first publish writes its files and makes no commit | board put, stopped | Uncommitted first version: `latest` and `versions` answer `NotFound`, and the next publish expects the dirty blobs, sets `previous` to null, replaces the files and commits, giving Published at C1 (crash point 2) |

A version is never removed. No event deletes a page.

### Binding (`/bindings/<name>.json`)

| State | Event | Owner | Next state |
| --- | --- | --- | --- |
| Absent | `save_binding` passes its checks and puts with `--expect none` | board put | Saved, pinning version V |
| Absent | two saves race | board put | one Saved; the other answers `NameTaken` |
| Saved at V | `move_binding(expected = V, version = W)` passes its checks and puts with the expect of the binding's blob | board put | Saved at W, with `last_move` set |
| Saved at V | `move_binding` with `expected` other than V | flare-board | unchanged; answers `Moved` with V |
| Saved at V | a move loses the expect to another move | flare-board | unchanged by the loser; it reads again and answers `Moved` with the pin it finds |
| Saved at V | a record a fill names changes or vanishes | a record writer | Saved at V; the binding may no longer fit, and `check_fit` reports it |

No event deletes a binding.

### Record edit (any record path outside `/.board/`, `/pages/`, `/bindings/` and `/flare/`)

| State | Event | Owner | Next state |
| --- | --- | --- | --- |
| At blob B | `write(based_on = B)` batch commits the record line and one `/flare/edits/` record | board put | At blob C, the new commit |
| At blob B | `write(based_on = A)`, A other than B | flare-board, or board's expect | unchanged; answers `Conflict` |
| At blob B | `write` with a body the schema refuses | board put | unchanged; answers `Rejected`, and no edit record exists |
| At blob B | `write` with a body equal to the current value | flare-board | unchanged; answers B; no commit and no edit record |

## Crash points

1. **flare-board stops after its read and before the put starts.** Nothing is written. The caller sees no answer and may repeat the call.
2. **The board child is stopped at its deadline, the flare process dies while a put runs, or board answers `board::put::head_moved`.** A put can leave written files with no commit. flare-board answers `Unavailable`. For a record, a binding or a page that already has a version, the state is the "dirty tree" row: a later read reports the dirty bytes with the Git blob id as the blob, and a later write or publish expects that blob, replaces the files and commits. No version and no edit record exists for the half-finished put. A batch stopped midway can leave some of its files written; the same rule covers each file. When the stopped put was a page's first publish, no commit holds the record: `latest` and `versions` answer `NotFound` and `pages` lists the page. The next publish finds no row for the record in a complete walk from HEAD (or `board::path::not_found`) and publishes with `previous` null, expecting the dirty blobs, so the page recovers with no person removing files. A walk that is not complete still answers `Unavailable`.
3. **The put commits and flare-board dies before it reads the report.** The write happened and the caller saw no answer. A repeated `write` from the old blob answers `Conflict`. A repeated `save_binding` answers `NameTaken`. A repeated `move_binding` answers `Moved`. A repeated `publish` makes one more version with the same content, whose `previous` names the version the lost answer would have returned. Each answer tells the caller to read again.
4. **Board answers `board::put::locked`.** Nothing is written. flare-board answers `Unavailable`. A stale `index.lock` left by a killed Git process needs a person to remove it; flare-board never removes a lock.
5. **A read child is stopped at its deadline.** Reads change nothing; flare-board answers `Unavailable`.
6. **The thread that runs a child panics or the future is dropped.** The child keeps its own deadline and process group, so it ends by itself. The thread releases its place in the counting gate when it ends, so a dropped future leaks no place.

## Invariants

1. Every page version is a commit that changed `/pages/<name>/page.json`. When the walk from HEAD is complete, `versions` lists exactly those commits that HEAD reaches, newest first. Past the 100,000-commit walk bound and on a shallow clone, `versions` answers `Unavailable`.
2. A version's content never changes, because `at` reads the files at that commit.
3. Every publish makes a new commit, because `page.json` records `previous`.
4. A pin names a commit that changed the page's record and that HEAD reaches when it is written. `pinned` checks it on `save_binding`, `move_binding`, and a publish or a binding whose origin is a page version, and refuses a commit on another branch, a fetched commit no merge took, or one a reset orphaned. `at`, which every read uses, checks only that the commit changed the record. When the walk from HEAD is not complete and does not list the version, `pinned` accepts it unchecked (accepted gap).
5. A binding is saved or moved only after its fills were read at one revision and fit the pinned version's slots.
6. A record's blob never repeats: it is the commit that last changed the file, or the Git blob id of uncommitted bytes.
7. A record edit and its `flare/edit/v1` record are in one commit, or neither exists.
8. Every page record, binding record and edit record names who made it; a page names the version it replaced, a move names the mover and what the move was based on, and an edit names the blob it was based on.
9. flare-board holds at most `Limits::children` child processes at once. A call waits at most `Limits::child_deadline` for a place, and each child ends by `Limits::child_deadline`.
10. flare-board writes every file through `board put`. flare-board runs only the board program and never runs `git`; it reads history and commits with `board log` and `board get --at`, and takes the head commit from `board get --at HEAD`.

## Accepted gaps

- A person who resets HEAD past a commit that a binding pins leaves the pin naming an unreachable version, which `git gc` may later delete; `at` then answers `NotFound`, and the binding must be moved.
- When the walk from HEAD is not complete and does not list the version, a pin is written with no check that HEAD reaches it.
- Between `write`'s read and its put, two other writers can move the bytes away and back. The put then succeeds, because the bytes equal what the edit was based on. The edit record still names the true `based_on`.
- A binding can stop fitting after it was saved, when a record changes. flare-core's contract already says so, and readers call `check_fit`.
- When a board's configuration turns `commit_on_put` false after `open`, each put writes its files with no commit, and flare-board answers `Unavailable`.
- A page whose record last changed more than 100,000 commits before HEAD can be pinned, saved, moved and fit-checked, but `publish`, `latest` and `versions` answer `Unavailable`.
- A page whose newest version is a shallow clone's boundary cannot be published or read as latest, and `at` of the boundary answers `Unavailable`.
- Each of these three gaps is permanent for the page it reaches, and no retry recovers it. flare has no way to name an older start commit for the walk, and it never commits files that another put left written.
