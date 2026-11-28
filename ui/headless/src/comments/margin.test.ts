import { describe, expect, it } from "vitest";
import type { Anchor, Located } from "./anchor.ts";
import { arrange } from "./margin.ts";
import { comment, commentId, jonas, priya, tomas } from "./testThread.ts";
import type { Comment, LiveComment } from "./thread.ts";

function anchored(id: string, quote: string, version = 2): LiveComment {
  const anchor: Anchor = { quote, prefix: "", suffix: "", version };
  const made = comment(id, priya, `on ${quote}`);
  if (made.deleted) throw new Error("the builder made a deleted comment");
  return { ...made, anchor };
}

// The page text holds "first" at 10 and "second" at 40; "gone" is not on it.
const places: Record<string, Located> = {
  first: { state: "anchored", start: 10, end: 15 },
  second: { state: "re-anchored", start: 40, end: 46 },
  gone: { state: "outdated", reason: "quote_gone" },
};
const place = (anchor: Anchor) => places[anchor.quote];

describe("where each comment shows", () => {
  const thread = [
    comment("c1", tomas, "On the whole page."),
    anchored("c2", "second"),
    anchored("c3", "gone"),
    anchored("c4", "first"),
    comment("c5", jonas, "Also on the whole page."),
  ];

  it("puts marked comments in the margin in page order, then outdated ones in the order they were made", () => {
    const { margin, bottom } = arrange(thread, place, false);
    expect(margin.map((card) => [card.comment.id, card.state])).toEqual([
      ["c4", "anchored"],
      ["c2", "re-anchored"],
      ["c3", "outdated"],
    ]);
    expect(bottom.map((c) => c.id)).toEqual(["c1", "c5"]);
  });

  it("on a phone keeps marked comments by their text and shows outdated ones in Comments at the end of the page", () => {
    const { margin, bottom } = arrange(thread, place, true);
    expect(margin.map((card) => card.comment.id)).toEqual(["c4", "c2"]);
    expect(bottom.map((c) => c.id)).toEqual(["c1", "c3", "c5"]);
  });

  it("gives a marked card the range of its text", () => {
    const [first] = arrange(thread, place, false).margin;
    expect(first?.state === "anchored" && [first.start, first.end]).toEqual([10, 15]);
  });

  it("keeps a comment whose text could not be searched, with no mark and no note, after the marked ones", () => {
    const unknown = arrange([anchored("c6", "first")], () => undefined, false);
    expect(unknown.margin.map((card) => [card.comment.id, card.state])).toEqual([
      ["c6", "unplaced"],
    ]);
  });

  it("leaves a deleted comment, whose anchor the delete erased, in Comments at the end of the page", () => {
    const deleted: Comment = { kind: "comment", id: commentId("c7"), deleted: true, replies: [] };
    const { margin, bottom } = arrange([deleted], place, false);
    expect(margin).toEqual([]);
    expect(bottom).toEqual([deleted]);
  });
});
