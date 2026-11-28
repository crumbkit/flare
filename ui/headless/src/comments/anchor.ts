// The text a comment on highlighted text is about, and where that text is on
// the version a reader views. Everything here is a pure function of its
// arguments, with no DOM and no clock, so every front end reads the same
// page text and finds the same place.
//
// A front end turns its page into pieces: the text of each text node in
// reading order, each with the block (paragraph, list item, heading or code
// block) that holds it. `pageText` reads the pieces into one string, and the
// same function reads the page when a reader selects text and when a later
// view searches for it.

/** The fewest characters a quote has, as flare-server allows. */
export const MIN_QUOTE_LENGTH = 3;
/** The most characters a quote has, as flare-server allows. */
export const MAX_QUOTE_LENGTH = 500;
/** The most characters of page text kept on each side of a quote. */
export const CONTEXT_LENGTH = 64;
/** The fewest context characters that let one copy of a repeated quote win. */
const MIN_SCORE = 8;
/** How many more context characters the winning copy needs than the next. */
const MIN_LEAD = 4;

/** What flare-server stores with a comment on highlighted text. */
export interface Anchor {
  readonly quote: string;
  readonly prefix: string;
  readonly suffix: string;
  /** The version of the flare the text was selected on. */
  readonly version: number;
}

/**
 * The text of one text node. `block` numbers the block that holds it, and
 * `kind` is "prose" for a paragraph, list item, heading or code block, and
 * "other" for text a comment may not quote, such as a table cell or a
 * diagram's label.
 */
export interface Piece {
  readonly text: string;
  readonly block: number;
  readonly kind: "prose" | "other";
}

/** A place in the pieces: an offset in UTF-16 units into one piece's text. */
export interface Point {
  readonly piece: number;
  readonly offset: number;
}

/**
 * The page text, and for each of its UTF-16 units and its end the place in
 * the pieces it came from.
 */
export interface PageText {
  readonly text: string;
  readonly places: readonly Point[];
}

function isBefore(a: Point, b: Point): boolean {
  return a.piece < b.piece || (a.piece === b.piece && a.offset < b.offset);
}

const segmenter = new Intl.Segmenter("en", { granularity: "grapheme" });
const blank = /^\s+$/u;

/**
 * The text of `pieces` in reading order, with each run of whitespace and
 * each break between blocks collapsed to one space, no space at either end,
 * and each character cluster normalised to NFC. A cluster is normalised on
 * its own, so each unit of the text maps back to the cluster it came from.
 */
export function pageText(pieces: readonly Piece[]): PageText {
  let text = "";
  const places: Point[] = [];
  let space: Point | undefined;
  let started = false;
  let block: number | undefined;
  pieces.forEach((piece, index) => {
    if (started && block !== undefined && piece.block !== block && space === undefined) {
      space = { piece: index, offset: 0 };
    }
    block = piece.block;
    for (const { segment, index: offset } of segmenter.segment(piece.text)) {
      if (blank.test(segment)) {
        if (started && space === undefined) space = { piece: index, offset };
        continue;
      }
      if (space !== undefined) {
        text += " ";
        places.push(space);
        space = undefined;
      }
      const place = { piece: index, offset };
      const normal = segment.normalize("NFC");
      text += normal;
      for (let unit = 0; unit < normal.length; unit += 1) places.push(place);
      started = true;
    }
  });
  const last = pieces[pieces.length - 1];
  places.push({ piece: Math.max(pieces.length - 1, 0), offset: last?.text.length ?? 0 });
  return { text, places };
}

/** The offset in the page text of the first unit at or after `point`. */
function offsetOf(page: PageText, point: Point): number {
  let low = 0;
  let high = page.text.length;
  while (low < high) {
    const middle = (low + high) >> 1;
    const place = page.places[middle];
    if (place !== undefined && isBefore(place, point)) low = middle + 1;
    else high = middle;
  }
  return low;
}

// A string's length counts UTF-16 code units; the limits count characters.
function characters(text: string): number {
  return [...text].length;
}

/** Why a selection cannot be quoted, or the anchor it makes and its place. */
export type Selected =
  | {
      readonly outcome: "anchor";
      readonly anchor: Anchor;
      readonly start: number;
      readonly end: number;
    }
  | { readonly outcome: "empty" | "spans-blocks" | "not-prose" | "too-short" | "too-long" };

/**
 * The anchor of the selection from `start` to `end` on `version`. A
 * selection quotes only text inside one prose block. Spaces at either end
 * are dropped, and a quote holds 3 to 500 characters.
 */
export function selectAnchor(
  page: PageText,
  pieces: readonly Piece[],
  start: Point,
  end: Point,
  version: number,
): Selected {
  const first = pieces[start.piece];
  const last = pieces[end.piece];
  if (first === undefined || last === undefined) return { outcome: "empty" };
  if (first.block !== last.block) return { outcome: "spans-blocks" };
  if (first.kind !== "prose") return { outcome: "not-prose" };
  let from = offsetOf(page, start);
  let to = offsetOf(page, end);
  while (from < to && page.text[from] === " ") from += 1;
  while (to > from && page.text[to - 1] === " ") to -= 1;
  if (to <= from) return { outcome: "empty" };
  const quote = page.text.slice(from, to);
  const length = characters(quote);
  if (length < MIN_QUOTE_LENGTH) return { outcome: "too-short" };
  if (length > MAX_QUOTE_LENGTH) return { outcome: "too-long" };
  return {
    outcome: "anchor",
    anchor: {
      quote,
      prefix: [...page.text.slice(0, from)].slice(-CONTEXT_LENGTH).join(""),
      suffix: [...page.text.slice(to)].slice(0, CONTEXT_LENGTH).join(""),
      version,
    },
    start: from,
    end: to,
  };
}

/**
 * Where an anchored comment's text is on the version a reader views:
 * `anchored` at the place it was made, `re-anchored` at a new place, or
 * `outdated` when the version holds no copy of the quote or several that
 * the words around them do not tell apart.
 */
export type Located =
  | {
      readonly state: "anchored" | "re-anchored";
      readonly start: number;
      readonly end: number;
    }
  | { readonly state: "outdated"; readonly reason: "quote_gone" | "ambiguous" };

/** Every offset at which `quote` starts in `text`, overlapping copies included. */
function occurrences(text: string, quote: string): number[] {
  const found: number[] = [];
  let at = text.indexOf(quote);
  while (at >= 0) {
    found.push(at);
    at = text.indexOf(quote, at + 1);
  }
  return found;
}

/** How many units of `context` match the text running back from `at`. */
function matchedBack(text: string, at: number, context: string): number {
  let count = 0;
  while (
    count < context.length &&
    at - count - 1 >= 0 &&
    text[at - count - 1] === context[context.length - count - 1]
  ) {
    count += 1;
  }
  return count;
}

/** How many units of `context` match the text running on from `at`. */
function matchedOn(text: string, at: number, context: string): number {
  let count = 0;
  while (count < context.length && text[at + count] === context[count]) count += 1;
  return count;
}

/**
 * The copy of the quote at the exact place it was selected: the stored
 * prefix and suffix around it, and a context shorter than 64 characters
 * running to the start or end of the page. An anchor stores no offset, so
 * on its own version this is how its stored place is read back.
 */
function exactPlace(anchor: Anchor, text: string): number | undefined {
  const fullPrefix = characters(anchor.prefix) === CONTEXT_LENGTH;
  const fullSuffix = characters(anchor.suffix) === CONTEXT_LENGTH;
  return occurrences(text, anchor.quote).find((at) => {
    const end = at + anchor.quote.length;
    const from = at - anchor.prefix.length;
    return (
      from >= 0 &&
      text.slice(from, at) === anchor.prefix &&
      (fullPrefix || from === 0) &&
      text.slice(end, end + anchor.suffix.length) === anchor.suffix &&
      (fullSuffix || end + anchor.suffix.length === text.length)
    );
  });
}

/**
 * The copy of the quote that wins: the only one, or the one whose context
 * matches at least 8 characters and 4 more than the next best.
 */
function bestPlace(anchor: Anchor, text: string): Located {
  const found = occurrences(text, anchor.quote);
  const end = (at: number) => at + anchor.quote.length;
  const [only] = found;
  if (only === undefined) return { state: "outdated", reason: "quote_gone" };
  if (found.length === 1) return { state: "re-anchored", start: only, end: end(only) };
  const scored = found
    .map((at) => ({
      at,
      score: matchedBack(text, at, anchor.prefix) + matchedOn(text, end(at), anchor.suffix),
    }))
    .sort((a, b) => b.score - a.score);
  const [best, next] = scored;
  if (best === undefined || best.score < MIN_SCORE || best.score - (next?.score ?? 0) < MIN_LEAD) {
    return { state: "outdated", reason: "ambiguous" };
  }
  return { state: "re-anchored", start: best.at, end: end(best.at) };
}

/**
 * Where `anchor`'s quote is in `text`, the page text of version `viewed`.
 * On the comment's own version the quote is read back at its exact place.
 * On another version the winning copy is anchored when it starts at the
 * offset the quote had on its own version, whose page text is `own`, and
 * re-anchored otherwise, also when `own` is not known.
 *
 * There is no fuzzy matching: a quote with one character changed is
 * outdated, even though a person would still find the place. A later
 * slice may add it if outdated comments turn out to be common.
 */
export function locate(anchor: Anchor, text: string, viewed: number, own?: string): Located {
  if (viewed === anchor.version) {
    const at = exactPlace(anchor, text);
    if (at !== undefined) return { state: "anchored", start: at, end: at + anchor.quote.length };
  }
  const found = bestPlace(anchor, text);
  if (found.state === "outdated" || own === undefined) return found;
  return exactPlace(anchor, own) === found.start ? { ...found, state: "anchored" } : found;
}
