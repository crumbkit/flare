// The card on highlighted text in each state that artboards 08 to 13 of the
// comment parts mockup (UBmgioBRkLLV9B7YMxdhEU, version 1795819564-9bd4)
// draw, on a reading page that the server's markup stands in for, with
// flare-server's routes stood in for by testServer.ts.
import { selectAnchor } from "@flare/headless/comments/anchor";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { describe, expect, it, vi } from "vitest";
import { readingHost } from "./host.ts";
import { readPage } from "./pageDom.ts";
import { CommentsSection } from "./Section.tsx";
import { owner, testServer, type WireAnchor, type WireComment } from "./testServer.ts";

const NOW = "2026-11-23T12:00:00Z";
const held = "If its peak gust for the crossing is above the limit, the sailing is held";
const knots = "more than 5 knots above the limit cancels the sailing at once";

const version2 = `<h2>When a sailing is held</h2>
<p>Each vessel has a gust limit in knots, stored as <code>gust_limit_knots</code>. A new forecast arrives every hour. ${held} and passengers see a notice that says the sailing is under review.</p>
<p>A gust that is ${knots}, with no review.</p>`;
const version3 = `<h2>When a sailing is held</h2>
<p>Forecasts come from the mid-channel buoy, which reports every ten minutes.</p>
<p>Each vessel has a gust limit in knots, stored as <code>gust_limit_knots</code>. A new forecast arrives every hour. ${held} and passengers see a notice that says the sailing is under review.</p>
<p>A gust that is more than 8 knots above the limit cancels the sailing at once, with no review.</p>`;

/** The reading page the server writes around the Markdown `body` of `version`. */
function readingPage(body: string, version: number): string {
  return `<main class="screen"><article class="doc"><div class="share-meta"><div class="line">Shared by <b>${owner}</b> · version ${version}</div></div><h1>Sailings</h1>\n${body}</article><section id="comments" class="comments" data-flare="f1" data-version="${version}" data-owner="${owner}"></section></main>`;
}

/** The anchor that selecting `quote` on `body` makes on `version`. */
function anchorIn(body: string, quote: string, version: number): WireAnchor {
  const scratch = document.createElement("article");
  scratch.innerHTML = body;
  const page = readPage(scratch);
  const start = page.text.text.indexOf(quote);
  const placeOf = (at: number) => page.text.places[at] ?? { piece: 0, offset: 0 };
  const made = selectAnchor(
    page.text,
    page.pieces,
    placeOf(start),
    placeOf(start + quote.length),
    version,
  );
  if (made.outcome !== "anchor") throw new Error(made.outcome);
  return made.anchor;
}

function comments(): WireComment[] {
  const base = { flare: "f1", deleted: false, replies: [] };
  return [
    {
      ...base,
      id: "c1",
      author: "mei.tanaka@example.com",
      at: "2026-11-22T09:14:00Z",
      body: "Which forecast point do we read for the crossing, the harbour mouth or mid-channel?",
      anchor: anchorIn(version2, held, 2),
    },
    {
      ...base,
      id: "c2",
      author: "priya.nair@example.com",
      at: "2026-11-23T10:41:00Z",
      body: "Does the 5 knot margin apply to the Curlew too?",
      anchor: anchorIn(version2, knots, 2),
    },
    {
      ...base,
      id: "c3",
      author: "tomas.rivera@example.com",
      at: "2026-11-23T11:00:00Z",
      body: "This reads well.",
    },
  ];
}

interface Shown {
  readonly version?: number;
  readonly narrow?: boolean;
  readonly thread?: WireComment[];
  readonly pages?: Record<number, string>;
}

function show({ version = 2, narrow = false, thread = comments(), pages }: Shown = {}) {
  document.body.innerHTML = readingPage(version === 2 ? version2 : version3, version);
  vi.stubGlobal("matchMedia", (query: string) => ({
    matches: narrow,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  }));
  const server = testServer(thread, NOW, pages ?? { 2: readingPage(version2, 2) });
  vi.stubGlobal("fetch", server.fetch);
  const host = readingHost(document);
  if (host === undefined) throw new Error("the page has no article");
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <CommentsSection
        flare="f1"
        owner={owner}
        page={host}
        now={() => Date.parse(NOW)}
        timeZone="UTC"
      />
    </QueryClientProvider>,
    { container: host.section },
  );
  return Object.assign(server, { host });
}

const margin = (name: string) => screen.getByRole("complementary", { name });
const marks = (kind: string) =>
  [...document.querySelectorAll(`mark.${kind}`)].map((mark) => mark.textContent).join("");

/** Selects `quote` in the article, as a reader's drag does. */
function select(quote: string, within: Element = document.body) {
  const walker = document.createTreeWalker(within, NodeFilter.SHOW_TEXT);
  for (let node = walker.nextNode(); node !== null; node = walker.nextNode()) {
    const at = node.textContent?.indexOf(quote) ?? -1;
    if (at < 0) continue;
    const range = document.createRange();
    range.setStart(node, at);
    range.setEnd(node, at + quote.length);
    const selection = window.getSelection();
    selection?.removeAllRanges();
    selection?.addRange(range);
    act(() => {
      document.dispatchEvent(new Event("selectionchange"));
    });
    return;
  }
  throw new Error(`${quote} is not on the page`);
}

function clearSelection() {
  window.getSelection()?.removeAllRanges();
  act(() => {
    document.dispatchEvent(new Event("selectionchange"));
  });
}

describe("artboard 08: selecting, then composing on highlighted text", () => {
  it("shows an anchored comment beside its text with its quote, and marks the text", async () => {
    show();
    const aside = await waitFor(() => margin("Comments on this page's text"));
    const card = await within(aside).findByText("mei.tanaka@example.com");
    const article = card.closest(".card");
    if (!(article instanceof HTMLElement)) throw new Error("no card");
    expect(within(article).getByText(`"${held}"`)).toHaveClass("quote");
    expect(within(article).getByRole("textbox", { name: "Reply to mei.tanaka@example.com" }));
    await waitFor(() => expect(marks("anchor")).toBe(`${held}${knots}`));
    const bottom = document.querySelector("section#comments");
    expect(bottom).toHaveTextContent("3 comments");
    expect(bottom).not.toHaveTextContent("Which forecast point");
    expect(bottom).toHaveTextContent("This reads well.");
  });

  it("opens the card with the quote and an empty field when text is selected, and leaves focus in the page", async () => {
    show({ thread: [] });
    await screen.findByText("None yet");
    select("forecast arrives every hour");
    const aside = margin("Comment on this passage");
    const form = within(aside).getByRole("textbox", { name: "Your comment on this text" });
    expect(form).toHaveValue("");
    expect(within(aside).getByText('"forecast arrives every hour"')).toHaveClass("quote");
    expect(within(aside).getByRole("button", { name: "Cancel" })).toHaveClass("ghost");
    expect(within(aside).getByRole("button", { name: "Comment" })).toHaveAttribute(
      "aria-disabled",
      "true",
    );
    expect(document.activeElement).not.toBe(form);
  });

  it("closes the empty card when the selection clears, and keeps one that holds text", async () => {
    show({ thread: [] });
    await screen.findByText("None yet");
    select("forecast arrives every hour");
    clearSelection();
    expect(screen.queryByRole("textbox", { name: "Your comment on this text" })).toBeNull();
    select("forecast arrives every hour");
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your comment on this text" }),
      "Why?",
    );
    clearSelection();
    expect(screen.getByRole("textbox", { name: "Your comment on this text" })).toHaveValue("Why?");
  });

  it("closes from the field on Escape and on Cancel", async () => {
    show({ thread: [] });
    await screen.findByText("None yet");
    select("forecast arrives every hour");
    await userEvent.type(screen.getByRole("textbox", { name: "Your comment on this text" }), "x");
    await userEvent.keyboard("{Escape}");
    expect(screen.queryByRole("textbox", { name: "Your comment on this text" })).toBeNull();
    select("forecast arrives every hour");
    await userEvent.click(screen.getByRole("button", { name: "Cancel" }));
    expect(screen.queryByRole("textbox", { name: "Your comment on this text" })).toBeNull();
  });

  it("visits the field, Cancel and Comment in that order", async () => {
    show({ thread: [] });
    await screen.findByText("None yet");
    select("forecast arrives every hour");
    await userEvent.click(screen.getByRole("textbox", { name: "Your comment on this text" }));
    await userEvent.keyboard("x");
    const aside = margin("Comment on this passage");
    await userEvent.tab();
    expect(document.activeElement).toBe(within(aside).getByRole("button", { name: "Cancel" }));
    await userEvent.tab();
    expect(document.activeElement).toBe(within(aside).getByRole("button", { name: "Comment" }));
  });

  it("opens no card for a selection across two paragraphs", async () => {
    show({ thread: [] });
    await screen.findByText("None yet");
    const [first, second] = document.querySelectorAll("article.doc p");
    const range = document.createRange();
    range.setStart(first?.lastChild ?? document.body, 3);
    range.setEnd(second?.firstChild ?? document.body, 5);
    window.getSelection()?.removeAllRanges();
    window.getSelection()?.addRange(range);
    act(() => {
      document.dispatchEvent(new Event("selectionchange"));
    });
    expect(screen.queryByRole("textbox", { name: "Your comment on this text" })).toBeNull();
  });

  it("sends the anchor with the comment, and the sent comment shows beside its marked text", async () => {
    const server = show({ thread: [] });
    await screen.findByText("None yet");
    select(knots);
    await userEvent.type(
      screen.getByRole("textbox", { name: "Your comment on this text" }),
      "Does the 5 knot margin apply to the Curlew too?",
    );
    await userEvent.keyboard("{Control>}{Enter}{/Control}");
    await waitFor(() => expect(server.posts).toHaveLength(1));
    expect(server.posts[0]?.anchor).toEqual(anchorIn(version2, knots, 2));
    const aside = await waitFor(() => margin("Comments on this page's text"));
    expect(await within(aside).findByText("Does the 5 knot margin apply to the Curlew too?"));
    expect(screen.queryByRole("textbox", { name: "Your comment on this text" })).toBeNull();
    await waitFor(() => expect(marks("anchor")).toBe(knots));
  });
});

describe("artboard 09: phone, composing on highlighted text", () => {
  it("opens the card under the paragraph that holds the selection and marks the text once the field has focus", async () => {
    show({ narrow: true, thread: [] });
    await screen.findByText("None yet");
    select(knots);
    const paragraph = [...document.querySelectorAll("article.doc p")].at(-1);
    const slot = paragraph?.nextElementSibling;
    expect(slot).toHaveAttribute("aria-label", "Comment on this passage");
    if (!(slot instanceof HTMLElement)) throw new Error("no slot after the paragraph");
    const field = within(slot).getByRole("textbox", {
      name: "Your comment on this text",
    });
    await userEvent.click(field);
    await waitFor(() => expect(marks("sel")).toBe(knots));
  });
});

describe("artboard 10: moved, on a later version", () => {
  it("moves the mark and the card with the text and says the comment was made on version 2", async () => {
    show({ version: 3 });
    const aside = await waitFor(() => margin("Comments on this page's text"));
    const note = await within(aside).findByText(
      "Made on version 2. The text has moved since then.",
    );
    const card = note.closest(".card");
    expect(note).toHaveClass("a-note");
    expect(note.previousElementSibling).toHaveTextContent(`"${held}"`);
    expect(card).toHaveTextContent("mei.tanaka@example.com");
    await waitFor(() => expect(marks("anchor")).toBe(held));
  });

  it("shows the comment with no mark and no note when its own version's text does not load", async () => {
    show({ version: 3, pages: {} });
    const aside = await waitFor(() => margin("Comments on this page's text"));
    await within(aside).findByText(
      "Which forecast point do we read for the crossing, the harbour mouth or mid-channel?",
    );
    expect(within(aside).queryByText(/The text has moved/)).toBeNull();
    expect(marks("anchor")).toBe("");
  });
});

describe("artboard 11: outdated, on a later version", () => {
  it("keeps the quote under a line saying the text changed on this version, marks nothing, and follows the anchored cards", async () => {
    show({ version: 3 });
    const aside = await waitFor(() => margin("Comments on this page's text"));
    const note = await within(aside).findByText("The quoted text changed on version 3.");
    expect(note).toHaveClass("a-note", "outdated");
    expect(note.nextElementSibling).toHaveTextContent("Commented on version 2");
    expect(note.nextElementSibling).toHaveClass("lab");
    expect(note.nextElementSibling?.nextElementSibling).toHaveTextContent(`"${knots}"`);
    const cards = [...aside.querySelectorAll(".card")].map((card) => card.textContent ?? "");
    expect(cards[0]).toContain("mei.tanaka@example.com");
    expect(cards[1]).toContain("priya.nair@example.com");
    await waitFor(() => expect(marks("anchor")).toBe(held));
  });
});

describe("artboard 12: phone, moved", () => {
  it("puts the moved card under the paragraph that now holds its text", async () => {
    show({ version: 3, narrow: true });
    await screen.findByText("Made on version 2. The text has moved since then.");
    const paragraph = [...document.querySelectorAll("article.doc p")].find((p) =>
      p.textContent?.includes(held),
    );
    const slot = paragraph?.nextElementSibling;
    expect(slot).toHaveAttribute("aria-label", "Comments on this passage");
    expect(slot).toHaveTextContent("mei.tanaka@example.com");
  });
});

describe("artboard 13: phone, outdated", () => {
  it("shows the outdated card in Comments at the end of the page, in the order the comments were made", async () => {
    show({ version: 3, narrow: true });
    const bottom = document.querySelector("section#comments");
    if (!(bottom instanceof HTMLElement)) throw new Error("no section");
    await within(bottom).findByText("The quoted text changed on version 3.");
    expect(bottom).toHaveTextContent("3 comments");
    const cards = [...bottom.querySelectorAll(".card")].map((card) => card.textContent ?? "");
    expect(cards[0]).toContain("priya.nair@example.com");
    expect(cards[0]).toContain(`"${knots}"`);
    expect(cards[1]).toContain("This reads well.");
  });
});
