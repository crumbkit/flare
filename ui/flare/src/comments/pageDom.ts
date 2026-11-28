// The reading page's article as the headless core's pieces, and the marks
// that show a comment's text on it. The article holds the line under the
// title and the title before the Markdown, a code block holds its
// language and Copy button, and on a phone the cards on page text sit
// between its paragraphs; none of them is the Markdown's text, so the
// pieces leave them out.
import { type PageText, type Piece, type Point, pageText } from "@flare/headless/comments/anchor";

/** The pieces of one article, the text nodes they came from, and their page text. */
export interface Page {
  readonly pieces: readonly Piece[];
  readonly nodes: readonly Text[];
  readonly text: PageText;
}

/** The blocks a comment may quote inside. */
const PROSE = "p, li, h1, h2, h3, h4, h5, h6, pre";
/** The blocks whose text is on the page but may not be quoted. */
const OTHER = "td, th, figure, caption";
/** Elements whose text is not the Markdown's. */
const SKIPPED = ".share-meta, .code-head, .margin, button, script, style, svg, .visually-hidden";

function isTitle(element: Element, article: Element): boolean {
  return (
    element.parentElement === article &&
    element.matches("h1") &&
    article.querySelector(":scope > h1") === element
  );
}

/** Reads the text of `article` in reading order. */
export function readPage(article: Element): Page {
  const pieces: Piece[] = [];
  const nodes: Text[] = [];
  const blocks = new Map<Element, number>();
  const walker = article.ownerDocument.createTreeWalker(article, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    if (!(node instanceof Text)) continue;
    const parent = node.parentElement;
    if (parent === null || parent.closest(SKIPPED) !== null) continue;
    const titled = parent.closest("h1");
    if (titled !== null && isTitle(titled, article)) continue;
    const other = parent.closest(OTHER);
    const prose = other === null ? parent.closest(PROSE) : null;
    const block = other ?? prose;
    if (block === null || !article.contains(block)) {
      if (node.data.trim() === "") continue;
    }
    const key = block ?? parent;
    let number = blocks.get(key);
    if (number === undefined) {
      number = blocks.size;
      blocks.set(key, number);
    }
    pieces.push({ text: node.data, block: number, kind: prose === null ? "other" : "prose" });
    nodes.push(node);
  }
  return { pieces, nodes, text: pageText(pieces) };
}

/**
 * The place in `page`'s pieces of the boundary point (`container`,
 * `offset`) of a selection: the text node it names, or the first text node
 * after it when it names an element.
 */
export function pointAt(page: Page, container: Node, offset: number): Point | undefined {
  if (container instanceof Text) {
    const piece = page.nodes.indexOf(container);
    if (piece >= 0) return { piece, offset };
  }
  const boundary = container.ownerDocument?.createRange();
  if (boundary === undefined) return undefined;
  boundary.setStart(container, offset);
  const piece = page.nodes.findIndex((node) => boundary.comparePoint(node, 0) >= 0);
  if (piece >= 0) return { piece, offset: 0 };
  const last = page.nodes.length - 1;
  return last < 0 ? undefined : { piece: last, offset: page.nodes[last]?.data.length ?? 0 };
}

/** The element of the block that holds the page text's offset `at`. */
export function blockAt(page: Page, at: number): Element | undefined {
  const place = page.text.places[at];
  return place === undefined ? undefined : (page.nodes[place.piece]?.parentElement ?? undefined);
}

/**
 * Wraps the page text from `start` to `end` in `<mark class="{kind}">`
 * elements, one for each text node it crosses. The page reads the same
 * text afterwards.
 */
export function markRange(page: Page, start: number, end: number, kind: string): void {
  const from = page.text.places[start];
  const to = page.text.places[end];
  if (from === undefined || to === undefined || end <= start) return;
  for (let piece = to.piece; piece >= from.piece; piece -= 1) {
    const node = page.nodes[piece];
    if (node === undefined) continue;
    const first = piece === from.piece ? from.offset : 0;
    const last = piece === to.piece ? to.offset : node.data.length;
    if (last <= first) continue;
    const range = node.ownerDocument.createRange();
    range.setStart(node, first);
    range.setEnd(node, last);
    const mark = node.ownerDocument.createElement("mark");
    mark.className = kind;
    range.surroundContents(mark);
  }
}

/** Removes every mark `markRange` made under `root`, joining the text it split. */
export function clearMarks(root: Element, kind?: string): void {
  const selector = kind === undefined ? "mark.anchor, mark.sel" : `mark.${kind}`;
  for (const mark of root.querySelectorAll(selector)) {
    const parent = mark.parentNode;
    if (parent === null) continue;
    while (mark.firstChild) parent.insertBefore(mark.firstChild, mark);
    mark.remove();
    parent.normalize();
  }
}

/**
 * The top of the line that holds the page text's offset `at`, in the
 * viewport, or undefined when the browser does not lay the page out.
 */
export function textTop(page: Page, at: number): number | undefined {
  const place = page.text.places[at];
  const node = place === undefined ? undefined : page.nodes[place.piece];
  if (place === undefined || node === undefined) return undefined;
  const range = node.ownerDocument.createRange();
  range.setStart(node, Math.min(place.offset, node.data.length));
  range.setEnd(node, Math.min(place.offset + 1, node.data.length));
  if (typeof range.getBoundingClientRect !== "function") return undefined;
  return range.getBoundingClientRect().top;
}
