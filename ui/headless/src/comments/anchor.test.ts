import { describe, expect, it } from "vitest";
import {
  type Anchor,
  CONTEXT_LENGTH,
  type Located,
  locate,
  MAX_QUOTE_LENGTH,
  type Piece,
  pageText,
  selectAnchor,
} from "./anchor.ts";

/** A page of one block per string, each block one piece of prose. */
function page(...blocks: string[]): Piece[] {
  return blocks.map((text, block) => ({ text, block, kind: "prose" }));
}

/** The anchor that selecting from `start` to `end` of the one-block page `text` makes. */
function anchorAt(text: string, start: number, end: number, version: number): Anchor {
  const pieces = page(text);
  const made = selectAnchor(
    pageText(pieces),
    pieces,
    { piece: 0, offset: start },
    { piece: 0, offset: end },
    version,
  );
  if (made.outcome !== "anchor") throw new Error(`refused: ${made.outcome}`);
  return made.anchor;
}

/** The anchor that selecting `quote` in `text` at its first occurrence makes on `version`. */
function anchorOf(text: string, quote: string, version: number): Anchor {
  const start = text.indexOf(quote);
  if (start < 0) throw new Error(`${quote} is not in the text`);
  return anchorAt(text, start, start + quote.length, version);
}

function quoted(text: string, located: Located): string | undefined {
  return located.state === "outdated" ? undefined : text.slice(located.start, located.end);
}

const held = "If its peak gust for the crossing is above the limit, the sailing is held";
const version2 = [
  "When a sailing is held",
  `Each vessel has a gust limit in knots, stored as gust_limit_knots. A new forecast arrives every hour. ${held} and passengers see a notice that says the sailing is under review.`,
  "A gust that is more than 5 knots above the limit cancels the sailing at once, with no review.",
];

describe("page text", () => {
  it("collapses each run of whitespace and each break between blocks to one space", () => {
    expect(pageText(page("  One\n\ttwo  ", "three ")).text).toBe("One two three");
  });

  it("normalises the text to NFC, so a decomposed accent matches a composed one", () => {
    expect(pageText(page("Toma\u0301s")).text).toBe("Tom\u00e1s");
  });

  it("reads pieces of one block, such as the text around bold words, as one run", () => {
    const pieces: Piece[] = [
      { text: "the ", block: 0, kind: "prose" },
      { text: "sailing", block: 0, kind: "prose" },
      { text: " is held", block: 0, kind: "prose" },
    ];
    expect(pageText(pieces).text).toBe("the sailing is held");
  });
});

describe("selecting text", () => {
  it("keeps the selected text and up to 64 characters of page text on each side", () => {
    const text = version2.join(" ");
    const anchor = anchorOf(version2[1] ?? "", held, 2);
    expect(anchor.quote).toBe(held);
    expect([...anchor.prefix]).toHaveLength(CONTEXT_LENGTH);
    expect(text).toContain(`${anchor.prefix}${held}${anchor.suffix}`);
    expect(anchor.suffix.startsWith(" and passengers")).toBe(true);
    expect(anchor.version).toBe(2);
  });

  it("keeps a shorter context at the start and end of the page", () => {
    const anchor = anchorOf("The sailing is held.", "The sailing", 1);
    expect(anchor.prefix).toBe("");
    expect(anchor.suffix).toBe(" is held.");
  });

  it("drops the spaces at either end of a selection", () => {
    const pieces = page("the sailing is held");
    const made = selectAnchor(
      pageText(pieces),
      pieces,
      { piece: 0, offset: 3 },
      { piece: 0, offset: 12 },
      1,
    );
    expect(made.outcome === "anchor" && made.anchor.quote).toBe("sailing");
  });

  it("refuses a selection that spans two blocks, and admits one inside either block", () => {
    const pieces = page("First paragraph here", "Second paragraph here");
    const text = pageText(pieces);
    const across = selectAnchor(text, pieces, { piece: 0, offset: 6 }, { piece: 1, offset: 6 }, 1);
    expect(across.outcome).toBe("spans-blocks");
    const inside = selectAnchor(text, pieces, { piece: 1, offset: 0 }, { piece: 1, offset: 6 }, 1);
    expect(inside.outcome).toBe("anchor");
  });

  it("refuses a selection in a table cell or a diagram, and admits one in prose", () => {
    const pieces: Piece[] = [
      { text: "Vessel Curlew", block: 0, kind: "other" },
      { text: "Vessel Curlew", block: 1, kind: "prose" },
    ];
    const text = pageText(pieces);
    const cell = selectAnchor(text, pieces, { piece: 0, offset: 0 }, { piece: 0, offset: 6 }, 1);
    expect(cell.outcome).toBe("not-prose");
    const prose = selectAnchor(text, pieces, { piece: 1, offset: 0 }, { piece: 1, offset: 6 }, 1);
    expect(prose.outcome).toBe("anchor");
  });

  it("refuses an empty selection, one under 3 characters and one over 500", () => {
    const long = "x".repeat(MAX_QUOTE_LENGTH + 1);
    const pieces = page(`ab ${long}`);
    const text = pageText(pieces);
    const at = (start: number, end: number) =>
      selectAnchor(text, pieces, { piece: 0, offset: start }, { piece: 0, offset: end }, 1).outcome;
    expect(at(2, 3)).toBe("empty");
    expect(at(0, 2)).toBe("too-short");
    expect(at(3, 3 + MAX_QUOTE_LENGTH + 1)).toBe("too-long");
    expect(at(3, 3 + MAX_QUOTE_LENGTH)).toBe("anchor");
    expect(at(3, 6)).toBe("anchor");
  });
});

describe("finding a comment's text on a version", () => {
  const anchor = anchorOf(version2[1] ?? "", held, 2);
  const own = pageText(page(...version2)).text;

  it("is anchored on the version the comment was made on", () => {
    const located = locate(anchor, own, 2);
    expect(located.state).toBe("anchored");
    expect(quoted(own, located)).toBe(held);
  });

  it("is re-anchored where the text moved down the page on a later version", () => {
    const later = pageText(
      page(
        version2[0] ?? "",
        "Forecasts come from the mid-channel buoy, which reports every ten minutes.",
        ...version2.slice(1),
      ),
    ).text;
    const located = locate(anchor, later, 3, own);
    expect(located.state).toBe("re-anchored");
    expect(quoted(later, located)).toBe(held);
  });

  it("stays anchored on a later version that changed only text after it", () => {
    const later = pageText(page(...version2.slice(0, 2), "A new last paragraph.")).text;
    expect(locate(anchor, later, 3, own).state).toBe("anchored");
  });

  it("is re-anchored when the text of its own version is not known", () => {
    expect(locate(anchor, own, 3).state).toBe("re-anchored");
  });

  it("finds the one occurrence of the quote among similar sentences", () => {
    const later = pageText(
      page("If its peak gust is above the limit, the crossing waits.", ...version2),
    ).text;
    const located = locate(anchor, later, 3, own);
    expect(located.state).toBe("re-anchored");
    expect(quoted(later, located)).toBe(held);
  });

  it("tells duplicated text apart by the words around it", () => {
    const repeated = "the sailing is held";
    const first = "In fog the sailing is held until the harbour master clears it.";
    const second = "In a gale the sailing is held and the desk calls each passenger.";
    const text = `${first} ${second}`;
    const start = text.lastIndexOf(repeated);
    const mine = anchorAt(text, start, start + repeated.length, 1);
    const later = pageText(page("A new opening line.", first, second)).text;
    const located = locate(mine, later, 2);
    expect(located.state).toBe("re-anchored");
    expect(located.state !== "outdated" && later.slice(located.start - 5, located.start)).toBe(
      "gale ",
    );
  });

  it("is outdated as ambiguous when the words around duplicated text do not tell the copies apart", () => {
    const line = "Check the forecast. The sailing is held. Ring the desk.";
    const mine = anchorOf(line, "The sailing is held.", 1);
    const later = pageText(page(line, line)).text;
    expect(locate(mine, later, 2)).toEqual({ state: "outdated", reason: "ambiguous" });
  });

  it("is outdated when one character of the quote changed", () => {
    const priya = anchorOf(
      version2[2] ?? "",
      "more than 5 knots above the limit cancels the sailing at once",
      2,
    );
    const later = pageText(
      page("A gust that is more than 8 knots above the limit cancels the sailing at once."),
    ).text;
    expect(locate(priya, later, 3)).toEqual({ state: "outdated", reason: "quote_gone" });
  });

  it("is outdated when the quote was deleted", () => {
    const later = pageText(page(version2[0] ?? "", version2[2] ?? "")).text;
    expect(locate(anchor, later, 3, own)).toEqual({ state: "outdated", reason: "quote_gone" });
  });

  it("is found again when only whitespace and formatting around the quote changed", () => {
    const pieces: Piece[] = [
      { text: version2[0] ?? "", block: 0, kind: "prose" },
      { text: "Each vessel has a gust limit in knots, stored as ", block: 1, kind: "prose" },
      { text: "gust_limit_knots", block: 1, kind: "prose" },
      {
        text: ".\n  A new forecast arrives every   hour. If its peak gust for the ",
        block: 1,
        kind: "prose",
      },
      { text: "crossing", block: 1, kind: "prose" },
      {
        text: " is above the limit,\nthe sailing is held and passengers see a notice.",
        block: 1,
        kind: "prose",
      },
    ];
    const later = pageText(pieces).text;
    const located = locate(anchor, later, 3, own);
    expect(located.state).toBe("anchored");
    expect(quoted(later, located)).toBe(held);
  });

  it("never throws, and an empty page leaves every comment outdated", () => {
    expect(locate(anchor, "", 3)).toEqual({ state: "outdated", reason: "quote_gone" });
  });
});

/** A small seeded generator, so every run tests the same cases. */
function generator(seed: number): () => number {
  let state = seed;
  return () => {
    state = (state + 0x6d2b79f5) | 0;
    let mixed = Math.imul(state ^ (state >>> 15), 1 | state);
    mixed = (mixed + Math.imul(mixed ^ (mixed >>> 7), 61 | mixed)) ^ mixed;
    return ((mixed ^ (mixed >>> 14)) >>> 0) / 4_294_967_296;
  };
}

describe("a property of selecting and finding", () => {
  // The alphabet holds repeated letters, spaces, a newline and a decomposed
  // accent, so pages repeat words and need collapsing and normalising.
  const alphabet = ["a", "b", "c", " ", " ", "\n", "e\u0301", "\u00e9", "\u03a9", "."];

  it("finds any selection inside one block of any page at the place it was selected", () => {
    const random = generator(7);
    const pick = (count: number) => Math.floor(random() * count);
    let checked = 0;
    for (let round = 0; round < 500; round += 1) {
      const blocks = Array.from({ length: 1 + pick(4) }, () =>
        Array.from({ length: 1 + pick(180) }, () => alphabet[pick(alphabet.length)]).join(""),
      );
      const pieces = page(...blocks);
      const text = pageText(pieces);
      const piece = pick(blocks.length);
      const raw = blocks[piece] ?? "";
      const start = pick(raw.length + 1);
      const end = start + pick(raw.length - start + 1);
      const made = selectAnchor(text, pieces, { piece, offset: start }, { piece, offset: end }, 4);
      if (made.outcome !== "anchor") continue;
      checked += 1;
      const located = locate(made.anchor, text.text, 4);
      expect(located.state, JSON.stringify({ blocks, start, end })).toBe("anchored");
      if (located.state === "outdated") continue;
      expect(text.text.slice(located.start, located.end)).toBe(made.anchor.quote);
      expect(text.text.slice(0, located.start).endsWith(made.anchor.prefix)).toBe(true);
      expect(text.text.slice(located.end).startsWith(made.anchor.suffix)).toBe(true);
    }
    expect(checked).toBeGreaterThan(200);
  });
});
