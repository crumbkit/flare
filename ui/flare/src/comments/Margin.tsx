// The cards on page text, as artboards 08 to 13 of the comment parts mockup
// (UBmgioBRkLLV9B7YMxdhEU, version 1795819564-9bd4) draw them. On a wide
// page they show in the margin beside the article: the card composing a
// comment on selected text first, then each comment whose text is on the
// page in page order, then those whose text is gone. On a phone each shows
// under the paragraph that holds its text, and a comment whose text is gone
// shows in Comments at the end of the page.
//
// On a wide page each card sits level with its text (level.ts), and the
// composing card takes its place among the others in the order of the text.
import { type Anchor, type Located, locate, selectAnchor } from "@flare/headless/comments/anchor";
import { type Arranged, arrange } from "@flare/headless/comments/margin";
import { closesPassage, marginLabel } from "@flare/headless/comments/passage";
import type { Comment, Posted, Thread } from "@flare/headless/comments/thread";
import { useQueries } from "@tanstack/react-query";
import {
  type ReactNode,
  useCallback,
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import { createPortal } from "react-dom";
import { versionTextQuery } from "../api/versions.ts";
import { type Clock, CommentCard } from "./cards.tsx";
import type { ReadingHost } from "./host.ts";
import { PLACE, useLevelCards } from "./level.ts";
import { useNarrow } from "./narrow.ts";
import { blockAt, clearMarks, markRange, pointAt, readPage } from "./pageDom.ts";
import { PassageCard } from "./passage.tsx";

/**
 * A count that rises each time the reading script finishes, because
 * colouring code replaces the text nodes of each code block.
 */
function usePageEpoch(): number {
  const [epoch, setEpoch] = useState(0);
  useEffect(() => {
    const observer = new MutationObserver(() => setEpoch((count) => count + 1));
    observer.observe(document.body, { attributes: true, attributeFilter: ["data-reading"] });
    return () => observer.disconnect();
  }, []);
  return epoch;
}

export interface Placement extends Arranged {
  readonly narrow: boolean;
  /** The card of each comment on page text, wherever it shows. */
  readonly cards: ReadonlyMap<string, Arranged["margin"][number]>;
}

/**
 * Where each comment of `thread` shows on `page`. A comment made on another
 * version needs that version's text to tell whether its text moved; until
 * the text loads, or when it does not, a comment whose quote is on the page
 * shows with no mark and no note.
 */
export function usePlacement(
  flare: string,
  page: ReadingHost | undefined,
  thread: Thread | undefined,
): Placement {
  const narrow = useNarrow();
  const epoch = usePageEpoch();
  const viewed = page?.version;
  const versions = [
    ...new Set(
      (thread ?? []).flatMap((comment) =>
        !comment.deleted && comment.anchor !== undefined && comment.anchor.version !== viewed
          ? [comment.anchor.version]
          : [],
      ),
    ),
  ];
  const texts = useQueries({
    queries: versions.map((version) => ({
      ...versionTextQuery(flare, version),
      enabled: page !== undefined,
    })),
  });
  const own = new Map(versions.map((version, at) => [version, texts[at]]));
  // The page text does not change when marks wrap it, so it is read again
  // only when the reading script finishes.
  // biome-ignore lint/correctness/useExhaustiveDependencies: the epoch says when the article's text nodes changed.
  const text = useMemo(() => (page ? readPage(page.article).text.text : ""), [page, epoch]);
  if (page === undefined || thread === undefined) {
    return { margin: [], bottom: thread ?? [], narrow, cards: new Map() };
  }
  const place = (anchor: Anchor): Located | undefined => {
    if (anchor.version === page.version) return locate(anchor, text, page.version);
    const loaded = own.get(anchor.version);
    // When the comment's own version does not load, the search alone
    // places it, so a quote found once shows as moved.
    if (loaded?.isError) return locate(anchor, text, page.version);
    const before = loaded?.data;
    const located = locate(anchor, text, page.version, before);
    return before === undefined && located.state !== "outdated" ? undefined : located;
  };
  const arranged = arrange(thread, place, narrow);
  const all = narrow ? arrange(thread, place, false).margin : arranged.margin;
  return {
    ...arranged,
    narrow,
    cards: new Map(all.map((card) => [card.comment.id, card])),
  };
}

/** The open card on selected text, and whether the reader's selection still shows it. */
interface Open {
  readonly anchor: Anchor;
  readonly start: number;
  readonly end: number;
  readonly live: boolean;
}

/** Tracks the reader's selection in the article and the card it opens. */
function usePassage(page: ReadingHost) {
  const [open, setOpen] = useState<Open | undefined>(undefined);
  const text = useRef("");
  const card = useRef<HTMLDivElement | null>(null);
  useEffect(() => {
    const changed = () => {
      const selection = document.getSelection();
      const range = selection && selection.rangeCount > 0 ? selection.getRangeAt(0) : undefined;
      if (range && !range.collapsed && page.article.contains(range.commonAncestorContainer)) {
        if (text.current !== "") return;
        const read = readPage(page.article);
        const start = pointAt(read, range.startContainer, range.startOffset);
        const end = pointAt(read, range.endContainer, range.endOffset);
        const made =
          start && end ? selectAnchor(read.text, read.pieces, start, end, page.version) : undefined;
        if (made?.outcome === "anchor") {
          setOpen((now) =>
            now?.start === made.start && now.end === made.end
              ? now
              : { anchor: made.anchor, start: made.start, end: made.end, live: true },
          );
          return;
        }
      }
      const focused = card.current?.contains(document.activeElement) ?? false;
      if (closesPassage("selection-cleared", text.current, focused)) setOpen(undefined);
      else setOpen((now) => (now === undefined || !now.live ? now : { ...now, live: false }));
    };
    document.addEventListener("selectionchange", changed);
    return () => document.removeEventListener("selectionchange", changed);
  }, [page]);
  const close = useCallback(() => {
    text.current = "";
    setOpen(undefined);
  }, []);
  const onText = useCallback((now: string) => {
    text.current = now;
  }, []);
  const onFocus = useCallback(
    () => setOpen((now) => (now === undefined || !now.live ? now : { ...now, live: false })),
    [],
  );
  return { open, close, onText, onFocus, card };
}

/** The article's child that holds `element`, which a phone's cards follow. */
function topLevel(article: Element, element: Element | undefined): Element | undefined {
  let at = element;
  while (at !== undefined && at.parentElement !== article) at = at.parentElement ?? undefined;
  return at;
}

/** A margin inserted after `after`, for the cards under one paragraph on a phone. */
function Slot({
  after,
  label,
  children,
}: {
  readonly after: Element;
  readonly label: string;
  readonly children: ReactNode;
}) {
  const [aside] = useState(() => {
    const made = document.createElement("aside");
    made.className = "margin";
    return made;
  });
  useLayoutEffect(() => {
    after.after(aside);
    return () => aside.remove();
  }, [after, aside]);
  useLayoutEffect(() => aside.setAttribute("aria-label", label), [aside, label]);
  return createPortal(children, aside);
}

/** The start and end of some page text. */
type Span = readonly [number, number];

export interface MarginProps {
  readonly page: ReadingHost;
  readonly placement: Placement;
  readonly flare: string;
  readonly owner: string;
  readonly clock: Clock;
  readonly onAdd: (posted: Posted) => void;
}

export function Margin({ page, placement, flare, owner, clock, onAdd }: MarginProps) {
  const passage = usePassage(page);
  const { open } = passage;
  const marked = placement.margin.flatMap((card): Span[] =>
    card.state === "anchored" || card.state === "re-anchored" ? [[card.start, card.end]] : [],
  );
  const selected: Span | undefined = open && !open.live ? [open.start, open.end] : undefined;
  // The marks are made again only when the ranges change, which the key
  // says; the ref holds the ranges the key was made from.
  const markKey = JSON.stringify([marked, selected]);
  const ranges = useRef({ marked, selected });
  ranges.current = { marked, selected };
  // biome-ignore lint/correctness/useExhaustiveDependencies: the key says when the ranges changed.
  useLayoutEffect(() => {
    const now = ranges.current;
    clearMarks(page.article);
    for (const [start, end] of now.marked) markRange(readPage(page.article), start, end, "anchor");
    if (now.selected) markRange(readPage(page.article), now.selected[0], now.selected[1], "sel");
    return () => clearMarks(page.article);
  }, [page, markKey]);

  // While the card is empty the selection stays the reader's, so copying
  // still works, and the stylesheet draws it in mark.sel's accent tint.
  const live = open?.live === true;
  useLayoutEffect(() => {
    page.article.classList.toggle("composing", live);
    return () => page.article.classList.remove("composing");
  }, [page, live]);
  useLevelCards(page, !placement.narrow);

  const sent = (posted: Comment) => {
    onAdd(posted);
    passage.close();
    document.getSelection()?.removeAllRanges();
  };
  const composing = open && (
    <div ref={passage.card} key={`${open.start}:${open.end}`}>
      <PassageCard
        flare={flare}
        owner={owner}
        anchor={open.anchor}
        onText={passage.onText}
        onFocus={passage.onFocus}
        onClose={passage.close}
        onSent={sent}
      />
    </div>
  );
  const card = (each: Placement["margin"][number]) => (
    <CommentCard
      key={each.comment.id}
      comment={each.comment}
      flare={flare}
      owner={owner}
      clock={clock}
      onReply={onAdd}
      card={each}
      viewed={page.version}
    />
  );

  useLayoutEffect(() => {
    page.margin.setAttribute(
      "aria-label",
      marginLabel({ composing: open !== undefined, narrow: false }),
    );
  }, [page, open]);
  if (!placement.narrow) {
    const places: { key: string; start: number | undefined; node: ReactNode }[] =
      placement.margin.map((each) => ({
        key: each.comment.id,
        start: "start" in each ? each.start : undefined,
        node: card(each),
      }));
    if (open) {
      const after = places.findIndex(
        (each) => each.start === undefined || each.start >= open.start,
      );
      places.splice(after < 0 ? places.length : after, 0, {
        key: "composing",
        start: open.start,
        node: composing,
      });
    }
    return createPortal(
      places.map(({ key, start, node }) => (
        <div className={PLACE} key={key} data-start={start}>
          {node}
        </div>
      )),
      page.margin,
    );
  }

  const read = readPage(page.article);
  const hosts = new Map<Element, ReactNode[]>();
  const hostOf = (at: number) => topLevel(page.article, blockAt(read, at));
  const add = (host: Element | undefined, node: ReactNode) => {
    if (host === undefined) return;
    hosts.set(host, [...(hosts.get(host) ?? []), node]);
  };
  if (open) add(hostOf(open.start), composing);
  for (const each of placement.margin) if ("start" in each) add(hostOf(each.start), card(each));
  const composingHost = open ? hostOf(open.start) : undefined;
  return (
    <>
      {[...hosts].map(([host, nodes]) => (
        <Slot
          key={[...page.article.children].filter((child) => !child.matches("aside")).indexOf(host)}
          after={host}
          label={marginLabel({ composing: host === composingHost, narrow: true })}
        >
          {nodes}
        </Slot>
      ))}
    </>
  );
}
