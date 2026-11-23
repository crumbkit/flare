# The reading page

A hosted flare shows a Markdown share at `/s/<id>` to a signed-in reader
whom the share's owner granted it to, and to the owner. The page follows
artboard 01 of the signed Flare mockup. `?version=<n>` shows an older
version. A share that is missing, a share the reader may not open, a version
that does not exist and a share that is not Markdown all give the same
not-found page, so a reader cannot tell them apart. When the share store
does not answer, the page is a 503 with a Retry link.

## What the server writes

The server renders the Markdown itself, and the page is complete before any
script runs. Raw HTML in the Markdown shows as text.

- A fenced block whose language is `mermaid` and whose first word is one of
  `flowchart`, `graph`, `sequenceDiagram`, `stateDiagram`, `classDiagram`,
  `erDiagram`, `gantt` or `pie` becomes a figure that holds its source. Any
  other Mermaid block, including one that starts with a `%%{init}%%`
  directive or with front matter, shows as code.
- Every other code block shows its language and a Copy button.
- A table scrolls sideways in its own box when the screen is narrower than
  the table.
- A paragraph that starts with "Figure", a number and a full stop, such as
  "Figure 1. What happens to one sailing", and comes directly after a
  diagram, table or code block at the top level of the document, becomes
  that block's caption. Any other paragraph stays a paragraph.

## What the script does

The page loads its script only when it has code or a diagram, and only then
does its Content-Security-Policy add `script-src 'self'`. The script comes
from `ui/flare`, which Vite builds; flare-server embeds the built files and
serves them at `/flare/assets/<file>` with a cache of one year.

- Shiki colours code with GitHub Light and GitHub Dark. Five theme colours
  that fall below a 4.5:1 contrast on the code background are replaced with
  darker or lighter GitHub colours.
- Mermaid draws each diagram twice, once for light and once for dark, with
  the base theme, the classic look and the ELK layout. Mermaid loads only on
  a page that has a diagram. A diagram Mermaid cannot parse keeps its source
  and shows Mermaid's message; the other diagrams still draw.

### Languages

Shiki colours these languages, each loaded only when a page has a block in
it. A block in any other language stays plain and keeps its label.

| Language | Fence names |
| --- | --- |
| Bash | `bash`, `sh`, `shell`, `zsh` |
| CSS | `css` |
| Diff | `diff` |
| Go | `go` |
| HTML | `html` |
| JavaScript | `javascript`, `js` |
| JSON | `json` |
| Markdown | `markdown`, `md` |
| Python | `python`, `py` |
| Rust | `rust`, `rs` |
| SQL | `sql` |
| Swift | `swift` |
| TOML | `toml` |
| TypeScript | `typescript`, `ts` |
| YAML | `yaml`, `yml` |

To add a language, add one line to `ui/flare/src/languages.ts` and one row
here.

## Fonts

The stylesheet that flare-server serves declares IBM Plex Sans and
JetBrains Mono with `@font-face`. The Latin subset of each family's variable
font sits in `crates/flare-server/fonts/` with its SIL Open Font License
file, and `/flare/assets/<name>` serves it as `font/woff2` with the same
year-long immutable cache as the script files. Each name ends with the first
8 hex digits of the file's SHA-256, so a new font file needs a new name and a
matching `@font-face` rule in `docs.rs`. The page's policy allows fonts only
from its own origin, so a font host is never asked.

## Building without Node

Cargo builds flare without Node. When `ui/flare/dist` holds no bundle, the
build script warns, and pages show plain code and diagram source. To build
the bundle, run `pnpm install` and `pnpm run build` in `ui/`. With
`FLARE_REQUIRE_UI=1`, a missing bundle fails the build. CI's reading job
sets it. The repository has no release workflow yet, so a build that will
serve readers, such as one for a hosted deployment, must set it itself;
without it, Cargo only warns and the reading pages load no script.

A test fails when any built file is over 5 MB, because AWS Lambda, which
serves the hosted site, refuses a response over 6 MB; the remedy is to split
the chunk in `ui/flare/vite.config.ts`.

## Acceptance journey

CI's `reading` job builds the bundle, runs a flare-server test that writes
artboard 01's sample share as a reader sees it into the directory that
`FLARE_READING_SAMPLE` names, and then runs the Playwright journey in
`ui/flare/tests/acceptance` in light, in dark and at 390 px wide. The
journey fails on any policy violation, console error or axe violation, and
saves a screenshot of each run.
