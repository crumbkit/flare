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
  `erDiagram`, `gantt` or `pie` becomes a figure, and the server draws the
  diagram in it as SVG (see [Diagrams](#diagrams)). Any other Mermaid block,
  including one that starts with a `%%{init}%%` directive, with front
  matter or with `stateDiagram-v2`, shows as code.
- Every other code block shows its language and a Copy button.
- A table scrolls sideways in its own box when the screen is narrower than
  the table.
- A paragraph that starts with "Figure", a number and a full stop, such as
  "Figure 1. What happens to one sailing", and comes directly after a
  diagram, table or code block at the top level of the document, becomes
  that block's caption. Any other paragraph stays a paragraph.

## Diagrams

flare-server draws each diagram with limu-rs 0.2.1, a Rust port of a
Mermaid renderer, so the page shows it with no script. limu's drawing
differs from the one Mermaid or another Mermaid tool shows the author: its
layout places nodes differently, and its edge labels and attributes are
12 px where node labels are 14 px.

- The drawing is laid out for the 820 px article column, with text measured
  as IBM Plex Sans at 14 px. Text outside ASCII is measured with one average
  width, so its box can be a little too wide or too narrow.
- Flowcharts, graphs and state diagrams have right-angle edges. Class and
  ER diagrams keep limu's own edges.
- limu draws no link, image or icon that a diagram asks for, and no
  script.
- One drawing serves light and dark: limu writes its colours as `--limu-*`
  CSS variables, and the stylesheet sets them from flare's colour tokens in
  each scheme. Colours that a diagram's own `style` or `classDef` lines set
  stay as written in both schemes.
- A drawing wider than the column shrinks until its smallest text would
  fall under 10.5 px, and from there scrolls sideways in its box, which
  takes keyboard focus.
- A diagram limu cannot parse keeps its source in view, followed by "Flare
  could not draw this diagram:" and limu's message. The other diagrams on
  the page still draw. To fix it, change or simplify the source and share
  a new version.
- A diagram whose source is over 50,000 bytes, Mermaid's own limit, shows
  as its source with a note that it is too long to draw. limu never lays
  it out. To show it, split it into smaller diagrams.

## The foot

Below the article, the page's foot says "Shared with Flare". When the
config sets `hosted.get_flare` (`docs/hosting.md`) and the reader owns no
share, the foot also holds a Get Flare link that opens that address in a
new tab, with `rel="noopener noreferrer"` and the accessible name "Get
Flare, opens in a new tab". The owner of the share never sees the link,
and a reader who owns one or more shares does not see it either. The link
has no dismiss control, and flare stores nothing about it. Whether the
reader owns a share is read from the share store on each page view, which
adds one read for a reader who is not the owner when the address is set.
When that read fails, the page shows the note without the link and the
server logs a warning. On a screen 640 px wide or narrower, the note and
the link stack and the link is 44 px tall.

## The comment count and the comments section

The line under the header ends with the comment count, as a link to the
comments section. It counts as the section's heading does, such as "2
comments, 1 reply", "1 comment" or "No comments": every reply that is not
deleted, and every top-level comment that is not deleted or that still has
such a reply. When the comment store does not answer, the page leaves the
count out.

Below the article, the comments section follows artboards 01 to 07 of the
comment parts mockup. It lists every comment on the share with its replies,
oldest first, each author as their identifier with its initials, and each
time in the reader's own time zone, such as "21 Nov, 16:02", or "just now"
for a post from the last minute. Under each top-level comment is a one-line
reply field, and under the thread is the field for a comment on the whole
page. A reply has no reply field. In the page field Enter adds a line and
Ctrl or Command with Enter sends; in a reply field Enter sends. Leaving the
page with unsent text asks first.

- While the comments load, the section says "Loading comments".
- When the comment routes give no answer or a server error, the section says
  so and offers Retry. When they refuse the reader (403 or 404), it says the
  reader no longer has access and names the owner, and when the session has
  ended (401) it asks the reader to reload the page to sign in again. Neither
  offers Retry. Once the thread has loaded, a later refetch that fails, such
  as the one after each send, keeps the thread and every draft as they are.
- A send with no answer keeps the text and offers Retry, which sends the
  draft again with the same key, so the server stores it once. Every other
  failure keeps the text, says why and offers Copy text with no Retry: the
  reader lost access (403 or 404), the flare already holds 2,000 comments
  and replies (409), the session ended (401), or the text holds a control
  character the server does not store (400). The last one names the first
  such character by its code point and its position in characters, such as
  "the control character U+0007 at character 5", and editing the text lets
  the reader send it again.
- After a send, the post joins its thread, the field empties and keeps
  focus, and a screen reader hears "Comment sent" or "Reply sent".
- An edited comment shows its current text with no edited mark, a deleted
  reply is left out, and a deleted comment stays only while it still has
  replies. Its card keeps its place above them and shows "Its author deleted
  this comment." in muted italic type, with no author or time, and its reply
  field is labelled "Reply" with the placeholder "Write a reply.". Editing and
  deleting from the page are not built yet.
- New comments from other readers show only after a reload.

The server writes only the section's heading and "Loading comments". When
the comments script never runs, a line saying the comments did not load
shows after ten seconds.

## What the scripts do

Every page loads the comments script, and a page with code also loads the
reading script, so a page without code never fetches Shiki. Diagrams need
no script. A page that loads a script has a Content-Security-Policy that
adds `script-src 'self'` to the hosted policy, whose `connect-src 'self'`
lets the comments section call `/api/comments` with the session cookie.
The scripts come from `ui/flare`, which Vite builds; flare-server embeds the
built files and serves them at `/flare/assets/<file>` with a cache of one
year. The comments script parses every answer with zod in its jitless mode,
because the policy has no `'unsafe-eval'`.

- Shiki colours code with GitHub Light and GitHub Dark. Five theme colours
  that fall below a 4.5:1 contrast on the code background are replaced with
  darker or lighter GitHub colours.

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
build script warns, pages show plain, uncoloured code, and the comments
section never loads. Diagrams are still drawn. To build
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

The comments journey, `ui/flare/tests/acceptance/comments.spec.ts`, needs a
running server. The flare-server test
`a_reader_comments_and_replies_in_a_browser_and_sees_both_after_a_reload`
starts the hosted server with memory stores, shares artboard 01's sample
with a reader who signed in through the test provider, and, when
`FLARE_COMMENTS_JOURNEY` names `ui/flare`, runs the journey against it with
the reader's session cookie. In light, in dark and at 390 px wide, the
reader posts a comment and a reply, reloads and sees both, and the journey
checks the Tab order, axe and the policy. CI's `reading` job runs it after
installing Chromium.
