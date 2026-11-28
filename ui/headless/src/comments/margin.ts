// Where each comment of a thread shows on the version a reader views. A
// comment whose text is on the page shows beside it, in page order. One
// whose text is gone (outdated) or could not be searched (unplaced) has no
// text to sit beside: on a wide page it follows the marked cards in the
// margin, and on a phone it shows in Comments at the end of the page, in
// the order the comments were made (artboards 11 and 13). A comment on the
// whole page, and a deleted one, show in Comments at the end of the page.
import type { Anchor, Located } from "./anchor.ts";
import type { Comment, LiveComment, Thread } from "./thread.ts";

export type Card =
  | {
      readonly comment: LiveComment;
      readonly anchor: Anchor;
      readonly state: "anchored" | "re-anchored";
      readonly start: number;
      readonly end: number;
    }
  | {
      readonly comment: LiveComment;
      readonly anchor: Anchor;
      readonly state: "outdated" | "unplaced";
    };

export interface Arranged {
  /** The cards beside the page text, marked ones in page order first. */
  readonly margin: readonly Card[];
  /** The comments of Comments at the end of the page, in the order they were made. */
  readonly bottom: readonly Comment[];
}

/**
 * Places each comment of `thread`. `place` gives where an anchor's text is
 * on the viewed version, or undefined when the page text is not known.
 */
export function arrange(
  thread: Thread,
  place: (anchor: Anchor) => Located | undefined,
  narrow: boolean,
): Arranged {
  const marked: Card[] = [];
  const unmarked: Card[] = [];
  const bottom: Comment[] = [];
  for (const comment of thread) {
    if (comment.deleted || comment.anchor === undefined) {
      bottom.push(comment);
      continue;
    }
    const { anchor } = comment;
    const located = place(anchor);
    if (located === undefined) {
      unmarked.push({ comment, anchor, state: "unplaced" });
    } else if (located.state === "outdated") {
      unmarked.push({ comment, anchor, state: "outdated" });
    } else {
      marked.push({ comment, anchor, ...located });
    }
    if (narrow && located?.state !== "anchored" && located?.state !== "re-anchored") {
      bottom.push(comment);
    }
  }
  marked.sort((a, b) => ("start" in a && "start" in b ? a.start - b.start : 0));
  return { margin: narrow ? marked : [...marked, ...unmarked], bottom };
}

/** One margin card as the page measures it. */
export interface Measured {
  /**
   * The top of the card's text, measured from the margin's top, or
   * undefined for a card with no text on the page.
   */
  readonly want: number | undefined;
  /** The card's height. */
  readonly height: number;
}

/**
 * The space above each card in a margin whose cards stand `gap` apart, so
 * that each card sits level with its text. A card that would overlap the
 * card above it is pushed down, so the cards keep their order, and a card
 * with no text follows the card above it.
 */
export function stack(cards: readonly Measured[], gap: number): number[] {
  let next = 0;
  return cards.map(({ want, height }, at) => {
    const natural = at === 0 ? 0 : next + gap;
    const space = Math.max(0, (want ?? natural) - natural);
    next = natural + space + height;
    return space;
  });
}
