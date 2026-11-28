// Puts each card of a wide page's margin level with its text, as artboards
// 08, 10 and 11 draw it: the space above each card is set from where its
// text is, a card that would overlap the one above it is pushed down, and a
// card with no text on the page follows the one above it. The space is set
// again after each render and whenever the article or a card changes size.
import { stack } from "@flare/headless/comments/margin";
import { useLayoutEffect } from "react";
import type { ReadingHost } from "./host.ts";
import { readPage, textTop } from "./pageDom.ts";

/** The class of the box around each card in the margin. */
export const PLACE = "place";

/** The page text offset a card's box holds in `data-start`, if any. */
function startOf(element: HTMLElement): number | undefined {
  const start = Number.parseInt(element.dataset.start ?? "", 10);
  return Number.isInteger(start) ? start : undefined;
}

function level(page: ReadingHost): void {
  const places = [...page.margin.children].filter(
    (child): child is HTMLElement =>
      child instanceof HTMLElement && child.classList.contains(PLACE),
  );
  const read = readPage(page.article);
  const origin = page.margin.getBoundingClientRect().top;
  const gap = Number.parseFloat(getComputedStyle(page.margin).rowGap) || 0;
  const measured = places.map((place) => {
    const start = startOf(place);
    const top = start === undefined ? undefined : textTop(read, start);
    return {
      want: top === undefined ? undefined : top - origin,
      height: place.getBoundingClientRect().height,
    };
  });
  stack(measured, gap).forEach((space, at) => {
    const place = places[at];
    if (place !== undefined) place.style.marginTop = space > 0 ? `${space}px` : "";
  });
}

/** Keeps the cards in `page`'s margin level with their text while `wide`. */
export function useLevelCards(page: ReadingHost, wide: boolean): void {
  useLayoutEffect(() => {
    if (!wide) return;
    const run = () => level(page);
    run();
    if (typeof ResizeObserver === "undefined") return;
    const observer = new ResizeObserver(run);
    observer.observe(page.article);
    for (const child of page.margin.children) observer.observe(child);
    return () => observer.disconnect();
  });
}
