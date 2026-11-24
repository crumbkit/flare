import { QueryClient } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import { z } from "zod";
import contract from "../../../../schema/flare-api.schema.json" with { type: "json" };
import { ApiError } from "./client.ts";
import {
  commentsQuery,
  postComment,
  wireComment,
  wirePosted,
  wireReply,
  wireThread,
} from "./comments.ts";

/** The property names and the required names of one contract type. */
function shapeOf(schema: unknown): { properties: string[]; required: string[] } {
  const parsed = z
    .object({ properties: z.record(z.string(), z.unknown()), required: z.array(z.string()) })
    .parse(schema);
  return {
    properties: Object.keys(parsed.properties).sort(),
    required: [...parsed.required].sort(),
  };
}

function json(status: number, body: unknown): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json" },
  });
}

const thread = {
  flare: "f1",
  comments: [
    {
      id: "c1",
      flare: "f1",
      author: "tomas.rivera@example.com",
      at: "2026-11-21T16:02:00Z",
      deleted: false,
      body: "Can the table also show which vessels carry vehicles?",
      replies: [
        {
          id: "r1",
          flare: "f1",
          parent: "c1",
          author: "ana.okafor@example.com",
          at: "2026-11-21T16:30:00Z",
          deleted: false,
          body: "Yes. All three do.",
        },
        {
          id: "r2",
          flare: "f1",
          parent: "c1",
          author: "jonas.weber@example.com",
          at: "2026-11-22T08:05:00Z",
          deleted: true,
        },
      ],
    },
    {
      id: "c2",
      flare: "f1",
      author: "jonas.weber@example.com",
      at: "2026-11-22T11:27:00Z",
      edited: "2026-11-22T11:30:00Z",
      deleted: false,
      body: "Do we pay for a night's stay?",
      replies: [],
    },
  ],
};

describe("the comment schemas", () => {
  it("have the properties and the required fields of the contract's comment types", () => {
    const defs = contract.$defs;
    for (const [name, schema] of [
      ["Comment", wireComment],
      ["Reply", wireReply],
      ["CommentThread", wireThread],
    ] as const) {
      expect(shapeOf(z.toJSONSchema(schema)), name).toEqual(shapeOf(defs[name]));
    }
  });

  it("tell a posted comment from a posted reply by the contract's kinds", () => {
    const kinds = contract.$defs.CommentOrReply.oneOf.map((one) => one.properties.kind.const);
    expect(kinds).toEqual(["comment", "reply"]);
    const { replies: _, ...comment } = thread.comments[1] ?? {};
    expect(wirePosted.parse({ kind: "comment", ...comment, replies: [] }).kind).toBe("comment");
    const reply = thread.comments[0]?.replies[0];
    expect(wirePosted.parse({ kind: "reply", ...reply }).kind).toBe("reply");
  });
});

describe("the comments query", () => {
  it("reads the flare's thread with each author named by their identifier", async () => {
    const fetch = vi.fn(async () => json(200, thread));
    vi.stubGlobal("fetch", fetch);
    const loaded = await new QueryClient().fetchQuery(commentsQuery("f1"));
    expect(fetch).toHaveBeenCalledWith(
      "/api/comments?flare=f1",
      expect.objectContaining({ credentials: "same-origin" }),
    );
    expect(loaded).toEqual([
      {
        kind: "comment",
        id: "c1",
        author: { name: "tomas.rivera@example.com" },
        body: "Can the table also show which vessels carry vehicles?",
        at: "2026-11-21T16:02:00Z",
        deleted: false,
        replies: [
          {
            kind: "reply",
            id: "r1",
            parent: "c1",
            author: { name: "ana.okafor@example.com" },
            body: "Yes. All three do.",
            at: "2026-11-21T16:30:00Z",
          },
        ],
      },
      {
        kind: "comment",
        id: "c2",
        author: { name: "jonas.weber@example.com" },
        body: "Do we pay for a night's stay?",
        at: "2026-11-22T11:27:00Z",
        deleted: false,
        replies: [],
      },
    ]);
  });

  it("leaves out a deleted comment with no replies and marks one that has replies", async () => {
    const deleted = {
      flare: "f1",
      comments: [
        { ...thread.comments[1], body: undefined, deleted: true },
        { ...thread.comments[0], id: "c3", body: undefined, deleted: true },
      ],
    };
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => json(200, deleted)),
    );
    const loaded = await new QueryClient().fetchQuery(commentsQuery("f1"));
    expect(
      loaded.map((c) => [c.id, c.deleted, "author" in c, "body" in c, c.replies.length]),
    ).toEqual([["c3", true, false, false, 1]]);
  });

  it("throws a refusal for a 404, no answer for a network failure and malformed for a bad body", async () => {
    const cases: [() => Promise<Response>, string][] = [
      [async () => json(404, { code: "flare::shares::not_found" }), "refused"],
      [async () => json(403, { code: "flare::comments::not_author" }), "refused"],
      [async () => json(503, { code: "flare::comments::unavailable" }), "failed"],
      [async () => json(401, { code: "flare::auth::sign_in_needed" }), "signed-out"],
      [async () => json(400, { code: "flare::comments::invalid" }), "invalid"],
      [
        async () => {
          throw new TypeError("Failed to fetch");
        },
        "no-answer",
      ],
      [async () => json(200, { flare: "f1" }), "malformed"],
    ];
    for (const [answer, kind] of cases) {
      vi.stubGlobal("fetch", vi.fn(answer));
      const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
      const failure = await client.fetchQuery(commentsQuery("f1")).catch((error: unknown) => error);
      expect(failure, kind).toBeInstanceOf(ApiError);
      expect(failure instanceof ApiError && failure.kind, kind).toBe(kind);
    }
  });
});

describe("posting a comment", () => {
  it("sends the flare, the parent, the text and the key as JSON and returns the post", async () => {
    const reply = { kind: "reply", ...thread.comments[0]?.replies[0] };
    const fetch = vi.fn(async (_url: string, _init: RequestInit) => json(201, reply));
    vi.stubGlobal("fetch", fetch);
    const posted = await postComment({ flare: "f1", parent: "c1", body: "Yes.", key: "k1" });
    expect(posted).toEqual({
      kind: "reply",
      id: "r1",
      parent: "c1",
      author: { name: "ana.okafor@example.com" },
      body: "Yes. All three do.",
      at: "2026-11-21T16:30:00Z",
    });
    const [url, init] = fetch.mock.calls[0] ?? [];
    expect(url).toBe("/api/comments");
    expect(init?.method).toBe("POST");
    expect(init?.credentials).toBe("same-origin");
    expect(new Headers(init?.headers).get("Content-Type")).toBe("application/json");
    expect(JSON.parse(String(init?.body))).toEqual({
      flare: "f1",
      parent: "c1",
      body: "Yes.",
      key: "k1",
    });
  });

  it("names a full flare apart from a refusal", async () => {
    vi.stubGlobal(
      "fetch",
      vi.fn(async () => json(409, { code: "flare::comments::too_many" })),
    );
    const failure = await postComment({ flare: "f1", body: "x", key: "k" }).catch(
      (error: unknown) => error,
    );
    expect(failure instanceof ApiError && failure.kind).toBe("full");
  });
});
