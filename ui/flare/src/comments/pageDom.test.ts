// The reading page's text as the headless core reads it, from the article
// the server writes.
import { describe, expect, it } from "vitest";
import { clearMarks, markRange, pointAt, readPage } from "./pageDom.ts";

function article(body: string): HTMLElement {
  document.body.innerHTML = `<main class="screen"><article class="doc"><div class="share-meta"><div class="line">Shared by <b>Ana</b> · version 3</div></div><h1>Sailings</h1>\n${body}</article></main>`;
  const found = document.querySelector<HTMLElement>("article.doc");
  if (found === null) throw new Error("no article");
  return found;
}

describe("reading the page", () => {
  it("reads the Markdown's text in order and leaves out the line under the title, the title and code chrome", () => {
    const doc = article(
      '<h2>When a sailing is held</h2>\n<p>Each vessel has a <strong>gust</strong> limit.</p>\n<div class="code"><div class="code-head"><span>rust</span><button>Copy</button></div><pre><code>let x = 1;</code></pre></div>',
    );
    expect(readPage(doc).text.text).toBe(
      "When a sailing is held Each vessel has a gust limit. let x = 1;",
    );
  });

  it("gives text in a table cell or a diagram the kind a comment may not quote", () => {
    const doc = article(
      '<table><tbody><tr><td>Curlew</td></tr></tbody></table><figure class="dg"><svg><text>Gust</text></svg></figure><p>Prose</p>',
    );
    const { pieces } = readPage(doc);
    expect(pieces.map((piece) => [piece.text, piece.kind])).toEqual([
      ["Curlew", "other"],
      ["Prose", "prose"],
    ]);
  });

  it("puts the pieces of one paragraph in one block and a list item in a block of its own", () => {
    const doc = article("<p>One <em>two</em></p><ul><li>three</li></ul>");
    const blocks = readPage(doc).pieces.map((piece) => piece.block);
    expect(blocks[0]).toBe(blocks[1]);
    expect(blocks[2]).not.toBe(blocks[1]);
  });

  it("turns a selection's boundary into a place in the pieces, also when it names an element", () => {
    const doc = article("<p>One <em>two</em> three</p>");
    const page = readPage(doc);
    const em = doc.querySelector("em");
    const text = em?.firstChild;
    if (!(text instanceof Text) || em === null) throw new Error("no text");
    expect(pointAt(page, text, 1)).toEqual({ piece: 1, offset: 1 });
    expect(pointAt(page, em.parentElement ?? doc, 2)).toEqual({ piece: 2, offset: 0 });
  });
});

describe("marking text", () => {
  it("wraps a range across nodes in marks and clearing restores the text", () => {
    const doc = article("<p>If its peak <strong>gust</strong> is above the limit</p>");
    const page = readPage(doc);
    const start = page.text.text.indexOf("peak");
    const end = page.text.text.indexOf("above") + "above".length;
    markRange(page, start, end, "anchor");
    const marks = [...doc.querySelectorAll("mark.anchor")].map((mark) => mark.textContent);
    expect(marks).toEqual(["peak ", "gust", " is above"]);
    expect(readPage(doc).text.text).toBe(page.text.text);
    clearMarks(doc);
    expect(doc.querySelectorAll("mark")).toHaveLength(0);
    expect(doc.querySelector("p")?.innerHTML).toBe(
      "If its peak <strong>gust</strong> is above the limit",
    );
  });
});
