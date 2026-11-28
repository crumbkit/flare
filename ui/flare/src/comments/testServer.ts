// A stand-in for flare-server's comment routes, given to the section as the
// global `fetch`, so the tests pass the same JSON through the same schemas
// as the server's answers. Only the tests import it.
import { expect, vi } from "vitest";

export interface WireReply {
  id: string;
  flare: string;
  parent: string;
  author: string;
  at: string;
  deleted: boolean;
  body?: string;
}

export interface WireAnchor {
  quote: string;
  prefix: string;
  suffix: string;
  version: number;
}

export interface WireComment extends Omit<WireReply, "parent"> {
  anchor?: WireAnchor;
  replies: WireReply[];
}

/** How the stand-in answers the next request of one kind. */
export type Answer = "answer" | "drop" | "hold" | 400 | 401 | 404 | 409 | 503;

export const reader = "priya.nair@example.com";
export const owner = "ana.okafor@example.com";

/** The mockup's thread: two comments, the first with two replies. */
export function mockupComments(): WireComment[] {
  const reply = (id: string, author: string, at: string, body: string): WireReply => ({
    id,
    flare: "f1",
    parent: "c1",
    author,
    at,
    deleted: false,
    body,
  });
  return [
    {
      id: "c1",
      flare: "f1",
      author: "tomas.rivera@example.com",
      at: "2026-11-21T16:02:00Z",
      deleted: false,
      body: "This reads well. Can the table also show which vessels carry vehicles?",
      replies: [
        reply("r1", owner, "2026-11-21T16:30:00Z", "Yes. All three do."),
        reply("r2", "jonas.weber@example.com", "2026-11-22T08:05:00Z", "I added the deck plans."),
      ],
    },
    {
      id: "c2",
      flare: "f1",
      author: "jonas.weber@example.com",
      at: "2026-11-22T11:27:00Z",
      deleted: false,
      body: "Do we pay for a night's stay?",
      replies: [],
    },
  ];
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

/**
 * The stand-in, its fetch, and the bodies of the writes it was sent.
 * `versions` holds the reading page's HTML of each version that
 * `/s/f1?version=<n>` answers; any other version answers 404.
 */
export function testServer(
  comments: WireComment[],
  now: string,
  versions: Record<number, string> = {},
) {
  const posts: {
    flare: string;
    parent?: string;
    body: string;
    key: string;
    anchor?: WireAnchor;
  }[] = [];
  const stored = new Map<string, WireComment | (WireReply & { kind: "reply" })>();
  const held: (() => void)[] = [];
  const next: { read: Answer[]; write: Answer[]; page: Answer[] } = {
    read: [],
    write: [],
    page: [],
  };

  function answerWrite(body: (typeof posts)[number]): Response {
    const known = stored.get(body.key);
    if (known) return json(200, known);
    const base = { flare: body.flare, author: reader, at: now, deleted: false, body: body.body };
    const id = `p${stored.size + 1}`;
    if (body.parent === undefined) {
      const anchor = body.anchor === undefined ? {} : { anchor: body.anchor };
      const comment: WireComment = { ...base, id, ...anchor, replies: [] };
      comments.push(comment);
      stored.set(body.key, comment);
      return json(201, { kind: "comment", ...comment });
    }
    const reply: WireReply = { ...base, id, parent: body.parent };
    comments.find((c) => c.id === body.parent)?.replies.push(reply);
    stored.set(body.key, { kind: "reply", ...reply });
    return json(201, { kind: "reply", ...reply });
  }

  async function answer(how: Answer | undefined, respond: () => Response): Promise<Response> {
    switch (how) {
      case undefined:
      case "answer":
        return respond();
      case "drop":
        throw new TypeError("Failed to fetch");
      case "hold":
        await new Promise<void>((resolve) => held.push(resolve));
        return respond();
      default:
        return json(how, { code: "flare::test::refused" });
    }
  }

  const fetch = vi.fn(async (url: string, init?: RequestInit): Promise<Response> => {
    if (init?.method === "POST") {
      const body = JSON.parse(String(init.body));
      posts.push(body);
      return answer(next.write.shift(), () => answerWrite(body));
    }
    const version = /^\/s\/f1\?version=(\d+)$/.exec(url)?.[1];
    if (version !== undefined) {
      const page = versions[Number(version)];
      return answer(next.page.shift(), () =>
        page === undefined
          ? new Response("not found", { status: 404 })
          : new Response(page, { status: 200, headers: { "Content-Type": "text/html" } }),
      );
    }
    expect(url).toBe("/api/comments?flare=f1");
    return answer(next.read.shift(), () => json(200, { flare: "f1", comments }));
  });

  return {
    fetch,
    posts,
    /** Sets how the next reads and the next writes are answered, in order. */
    next,
    /** Answers every held request. */
    release: () => {
      for (const resolve of held.splice(0)) resolve();
    },
  };
}
