# hangar-board

`BoardStore` keeps hangar's pages, bindings and record edits in a board. It
implements hangar-core's `PageStore` and `RecordStore` and runs the board
program for every read and write, so it links no Board crate and runs no
`git` itself. The board program must be a build of crumbkit/board e4ee3333
or later, which has `board get --at` and `board log`.

## What it stores

| Path | Type | What it holds |
| --- | --- | --- |
| `/pages/<name>/page.html` or `page.md` | file | The page's text. |
| `/pages/<name>/page` | `hangar/page/v1` | The slots, the provenance and `previous`, the version this one replaced. |
| `/bindings/<name>` | `hangar/binding/v1` | The pin, the fills, the provenance and the last move. |
| `/hangar/edits/<minute>-<hex>` | `hangar/edit/v1` | Who edited which record, through which binding and from which version. |

Every publish, save, move and record edit is one `board put --batch`, which
board applies whole or not at all and commits once. A page version is the
commit that changed `page.json`. A record edit's `hangar/edit/v1` record is
in the same commit as the edit. The schemas of the three types travel in
the same batch, so a board needs no install step.

A record's blob is the commit that last changed its file, or the Git blob id
of the working-tree bytes when they are not committed. `read_all` reads every
path in the tree of the commit `HEAD` names, so a binding's fit check sees
its records at one commit.

A typed Markdown record, such as a task, reads as one JSON object: each
front-matter field is a key, and the prose after the closing `---` line is
under `$body`. A write keeps the file's field order and its prose bytes.

## Opening a store

```rust,no_run
use hangar_board::{BoardConfig, BoardStore, Committer};
use std::path::PathBuf;

# fn main() -> Result<(), Box<dyn std::error::Error>> {
let config = BoardConfig::new(
    PathBuf::from("/srv/board"),
    PathBuf::from("/usr/local/bin/board"),
    PathBuf::from("/var/lib/hangar/board-state"),
    "/usr/bin:/bin".into(),
    Committer::new("hangar", "hangar@example.org")?,
);
let store = BoardStore::open(config)?;
# let _ = store;
# Ok(())
# }
```

The state directory is each child's `HOME` and holds its XDG directories.
`Limits` bounds the children that run at once, each child's deadline, the
wait for the board's lock, the output read from a child and the retries of
a publish or a move that lost a race. Dropping a call's future before it
resolves stops its child's process group and blocks until the child has
ended, so a caller that gives up on a write knows it will not commit later.

## Tests

The tests build scratch boards and run the board program from
`HANGAR_TEST_BOARD`, or from `PATH` when that variable is not set.

Licensed under either of Apache License, Version 2.0 or MIT license at your
option.
